use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::durations::attach_durations;
use crate::event::CanonicalEvent;
use crate::normalize::normalize_line;
use crate::walk::{find_files, SkipMode};

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

pub fn ingest_jsonl_path(path: &Path) -> Box<dyn Iterator<Item = CanonicalEvent>> {
    if path.is_dir() {
        let files: Vec<_> = find_files(path, SkipMode::ContentIngest)
            .into_iter()
            .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        Box::new(ingest_files(files))
    } else {
        Box::new(ingest_files(vec![path.to_path_buf()]))
    }
}
