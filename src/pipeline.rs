//! wasm32-wasip1-only: the `scan` verb handler tying ingest/dedup/baseline/
//! score/emit together into one plugin dispatch.
#![cfg(target_arch = "wasm32")]

use serde_json::Value;

use crate::abi::{host_cwd_string, return_json};
use crate::baseline::{FieldFreq, TimingStats, TransitionFreq};
use crate::emit::{dump_as_values, manifest_top_values, score_range, Manifest, WeightsOut};
use crate::score::Weights;

fn f64_field(body: &Value, key: &str, default: f64) -> f64 {
    body.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn usize_field(body: &Value, key: &str, default: usize) -> usize {
    body.get(key).and_then(Value::as_u64).map(|v| v as usize).unwrap_or(default)
}

/// `scan` verb: recursively finds `.jsonl` files under `body.root` (relative
/// to the calling project's root, default "."), concentrates rare/surprising
/// shapes out of them, and returns `{ok, dump, manifest}` in one call --
/// there is no persistent process to stream results across, so a plugin
/// dispatch does the whole ingest/dedup/baseline/score/select pipeline in
/// one shot and returns the finished dump inline.
pub fn handle_scan(body: &Value) -> u64 {
    let cwd = host_cwd_string();
    let requested_root = body.get("root").and_then(Value::as_str).unwrap_or(".");
    let root = if requested_root == "." || requested_root.is_empty() {
        cwd.clone()
    } else if requested_root.starts_with('/') || requested_root.contains(':') {
        requested_root.to_string()
    } else {
        format!("{}/{}", cwd.trim_end_matches('/'), requested_root)
    };

    let max_files = usize_field(body, "max_files", 50_000);
    let context_window = usize_field(body, "context_window", 3);
    let events = crate::wasm_ingest::scan_project(&root, max_files);
    let raw_events = events.len();

    let shapes = crate::dedup::dedup(events.iter().cloned());
    let shapes_after_dedup = shapes.len();
    let total_occurrences: u64 = shapes.iter().map(|s| s.count).sum();

    let field_freq = FieldFreq::build(&shapes);
    let transition_freq = TransitionFreq::build(&shapes);
    let timing = TimingStats::build(&shapes);

    let weights = Weights {
        field: f64_field(body, "weight_field", 1.0),
        transition: f64_field(body, "weight_transition", 1.5),
        timing: f64_field(body, "weight_timing", 1.0),
        count: f64_field(body, "weight_count", 0.5),
    };
    let smoothing = f64_field(body, "smoothing", 1.0);
    let select_percentile = f64_field(body, "select_percentile", 99.0);
    let select_min = usize_field(body, "select_min", 10);
    let select_max = usize_field(body, "select_max", 500);

    let scored = crate::score::score_shapes(
        &shapes,
        &field_freq,
        &transition_freq,
        &timing,
        weights,
        smoothing,
        total_occurrences,
    );
    let selected = crate::score::select(scored, select_percentile, select_min, select_max);

    let distinct_actions = field_freq.tables.get("action").map_or(0, |t| t.len());
    let distinct_actors = field_freq.tables.get("actor").map_or(0, |t| t.len());
    let (top_actions_by_occurrence, top_actors_by_occurrence) = manifest_top_values(&field_freq, 10);
    let shapes_selected = selected.len();
    let dump = dump_as_values(&selected, &events, context_window, &[]);
    let score_range = score_range(&selected);

    let manifest = Manifest {
        input_path: root,
        raw_events,
        shapes_after_dedup,
        shapes_selected,
        distinct_actions,
        distinct_actors,
        top_actions_by_occurrence,
        top_actors_by_occurrence,
        score_range,
        weights: WeightsOut {
            field: weights.field,
            transition: weights.transition,
            timing: weights.timing,
            count: weights.count,
        },
        select_percentile,
        smoothing,
    };

    return_json(serde_json::json!({
        "ok": true,
        "dump": dump,
        "manifest": manifest,
    }))
}
