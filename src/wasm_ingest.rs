//! wasm32-wasip1-only: directory scanning driven by host filesystem
//! imports instead of native std::fs (the plugin has no native fs access).
#![cfg(target_arch = "wasm32")]

use crate::abi::{host_read, host_readdir, host_stat};
use crate::durations::attach_durations;
use crate::event::CanonicalEvent;
use crate::normalize::normalize_line;
use crate::skiplist;

fn join(root: &str, name: &str) -> String {
    if root.is_empty() || root.ends_with('/') {
        format!("{root}{name}")
    } else {
        format!("{root}/{name}")
    }
}

fn load_gitignore(root: &str) -> Option<ignore::gitignore::Gitignore> {
    let content = host_read(&join(root, ".gitignore"))?;
    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
    for line in content.lines() {
        let _ = builder.add_line(None, line);
    }
    builder.build().ok()
}

fn gitignore_excludes(gi: &Option<ignore::gitignore::Gitignore>, rel_path: &str, is_dir: bool) -> bool {
    match gi {
        Some(g) => g.matched(rel_path, is_dir).is_ignore(),
        None => false,
    }
}

/// Recursively walks `root` via host filesystem imports, skipping the same
/// directories/files gm's own code-index skips and honoring `.gitignore`
/// at `root`, then yields canonical events from every `.jsonl` file found,
/// in file-then-line order.
pub fn scan_project(root: &str, max_files: usize) -> Vec<CanonicalEvent> {
    let gi = load_gitignore(root);
    let mut files = Vec::new();
    walk(root, &gi, max_files, &mut files);

    let mut all_events = Vec::new();
    for path in files {
        let Some(content) = host_read(&path) else { continue };
        let mut lines: Vec<CanonicalEvent> = content
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .flat_map(|(i, l)| normalize_line(&path, i as u64 + 1, l))
            .collect();
        attach_durations(&mut lines);
        all_events.extend(lines);
    }
    all_events
}

fn walk(root: &str, gi: &Option<ignore::gitignore::Gitignore>, max_files: usize, out: &mut Vec<String>) {
    if out.len() >= max_files {
        return;
    }
    for name in host_readdir(root) {
        if out.len() >= max_files {
            return;
        }
        let next = join(root, &name);
        let stat = host_stat(&next);
        let is_dir = stat.as_ref().map(|s| s.is_dir).unwrap_or(false);

        if is_dir {
            if skiplist::is_hidden_segment(&name) || skiplist::is_skipped_dir_segment(&name) {
                continue;
            }
            if gitignore_excludes(gi, &next, true) {
                continue;
            }
            walk(&next, gi, max_files, out);
        } else {
            if skiplist::is_skipped_filename(&name) {
                continue;
            }
            if !name.ends_with(".jsonl") {
                continue;
            }
            if gitignore_excludes(gi, &next, false) {
                continue;
            }
            out.push(next);
        }
    }
}
