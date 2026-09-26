use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::skiplist;

#[derive(Clone, Copy)]
pub enum SkipMode {
    ContentIngest,
    StructuralScan,
}

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
