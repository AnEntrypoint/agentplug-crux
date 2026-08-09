use std::io::Write;

use serde::Serialize;

use crate::event::CanonicalEvent;
use crate::score::ScoredShape;

#[derive(Serialize)]
struct ScoreBreakdown {
    field: f64,
    transition: f64,
    timing: f64,
    count: f64,
}

#[derive(Serialize)]
struct Source {
    file: String,
    line: u64,
}

#[derive(Serialize)]
struct DumpEntry<'a> {
    shape_id: String,
    score: f64,
    score_breakdown: ScoreBreakdown,
    occurrence_count: u64,
    first_seen_ms: Option<i64>,
    last_seen_ms: Option<i64>,
    representative_event: &'a CanonicalEvent,
    source: Source,
}

pub fn write_jsonl<W: Write>(mut out: W, scored: &[ScoredShape]) -> std::io::Result<()> {
    for s in scored {
        let entry = DumpEntry {
            shape_id: format!("{:016x}", s.shape.shape_hash),
            score: s.score,
            score_breakdown: ScoreBreakdown {
                field: s.field_score,
                transition: s.transition_score,
                timing: s.timing_score,
                count: s.count_score,
            },
            occurrence_count: s.shape.count,
            first_seen_ms: s.shape.first_seen,
            last_seen_ms: s.shape.last_seen,
            representative_event: &s.shape.representative,
            source: Source {
                file: s.shape.representative.source_file.clone(),
                line: s.shape.representative.source_line,
            },
        };
        serde_json::to_writer(&mut out, &entry)?;
        writeln!(out)?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Manifest {
    pub input_path: String,
    pub raw_events: usize,
    pub shapes_after_dedup: usize,
    pub shapes_selected: usize,
    pub distinct_actions: usize,
    pub distinct_actors: usize,
    pub weights: WeightsOut,
    pub select_percentile: f64,
    pub smoothing: f64,
}

#[derive(Serialize)]
pub struct WeightsOut {
    pub field: f64,
    pub transition: f64,
    pub timing: f64,
    pub count: f64,
}

pub fn write_manifest<W: Write>(mut out: W, manifest: &Manifest) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(manifest)?;
    writeln!(out, "{s}")
}
