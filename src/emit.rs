use std::io::Write;

use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::event::CanonicalEvent;
use crate::score::ScoredShape;

fn format_ms(ms: Option<i64>) -> Option<String> {
    let ms = ms?;
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
}

/// Which score component contributed the largest share of the total, so a
/// consumer can sort/filter by "why this is rare" without recomputing the
/// breakdown itself. Names the loudest signal, states no opinion on
/// whether that's good or bad.
fn dominant_signal(field: f64, transition: f64, timing: f64, count: f64) -> &'static str {
    let mut best = ("field", field);
    for candidate in [("transition", transition), ("timing", timing), ("count", count)] {
        if candidate.1 > best.1 {
            best = candidate;
        }
    }
    best.0
}

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
    rank: usize,
    score: f64,
    dominant_signal: &'static str,
    score_breakdown: ScoreBreakdown,
    occurrence_count: u64,
    first_seen_ms: Option<i64>,
    first_seen: Option<String>,
    last_seen_ms: Option<i64>,
    last_seen: Option<String>,
    representative_event: &'a CanonicalEvent,
    source: Source,
}

#[derive(Serialize)]
struct SchemaMeta {
    #[serde(rename = "__meta")]
    marker: &'static str,
    description: &'static str,
    fields: SchemaFields,
    dominant_signal_values: [&'static str; 4],
}

#[derive(Serialize)]
struct SchemaFields {
    shape_id: &'static str,
    rank: &'static str,
    score: &'static str,
    dominant_signal: &'static str,
    score_breakdown: &'static str,
    occurrence_count: &'static str,
    first_seen: &'static str,
    last_seen: &'static str,
    representative_event: &'static str,
    source: &'static str,
}

fn schema_meta() -> SchemaMeta {
    SchemaMeta {
        marker: "crux-dump-v1",
        description: "One line per statistically rare deduped event shape, ranked highest-score first. No field expresses an opinion on whether a shape is good or bad -- only how rare it is relative to this corpus's own baseline. See crux-manifest.json (same run) for corpus-level context.",
        fields: SchemaFields {
            shape_id: "Stable hash of this shape's (actor, action, status, quantized duration, fields) tuple -- same shape across runs on the same corpus gets the same id.",
            rank: "1-indexed position in this dump, highest score first.",
            score: "Weighted sum of the four score_breakdown components. Higher = rarer relative to corpus baseline.",
            dominant_signal: "Name of the score_breakdown component with the largest value for this shape -- which axis made it rare, not what that means.",
            score_breakdown: "field: sum of -log2(P(value|field)) across actor/action/status/fields.*. transition: max -log2(P(this action as a transition target)) over all observed predecessor actions. timing: normalized distance beyond this action's p99 duration, 0 if within band. count: -log2(occurrence_count / total_corpus_occurrences), rarer shapes score higher.",
            occurrence_count: "How many raw events collapsed into this one deduped shape.",
            first_seen: "RFC3339 UTC, alongside the epoch-ms field, null if no event in this shape carried a parseable timestamp.",
            last_seen: "RFC3339 UTC, alongside the epoch-ms field, null if no event in this shape carried a parseable timestamp.",
            representative_event: "The first raw event that produced this shape, byte-faithful canonical form (actor/action/status/duration_ms/fields).",
            source: "File and 1-indexed line number the representative_event came from, for follow-up reads.",
        },
        dominant_signal_values: ["field", "transition", "timing", "count"],
    }
}

pub fn write_jsonl<W: Write>(mut out: W, scored: &[ScoredShape]) -> std::io::Result<()> {
    serde_json::to_writer(&mut out, &schema_meta())?;
    writeln!(out)?;
    for (i, s) in scored.iter().enumerate() {
        let entry = DumpEntry {
            shape_id: format!("{:016x}", s.shape.shape_hash),
            rank: i + 1,
            score: s.score,
            dominant_signal: dominant_signal(
                s.field_score,
                s.transition_score,
                s.timing_score,
                s.count_score,
            ),
            score_breakdown: ScoreBreakdown {
                field: s.field_score,
                transition: s.transition_score,
                timing: s.timing_score,
                count: s.count_score,
            },
            occurrence_count: s.shape.count,
            first_seen_ms: s.shape.first_seen,
            first_seen: format_ms(s.shape.first_seen),
            last_seen_ms: s.shape.last_seen,
            last_seen: format_ms(s.shape.last_seen),
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

fn top_values(counts: &std::collections::HashMap<String, u64>, n: usize) -> Vec<(String, u64)> {
    let mut pairs: Vec<(String, u64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));
    pairs.truncate(n);
    pairs
}

#[derive(Serialize)]
pub struct Manifest {
    pub input_path: String,
    pub raw_events: usize,
    pub shapes_after_dedup: usize,
    pub shapes_selected: usize,
    pub distinct_actions: usize,
    pub distinct_actors: usize,
    pub top_actions_by_occurrence: Vec<(String, u64)>,
    pub top_actors_by_occurrence: Vec<(String, u64)>,
    pub score_range: (f64, f64),
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

pub fn manifest_top_values(
    field_freq: &crate::baseline::FieldFreq,
    n: usize,
) -> (Vec<(String, u64)>, Vec<(String, u64)>) {
    let actions = field_freq
        .tables
        .get("action")
        .map(|t| top_values(t, n))
        .unwrap_or_default();
    let actors = field_freq
        .tables
        .get("actor")
        .map(|t| top_values(t, n))
        .unwrap_or_default();
    (actions, actors)
}

pub fn score_range(scored: &[ScoredShape]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for s in scored {
        min = min.min(s.score);
        max = max.max(s.score);
    }
    if !min.is_finite() || !max.is_finite() {
        (0.0, 0.0)
    } else {
        (min, max)
    }
}

pub fn write_manifest<W: Write>(mut out: W, manifest: &Manifest) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(manifest)?;
    writeln!(out, "{s}")
}
