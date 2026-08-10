use std::fs::File;
use std::io::{self, BufWriter};
use std::path::PathBuf;

use clap::Parser;

use agentplug_crux::baseline::{FieldFreq, TimingStats, TransitionFreq};
use agentplug_crux::score::Weights;
use agentplug_crux::{dedup, emit, ingest_files_mode, ingest_gitlog, native_ingest, score};

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Mode {
    /// .jsonl trace files: one event per record (the original use case --
    /// session transcripts, workflow/orchestrator logs).
    Jsonl,
    /// Any directory: one event per file (extension, size, depth, dir
    /// name), no file content parsing -- works on any codebase or file
    /// tree, surfaces structural outliers (a wildly rare extension, a file
    /// far outside its extension's typical size band).
    Files,
    /// A git repository: one event per (commit, changed file) via
    /// `git log --numstat` -- surfaces history outliers (unusually large
    /// changes, rare authorship/file-type combinations).
    Gitlog,
}

/// crux: concentrate rare/surprising material out of large, low-signal-density corpora.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Input paths (files or directories, recursed) to scan.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// What an "event" is: jsonl trace records, per-file structural
    /// metadata over any codebase, or git commit history.
    #[arg(long, value_enum, default_value_t = Mode::Jsonl)]
    mode: Mode,
    /// gitlog mode only: how many recent commits to scan.
    #[arg(long, default_value_t = 5000)]
    max_commits: usize,

    /// Laplace smoothing constant applied to all frequency tables.
    #[arg(long, default_value_t = 1.0)]
    smoothing: f64,

    /// Weight for field-value surprisal.
    #[arg(long, default_value_t = 1.0)]
    weight_field: f64,
    /// Weight for transition surprisal.
    #[arg(long, default_value_t = 1.5)]
    weight_transition: f64,
    /// Weight for timing deviation.
    #[arg(long, default_value_t = 1.0)]
    weight_timing: f64,
    /// Weight for count rarity.
    #[arg(long, default_value_t = 0.5)]
    weight_count: f64,

    /// Percentile threshold for selection (top (100-p)% of shapes by score).
    #[arg(long, default_value_t = 99.0)]
    select_percentile: f64,
    /// Minimum number of shapes to select regardless of percentile.
    #[arg(long, default_value_t = 10)]
    select_min: usize,
    /// Maximum number of shapes to select regardless of percentile.
    #[arg(long, default_value_t = 500)]
    select_max: usize,

    /// Output path for the JSONL dump (default: stdout).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Output path for the run manifest.
    #[arg(long, default_value = "crux-manifest.json")]
    manifest: PathBuf,

    /// Events immediately before/after each selected shape (same source
    /// file) to include as context, 0 to disable.
    #[arg(long, default_value_t = 3)]
    context_window: usize,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let mut all_events = Vec::new();
    for input in &args.inputs {
        match args.mode {
            Mode::Jsonl => all_events.extend(native_ingest::ingest_jsonl_path(input)),
            Mode::Files => all_events.extend(ingest_files_mode::scan_codebase(input)),
            Mode::Gitlog => all_events.extend(ingest_gitlog::scan_git_log(input, args.max_commits)),
        }
    }
    let raw_events = all_events.len();

    let shapes = dedup::dedup(all_events.iter().cloned());
    let total_occurrences: u64 = shapes.iter().map(|s| s.count).sum();

    let field_freq = FieldFreq::build(&shapes);
    let transition_freq = TransitionFreq::build(&shapes);
    let timing = TimingStats::build(&shapes);

    let weights = Weights {
        field: args.weight_field,
        transition: args.weight_transition,
        timing: args.weight_timing,
        count: args.weight_count,
    };

    let scored = score::score_shapes(
        &shapes,
        &field_freq,
        &transition_freq,
        &timing,
        weights,
        args.smoothing,
        total_occurrences,
    );

    let distinct_actions = field_freq.tables.get("action").map_or(0, |t| t.len());
    let distinct_actors = field_freq.tables.get("actor").map_or(0, |t| t.len());
    let (top_actions_by_occurrence, top_actors_by_occurrence) =
        emit::manifest_top_values(&field_freq, 10);

    let selected = score::select(
        scored,
        args.select_percentile,
        args.select_min,
        args.select_max,
    );
    let shapes_selected = selected.len();
    let score_range = emit::score_range(&selected);

    match &args.out {
        Some(path) => {
            let f = BufWriter::new(File::create(path)?);
            emit::write_jsonl(f, &selected, &all_events, args.context_window)?;
        }
        None => {
            let stdout = io::stdout();
            emit::write_jsonl(stdout.lock(), &selected, &all_events, args.context_window)?;
        }
    }

    let manifest = emit::Manifest {
        input_path: args
            .inputs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(","),
        raw_events,
        shapes_after_dedup: shapes.len(),
        shapes_selected,
        distinct_actions,
        distinct_actors,
        top_actions_by_occurrence,
        top_actors_by_occurrence,
        score_range,
        weights: emit::WeightsOut {
            field: weights.field,
            transition: weights.transition,
            timing: weights.timing,
            count: weights.count,
        },
        select_percentile: args.select_percentile,
        smoothing: args.smoothing,
    };
    let manifest_file = BufWriter::new(File::create(&args.manifest)?);
    emit::write_manifest(manifest_file, &manifest)?;

    eprintln!(
        "crux: {} raw events -> {} shapes -> {} selected (manifest: {})",
        manifest.raw_events,
        manifest.shapes_after_dedup,
        manifest.shapes_selected,
        args.manifest.display()
    );
    if manifest.raw_events == 0 {
        let hint = match args.mode {
            Mode::Jsonl => "no .jsonl files found under the given path(s)",
            Mode::Files => "no files found under the given path(s) after skip-list/.gitignore filtering",
            Mode::Gitlog => "no commits found -- is this path a git repository, and is `git` on PATH?",
        };
        eprintln!("crux: zero events ingested ({hint})");
    }

    Ok(())
}
