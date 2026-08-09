use crate::baseline::{FieldFreq, TimingStats, TransitionFreq};
use crate::dedup::DedupedShape;
use crate::event::FieldValue;

#[derive(Debug, Clone, Copy)]
pub struct Weights {
    pub field: f64,
    pub transition: f64,
    pub timing: f64,
    pub count: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            field: 1.0,
            transition: 1.5,
            timing: 1.0,
            count: 0.5,
        }
    }
}

pub struct ScoredShape<'a> {
    pub shape: &'a DedupedShape,
    pub score: f64,
    pub field_score: f64,
    pub transition_score: f64,
    pub timing_score: f64,
    pub count_score: f64,
}

fn field_value_key(v: &FieldValue) -> String {
    match v {
        FieldValue::Str(s) => s.clone(),
        FieldValue::Num(n) => n.to_string(),
        FieldValue::Bool(b) => b.to_string(),
    }
}

pub fn score_shapes<'a>(
    shapes: &'a [DedupedShape],
    field_freq: &FieldFreq,
    transition_freq: &TransitionFreq,
    timing: &TimingStats,
    weights: Weights,
    smoothing: f64,
    total_occurrences: u64,
) -> Vec<ScoredShape<'a>> {
    shapes
        .iter()
        .map(|shape| {
            let e = &shape.representative;

            let mut field_score = 0.0;
            if let Some(actor) = &e.actor {
                field_score += field_freq.surprisal("actor", actor, smoothing);
            }
            if let Some(action) = &e.action {
                field_score += field_freq.surprisal("action", action, smoothing);
            }
            if let Some(status) = &e.status {
                field_score += field_freq.surprisal("status", status, smoothing);
            }
            for (k, v) in &e.fields {
                if k == "tool_use_id" {
                    continue;
                }
                field_score +=
                    field_freq.surprisal(&format!("fields.{k}"), &field_value_key(v), smoothing);
            }

            // Transition surprisal needs a "from" action; approximate by
            // scoring this shape's action as a "to" against every "from" it
            // was observed following. Since DedupedShape does not retain
            // predecessor identity post-collapse, fall back to the max
            // surprisal of this action appearing as a transition target
            // across the whole table, which still flags actions that only
            // ever showed up in rare transitions.
            let transition_score = e
                .action
                .as_deref()
                .map(|to| {
                    transition_freq
                        .tables
                        .keys()
                        .map(|from| transition_freq.surprisal(from, to, smoothing))
                        .fold(0.0_f64, f64::max)
                })
                .unwrap_or(0.0)
                .min(20.0); // cap: a to-value absent everywhere shouldn't dominate

            let timing_score = match (&e.action, e.duration_ms) {
                (Some(action), Some(d)) => timing.deviation(action, d),
                _ => 0.0,
            };

            let count_score = if total_occurrences > 0 {
                -((shape.count as f64) / (total_occurrences as f64))
                    .max(f64::MIN_POSITIVE)
                    .log2()
            } else {
                0.0
            };

            let score = weights.field * field_score
                + weights.transition * transition_score
                + weights.timing * timing_score
                + weights.count * count_score;

            ScoredShape {
                shape,
                score,
                field_score,
                transition_score,
                timing_score,
                count_score,
            }
        })
        .collect()
}

pub fn select<'a>(
    mut scored: Vec<ScoredShape<'a>>,
    percentile: f64,
    min: usize,
    max: usize,
) -> Vec<ScoredShape<'a>> {
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let n = scored.len();
    if n == 0 {
        return scored;
    }
    let keep_frac = ((100.0 - percentile) / 100.0 * n as f64).ceil() as usize;
    let keep = keep_frac.clamp(min.min(n), max.min(n).max(min.min(n)));
    scored.truncate(keep.max(1).min(n));
    scored
}
