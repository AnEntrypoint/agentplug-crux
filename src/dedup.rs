use std::collections::HashMap;

use xxhash_rust::xxh3::Xxh3;
use std::hash::Hasher;

use crate::event::{CanonicalEvent, FieldValue};

/// Log-scale duration buckets so nearby latencies collapse into the same
/// shape instead of every millisecond producing a distinct shape.
fn quantize_duration(ms: f64) -> &'static str {
    match ms {
        d if d < 1.0 => "0-1ms",
        d if d < 10.0 => "1-10ms",
        d if d < 100.0 => "10-100ms",
        d if d < 1_000.0 => "100-1000ms",
        d if d < 10_000.0 => "1-10s",
        d if d < 60_000.0 => "10-60s",
        _ => "60s+",
    }
}

fn shape_hash(e: &CanonicalEvent) -> u64 {
    let mut h = Xxh3::new();
    h.write(e.actor.as_deref().unwrap_or("").as_bytes());
    h.write(e.action.as_deref().unwrap_or("").as_bytes());
    h.write(e.status.as_deref().unwrap_or("").as_bytes());
    if let Some(d) = e.duration_ms {
        h.write(quantize_duration(d).as_bytes());
    }
    for (k, v) in &e.fields {
        if k == "tool_use_id" {
            continue;
        }
        h.write(k.as_bytes());
        match v {
            FieldValue::Str(s) => h.write(s.as_bytes()),
            FieldValue::Num(n) => h.write(&n.to_bits().to_le_bytes()),
            FieldValue::Bool(b) => h.write(&[*b as u8]),
        }
    }
    h.finish()
}

pub struct DedupedShape {
    pub shape_hash: u64,
    pub count: u64,
    pub first_seen: Option<i64>,
    pub last_seen: Option<i64>,
    pub representative: CanonicalEvent,
}

pub fn dedup(events: impl Iterator<Item = CanonicalEvent>) -> Vec<DedupedShape> {
    let mut shapes: HashMap<u64, DedupedShape> = HashMap::new();
    for e in events {
        let h = shape_hash(&e);
        match shapes.get_mut(&h) {
            Some(shape) => {
                shape.count += 1;
                if let Some(ts) = e.timestamp_ms {
                    shape.first_seen = Some(shape.first_seen.map_or(ts, |f| f.min(ts)));
                    shape.last_seen = Some(shape.last_seen.map_or(ts, |l| l.max(ts)));
                }
            }
            None => {
                let ts = e.timestamp_ms;
                shapes.insert(
                    h,
                    DedupedShape {
                        shape_hash: h,
                        count: 1,
                        first_seen: ts,
                        last_seen: ts,
                        representative: e,
                    },
                );
            }
        }
    }
    shapes.into_values().collect()
}
