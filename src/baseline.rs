use std::collections::HashMap;

use crate::dedup::DedupedShape;
use crate::event::FieldValue;

fn field_value_key(v: &FieldValue) -> String {
    match v {
        FieldValue::Str(s) => s.clone(),
        FieldValue::Num(n) => n.to_string(),
        FieldValue::Bool(b) => b.to_string(),
    }
}

/// Per-field frequency table: field name -> (value -> occurrence count),
/// counted by shape occurrence count so it reflects true corpus frequency.
pub struct FieldFreq {
    pub tables: HashMap<String, HashMap<String, u64>>,
    pub totals: HashMap<String, u64>,
}

impl FieldFreq {
    pub fn build(shapes: &[DedupedShape]) -> Self {
        let mut tables: HashMap<String, HashMap<String, u64>> = HashMap::new();
        let mut totals: HashMap<String, u64> = HashMap::new();

        let mut bump = |field: &str, value: String, count: u64| {
            *tables
                .entry(field.to_string())
                .or_default()
                .entry(value)
                .or_insert(0) += count;
            *totals.entry(field.to_string()).or_insert(0) += count;
        };

        for shape in shapes {
            let e = &shape.representative;
            if let Some(actor) = &e.actor {
                bump("actor", actor.clone(), shape.count);
            }
            if let Some(action) = &e.action {
                bump("action", action.clone(), shape.count);
            }
            if let Some(status) = &e.status {
                bump("status", status.clone(), shape.count);
            }
            for (k, v) in &e.fields {
                if k == "tool_use_id" {
                    continue;
                }
                bump(&format!("fields.{k}"), field_value_key(v), shape.count);
            }
        }

        FieldFreq { tables, totals }
    }

    /// -log2(P(value|field)) with Laplace smoothing.
    pub fn surprisal(&self, field: &str, value: &str, smoothing: f64) -> f64 {
        let table = self.tables.get(field);
        let cardinality = table.map_or(0, |t| t.len()) as f64;
        let total = *self.totals.get(field).unwrap_or(&0) as f64;
        let count = table
            .and_then(|t| t.get(value))
            .copied()
            .unwrap_or(0) as f64;
        let p = (count + smoothing) / (total + smoothing * cardinality.max(1.0));
        -p.max(f64::MIN_POSITIVE).log2()
    }
}

/// First-order Markov table of action_i -> action_{i+1}, grouped per actor
/// (session), in timestamp order (fallback: dedup insertion order carries no
/// ordering, so transitions are only meaningful when timestamps exist).
pub struct TransitionFreq {
    pub tables: HashMap<String, HashMap<String, u64>>, // from -> (to -> count)
    pub totals: HashMap<String, u64>,                  // from -> total count
}

impl TransitionFreq {
    pub fn build(shapes: &[DedupedShape]) -> Self {
        // Reconstruct approximate per-actor sequences from deduped
        // representatives ordered by first_seen. This loses exact-repeat
        // transitions (already collapsed by dedup) but preserves the set of
        // distinct transitions that occurred, which is what novelty
        // detection needs.
        let mut by_actor: HashMap<String, Vec<(&DedupedShape, i64)>> = HashMap::new();
        for shape in shapes {
            let actor = shape
                .representative
                .actor
                .clone()
                .unwrap_or_else(|| "_none".to_string());
            let ts = shape.first_seen.unwrap_or(0);
            by_actor.entry(actor).or_default().push((shape, ts));
        }

        let mut tables: HashMap<String, HashMap<String, u64>> = HashMap::new();
        let mut totals: HashMap<String, u64> = HashMap::new();

        for seq in by_actor.values_mut() {
            seq.sort_by_key(|(_, ts)| *ts);
            for pair in seq.windows(2) {
                let (from_shape, _) = pair[0];
                let (to_shape, _) = pair[1];
                let (Some(from), Some(to)) = (
                    &from_shape.representative.action,
                    &to_shape.representative.action,
                ) else {
                    continue;
                };
                let weight = to_shape.count;
                *tables
                    .entry(from.clone())
                    .or_default()
                    .entry(to.clone())
                    .or_insert(0) += weight;
                *totals.entry(from.clone()).or_insert(0) += weight;
            }
        }

        TransitionFreq { tables, totals }
    }

    pub fn surprisal(&self, from: &str, to: &str, smoothing: f64) -> f64 {
        let table = self.tables.get(from);
        let cardinality = table.map_or(0, |t| t.len()) as f64;
        let total = *self.totals.get(from).unwrap_or(&0) as f64;
        let count = table.and_then(|t| t.get(to)).copied().unwrap_or(0) as f64;
        let p = (count + smoothing) / (total + smoothing * cardinality.max(1.0));
        -p.max(f64::MIN_POSITIVE).log2()
    }
}

/// Per-action duration quantiles (p50/p90/p99), for scoring how far a
/// shape's duration sits from its action's typical band.
pub struct TimingStats {
    pub quantiles: HashMap<String, (f64, f64, f64)>, // action -> (p50, p90, p99)
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

impl TimingStats {
    pub fn build(shapes: &[DedupedShape]) -> Self {
        let mut by_action: HashMap<String, Vec<f64>> = HashMap::new();
        for shape in shapes {
            let e = &shape.representative;
            if let (Some(action), Some(d)) = (&e.action, e.duration_ms) {
                by_action
                    .entry(action.clone())
                    .or_default()
                    .extend(std::iter::repeat(d).take(shape.count as usize));
            }
        }
        let mut quantiles = HashMap::new();
        for (action, mut durations) in by_action {
            durations.sort_by(|a, b| a.total_cmp(b));
            quantiles.insert(
                action,
                (
                    quantile(&durations, 0.50),
                    quantile(&durations, 0.90),
                    quantile(&durations, 0.99),
                ),
            );
        }
        TimingStats { quantiles }
    }

    /// 0 if within p1-p99 band (approximated as within p99 of the median
    /// direction), else normalized distance beyond p99.
    pub fn deviation(&self, action: &str, duration_ms: f64) -> f64 {
        let Some(&(p50, _p90, p99)) = self.quantiles.get(action) else {
            return 0.0;
        };
        if p99 <= 0.0 {
            return 0.0;
        }
        if duration_ms <= p99 {
            0.0
        } else {
            ((duration_ms - p50) / p99).max(0.0)
        }
    }
}
