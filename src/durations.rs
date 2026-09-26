use std::collections::HashMap;

use crate::event::{CanonicalEvent, FieldValue};

pub fn attach_durations(events: &mut [CanonicalEvent]) {
    let mut pending: HashMap<String, (usize, i64)> = HashMap::new();
    for i in 0..events.len() {
        let id = match events[i].fields.get("tool_use_id") {
            Some(FieldValue::Str(s)) => Some(s.clone()),
            _ => None,
        };
        let Some(id) = id else { continue };
        let is_result = events[i].action.as_deref() == Some("tool_result");
        if is_result {
            if let Some((_start_idx, start_ts)) = pending.remove(&id) {
                if let Some(end_ts) = events[i].timestamp_ms {
                    events[i].duration_ms = Some((end_ts - start_ts).max(0) as f64);
                }
            }
        } else if let Some(ts) = events[i].timestamp_ms {
            pending.insert(id, (i, ts));
        }
    }
}
