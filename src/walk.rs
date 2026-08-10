use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::skiplist;

#[derive(Clone, Copy)]
pub enum SkipMode {
    /// gm's own code-index list plus all hidden segments plus noise-file
    /// suffixes (binaries/media/lockfiles) -- appropriate when ingesting
    /// file *content* (jsonl records): a hidden or vendored file's content
    /// was never going to be a real trace record anyway.
    ContentIngest,
    /// Minimal VCS-internals/build-output-only list, hidden segments NOT
    /// blanket-excluded (`.github`, `.cargo`, etc. are real authored
    /// content on plenty of repos), noise-file suffixes NOT applied --
    /// appropriate when a file's own type/size/location is itself the
    /// thing being scored for rarity, so hiding it first defeats the
    /// point.
    StructuralScan,
}

/// Recursively walks `root`, applying `mode`'s skip rules plus
/// `.gitignore`, yielding every file found.
pub fn find_files(root: &Path, mode: SkipMode) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .hidden(false) // skiplist decides hidden-segment skipping itself, per mode
        .filter_entry(move |e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            let name = e.file_name().to_string_lossy();
            match mode {
                SkipMode::ContentIngest => {
                    if is_dir {
                        !skiplist::is_hidden_segment(&name) && !skiplist::is_skipped_dir_segment(&name)
                    } else {
                        !skiplist::is_skipped_filename(&name)
                    }
                }
                SkipMode::StructuralScan => {
                    // A skip name may be a real directory OR (submodule/
                    // worktree gitlink) a plain file with that exact name --
                    // `.git` is a one-line pointer file in that case, not a
                    // directory to recurse into, but it's still VCS
                    // internals, never authored content, and must be
                    // excluded either way.
                    !skiplist::is_structural_skip_dir(&name)
                }
            }
        })
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.path().to_path_buf())
        .collect()
}
