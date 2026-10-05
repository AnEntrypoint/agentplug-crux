use std::path::Path;

use crate::ncd::ncd;
use crate::score::ScoredShape;

const MAX_COMPARE_BYTES: u64 = 96 * 1024;

pub struct NearDuplicate {
    pub path: String,
    pub ncd: f64,
}

pub fn find_near_duplicates<'a>(
    selected: &[ScoredShape<'a>],
    corpus_files: &[std::path::PathBuf],
    threshold: f64,
) -> Vec<Vec<NearDuplicate>> {
    if threshold <= 0.0 {
        return (0..selected.len()).map(|_| Vec::new()).collect();
    }

    selected
        .iter()
        .map(|s| {
            let target_path = Path::new(&s.shape.representative.source_file);
            let Ok(target_meta) = std::fs::metadata(target_path) else {
                return Vec::new();
            };
            let target_size = target_meta.len();
            if target_size == 0 || target_size > MAX_COMPARE_BYTES {
                return Vec::new();
            }
            let Ok(target_bytes) = std::fs::read(target_path) else {
                return Vec::new();
            };
            let target_ext = target_path.extension();

            let mut hits = Vec::new();
            for candidate in corpus_files {
                if candidate == target_path {
                    continue;
                }
                if candidate.extension() != target_ext {
                    continue;
                }
                let Ok(cand_meta) = std::fs::metadata(candidate) else { continue };
                let cand_size = cand_meta.len();
                if cand_size == 0 || cand_size > MAX_COMPARE_BYTES {
                    continue;
                }
                let (small, large) = if target_size < cand_size {
                    (target_size, cand_size)
                } else {
                    (cand_size, target_size)
                };
                if large > small.saturating_mul(10) {
                    continue;
                }

                let Ok(cand_bytes) = std::fs::read(candidate) else { continue };
                let d = ncd(&target_bytes, &cand_bytes);
                if d <= threshold {
                    hits.push(NearDuplicate {
                        path: candidate.to_string_lossy().into_owned(),
                        ncd: d,
                    });
                }
            }
            hits.sort_by(|a, b| a.ncd.total_cmp(&b.ncd));
            hits
        })
        .collect()
}
