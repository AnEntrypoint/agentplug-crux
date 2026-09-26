use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::event::{CanonicalEvent, FieldValue};
use crate::walk::{find_files, SkipMode};

fn quantize(n: u64) -> &'static str {
    match n {
        0 => "0",
        1..=9 => "1-9",
        10..=99 => "10-99",
        100..=999 => "100-999",
        1_000..=9_999 => "1K-10K",
        10_000..=99_999 => "10K-100K",
        100_000..=999_999 => "100K-1M",
        _ => "1M+",
    }
}

fn size_and_line_count(path: &Path) -> Option<(u64, Option<u64>)> {
    let file = File::open(path).ok()?;
    let size_bytes = file.metadata().ok()?.len();
    let mut reader = BufReader::new(file);
    let mut buf = [0u8; 8192];
    let mut newlines: u64 = 0;
    let mut any_bytes = false;
    let mut first_chunk = true;

    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return Some((size_bytes, None)),
        };
        any_bytes = true;
        if first_chunk {
            first_chunk = false;
            if buf[..n].contains(&0) {
                return Some((size_bytes, None)); // binary-looking
            }
        }
        newlines += buf[..n].iter().filter(|&&b| b == b'\n').count() as u64;
    }

    let line_count = if any_bytes { Some(newlines + 1) } else { Some(0) };
    Some((size_bytes, line_count))
}

pub fn find_codebase_files(root: &Path) -> Vec<std::path::PathBuf> {
    find_files(root, SkipMode::StructuralScan)
}

pub fn scan_codebase_files(root: &Path, files: &[std::path::PathBuf]) -> Vec<CanonicalEvent> {
    files
        .iter()
        .filter_map(|path| {
            let rel = path.strip_prefix(root).unwrap_or(path.as_path());
            let mut components = rel.components();
            let top_level = components
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_else(|| "(root)".to_string());
            let depth = rel.components().count() as u64;
            let dir_name = rel
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "(root)".to_string());
            let ext = rel
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_else(|| "(no-ext)".to_string());

            let (size_bytes, line_count) = size_and_line_count(path)?;

            let mut fields: BTreeMap<String, FieldValue> = BTreeMap::new();
            fields.insert("depth".into(), FieldValue::Num(depth as f64));
            fields.insert("dir_name".into(), FieldValue::Str(dir_name));
            fields.insert(
                "size_bucket".into(),
                FieldValue::Str(quantize(size_bytes).to_string()),
            );
            if let Some(lc) = line_count {
                fields.insert(
                    "line_count_bucket".into(),
                    FieldValue::Str(quantize(lc).to_string()),
                );
            }

            Some(CanonicalEvent {
                source_file: path.to_string_lossy().to_string(),
                source_line: 1,
                timestamp_ms: None,
                timestamp_raw: None,
                actor: Some(top_level),
                action: Some(ext),
                status: None,
                duration_ms: Some(size_bytes as f64),
                fields,
            })
        })
        .collect()
}
