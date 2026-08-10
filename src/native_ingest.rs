use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use ignore::WalkBuilder;

use crate::durations::attach_durations;
use crate::event::CanonicalEvent;
use crate::normalize::normalize_line;
use crate::skiplist;

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

/// Recursively finds `.jsonl` files under `root`, skipping the same
/// directories/files gm's own code-index skips (see `skiplist`) and
/// honoring `.gitignore` the same way `ignore::WalkBuilder` does for any
/// other gm-adjacent tool -- so a directory crux scans and a directory gm
/// scans agree on what counts as noise.
fn find_jsonl_files(root: &Path) -> Vec<std::path::PathBuf> {
    WalkBuilder::new(root)
        .hidden(false) // skiplist below decides hidden-segment skipping itself
        .filter_entry(|e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            let name = e.file_name().to_string_lossy();
            if is_dir {
                !skiplist::is_hidden_segment(&name) && !skiplist::is_skipped_dir_segment(&name)
            } else {
                !skiplist::is_skipped_filename(&name)
            }
        })
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Accepts either a single `.jsonl` file or a directory (recursed for
/// `.jsonl` files) and yields canonical events, in file-then-line order.
/// Malformed lines are skipped, not fatal: a corpus this size always has a
/// few truncated tail lines from an interrupted write.
pub fn ingest_path(path: &Path) -> Box<dyn Iterator<Item = CanonicalEvent>> {
    if path.is_dir() {
        Box::new(ingest_files(find_jsonl_files(path)))
    } else {
        Box::new(ingest_files(vec![path.to_path_buf()]))
    }
}
