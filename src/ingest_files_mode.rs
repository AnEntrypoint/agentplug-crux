use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::event::{CanonicalEvent, FieldValue};
use crate::walk::{find_files, SkipMode};

/// Log-scale buckets, same shape as `dedup::quantize_duration`, so file
/// sizes/line counts collapse into shapes the same way durations do --
/// two files that are both "roughly 5KB" are the same shape, a file that's
/// 500x the corpus's typical size is not.
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

/// (size_bytes, line_count) from one streaming pass: a single open file
/// handle, size from its metadata (no separate `std::fs::metadata` stat
/// call), newlines counted incrementally through a fixed-size buffer so a
/// large file is never loaded into memory whole just to sniff/count it.
/// `line_count` is `None` when the first chunk looks binary (a NUL byte),
/// since a line count is meaningless there.
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

/// One `CanonicalEvent` per file: no content is read beyond a streaming
/// binary sniff and newline count (never the whole file into memory), so
/// this mode is cheap enough to run over an entire codebase including
/// binaries/lockfiles crux's jsonl mode would skip -- a file's own
/// type/size/location is exactly the kind of thing whose rarity is worth
/// scoring here, not noise to filter out first.
///
/// actor = top-level directory under root (crux's closest analog to
/// "which subsystem"), action = file extension (or "(no-ext)"), fields =
/// {depth, dir_name, size_bucket, line_count_bucket} (raw byte size and
/// line count are NOT stored uninterpreted in fields -- they're quantized
/// the same way duration_ms is, so shape dedup collapses "roughly the same
/// size" files instead of treating every distinct byte count as its own
/// shape). duration_ms carries the raw file size in bytes, reusing the
/// existing duration-quantile machinery in baseline.rs/score.rs for a
/// free "this file's size is way outside this extension's typical band"
/// signal -- crux has no dedicated "size" axis, but duration_ms's timing
/// quantiles/deviation are exactly that computation already built.
pub fn scan_codebase(root: &Path) -> Vec<CanonicalEvent> {
    find_files(root, SkipMode::StructuralScan)
        .into_iter()
        .filter_map(|path| {
            let rel = path.strip_prefix(root).unwrap_or(&path);
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

            let (size_bytes, line_count) = size_and_line_count(&path)?;

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
