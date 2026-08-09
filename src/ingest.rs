use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use walkdir::WalkDir;

use crate::event::{CanonicalEvent, FieldValue};
use crate::normalize::normalize_line;

/// Pairs `tool_use` events with their later `tool_result` by `tool_use_id`
/// (within one file/session) to derive `duration_ms`, since the transcript
/// never states it directly.
fn attach_durations(events: &mut [CanonicalEvent]) {
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

fn ingest_files(files: Vec<std::path::PathBuf>) -> impl Iterator<Item = CanonicalEvent> {
    files.into_iter().flat_map(|path| {
        let source_file = path.to_string_lossy().to_string();
        let reader = File::open(&path).ok().map(BufReader::new);
        let mut lines: Vec<CanonicalEvent> = match reader {
            Some(r) => r
                .lines()
                .enumerate()
                .filter_map(|(i, line)| line.ok().map(|l| (i as u64 + 1, l)))
                .filter(|(_, l)| !l.trim().is_empty())
                .flat_map(|(lineno, l)| normalize_line(&source_file, lineno, &l))
                .collect(),
            None => Vec::new(),
        };
        attach_durations(&mut lines);
        lines.into_iter()
    })
}

/// Accepts either a single `.jsonl` file or a directory (recursed for
/// `.jsonl` files) and yields canonical events, in file-then-line order.
/// Malformed lines are skipped, not fatal: a corpus this size always has a
/// few truncated tail lines from an interrupted write.
pub fn ingest_path(path: &Path) -> Box<dyn Iterator<Item = CanonicalEvent>> {
    if path.is_dir() {
        let files: Vec<_> = WalkDir::new(path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .map(|e| e.path().to_path_buf())
            .collect();
        Box::new(ingest_files(files))
    } else {
        Box::new(ingest_files(vec![path.to_path_buf()]))
    }
}
