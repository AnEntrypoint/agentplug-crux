use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

use time::OffsetDateTime;

use crate::event::{CanonicalEvent, FieldValue};

fn quantize(n: u64) -> &'static str {
    match n {
        0 => "0",
        1..=9 => "1-9",
        10..=99 => "10-99",
        100..=999 => "100-999",
        1_000..=9_999 => "1K-10K",
        _ => "10K+",
    }
}

/// One `CanonicalEvent` per (commit, changed file) pair, via
/// `git log --numstat`. actor=author, action=file extension changed,
/// status=change kind (added/modified/deleted, derived from
/// insertions/deletions being zero -- a factual read of the numstat
/// output, not an opinion), fields={insertions_bucket, deletions_bucket,
/// commit_hash, dir_name}, duration_ms=insertions+deletions (reuses the
/// same timing-quantile machinery as the file-metadata mode for a "this
/// change was way outside the typical size for this file type" signal).
/// timestamp=commit time.
///
/// Native-only: shells out to the `git` binary, which the wasm plugin
/// sandbox has no access to (no subprocess imports in agentplug's ABI).
pub fn scan_git_log(repo_root: &Path, max_commits: usize) -> Vec<CanonicalEvent> {
    let output = Command::new("git")
        .args([
            "log",
            &format!("-n{max_commits}"),
            "--no-merges",
            "--numstat",
            "--format=%H%x09%ae%x09%aI",
        ])
        .current_dir(repo_root)
        .output();

    let Ok(output) = output else { return Vec::new() };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut events = Vec::new();
    let mut current: Option<(String, String, Option<i64>)> = None; // (hash, author, ts_ms)

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some((hash, rest)) = line.split_once('\t') {
            if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Some((author, ts_raw)) = rest.split_once('\t') {
                    let ts_ms = OffsetDateTime::parse(ts_raw, &time::format_description::well_known::Rfc3339)
                        .ok()
                        .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64);
                    current = Some((hash.to_string(), author.to_string(), ts_ms));
                    continue;
                }
            }
        }

        // numstat line: "<insertions>\t<deletions>\t<path>" (insertions/
        // deletions are "-" for a binary file diff, treated as 0 -- git
        // genuinely cannot count line changes in a binary, this is not a
        // guess).
        let Some((hash, author, ts_ms)) = &current else { continue };
        let mut parts = line.splitn(3, '\t');
        let (Some(ins_s), Some(del_s), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let insertions: u64 = ins_s.parse().unwrap_or(0);
        let deletions: u64 = del_s.parse().unwrap_or(0);

        let status = if insertions > 0 && deletions == 0 {
            "added-or-modified"
        } else if insertions == 0 && deletions > 0 {
            "deleted-or-modified"
        } else {
            "modified"
        };

        let path_buf = std::path::Path::new(path);
        let ext = path_buf
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(no-ext)".to_string());
        let dir_name = path_buf
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(root)".to_string());

        let mut fields: BTreeMap<String, FieldValue> = BTreeMap::new();
        fields.insert("insertions_bucket".into(), FieldValue::Str(quantize(insertions).to_string()));
        fields.insert("deletions_bucket".into(), FieldValue::Str(quantize(deletions).to_string()));
        fields.insert("commit_hash".into(), FieldValue::Str(hash.clone()));
        fields.insert("dir_name".into(), FieldValue::Str(dir_name));

        events.push(CanonicalEvent {
            source_file: path.to_string(),
            source_line: 0, // placeholder, assigned below
            timestamp_ms: *ts_ms,
            timestamp_raw: None,
            actor: Some(author.clone()),
            action: Some(ext),
            status: Some(status.to_string()),
            duration_ms: Some((insertions + deletions) as f64),
            fields,
        });
    }

    assign_per_file_sequence(&mut events);
    events
}

/// `git log` emits commits newest-first, so events arrive in reverse
/// chronological order per file. `source_line` is crux's only ordering
/// key for context windows (see `context.rs`), and jsonl mode's
/// convention is ascending = chronological -- so number each file's
/// occurrences 1..N in ascending timestamp order (oldest first), giving
/// every event a source_line unique within its file and making "next
/// event in the same file" mean "the next change to this file
/// chronologically", not an arbitrary tie among same-valued entries.
fn assign_per_file_sequence(events: &mut [CanonicalEvent]) {
    let mut indices_by_file: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in events.iter().enumerate() {
        indices_by_file.entry(e.source_file.clone()).or_default().push(i);
    }
    for indices in indices_by_file.into_values() {
        let mut sorted = indices.clone();
        sorted.sort_by_key(|&i| events[i].timestamp_ms);
        for (line, &i) in sorted.iter().enumerate() {
            events[i].source_line = line as u64 + 1;
        }
    }
}
