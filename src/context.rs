use std::collections::HashMap;

use crate::event::CanonicalEvent;
use crate::score::ScoredShape;

/// Builds a `source_file -> events in that file, sorted by source_line`
/// index once, so every selected shape's context window is a slice lookup
/// instead of a fresh scan of the whole corpus.
pub struct ContextIndex<'a> {
    by_file: HashMap<&'a str, Vec<&'a CanonicalEvent>>,
}

impl<'a> ContextIndex<'a> {
    pub fn build(events: &'a [CanonicalEvent]) -> Self {
        let mut by_file: HashMap<&'a str, Vec<&'a CanonicalEvent>> = HashMap::new();
        for e in events {
            by_file.entry(e.source_file.as_str()).or_default().push(e);
        }
        for v in by_file.values_mut() {
            v.sort_by_key(|e| e.source_line);
        }
        ContextIndex { by_file }
    }

    /// Up to `window` events immediately before and after `(source_file,
    /// source_line)` in the same file, split into (before, after), each in
    /// original order. Empty on either side if the shape sits at a file
    /// boundary or its file/line was not found (e.g. a representative event
    /// whose exact line no longer matches after dedup's first-seen pick --
    /// should not happen in practice, but context is scaffolding, not
    /// signal, so a miss degrades to "no context" rather than an error).
    pub fn window(
        &self,
        source_file: &str,
        source_line: u64,
        window: usize,
    ) -> (Vec<&'a CanonicalEvent>, Vec<&'a CanonicalEvent>) {
        let Some(events) = self.by_file.get(source_file) else {
            return (Vec::new(), Vec::new());
        };
        let Some(idx) = events.iter().position(|e| e.source_line == source_line) else {
            return (Vec::new(), Vec::new());
        };
        let before_start = idx.saturating_sub(window);
        let before = events[before_start..idx].to_vec();
        let after_end = (idx + 1 + window).min(events.len());
        let after = events[idx + 1..after_end].to_vec();
        (before, after)
    }
}

/// Two selected shapes cross-reference when their representative events
/// share an actor (crux's actor is always the session id, so this is
/// "occurred in the same session") -- the cheapest resolvable session/trace
/// key, matching the spec's "same session key" cross-reference case without
/// needing a separate trace-key config.
pub fn cross_references<'a>(scored: &'a [ScoredShape<'a>]) -> Vec<Vec<String>> {
    let mut by_actor: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, s) in scored.iter().enumerate() {
        if let Some(actor) = s.shape.representative.actor.as_deref() {
            by_actor.entry(actor).or_default().push(i);
        }
    }

    let mut refs: Vec<Vec<String>> = vec![Vec::new(); scored.len()];
    for indices in by_actor.values() {
        if indices.len() < 2 {
            continue;
        }
        for &i in indices {
            refs[i] = indices
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| format!("{:016x}", scored[j].shape.shape_hash))
                .collect();
        }
    }
    refs
}
