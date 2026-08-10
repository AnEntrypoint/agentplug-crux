use std::path::Path;

use crate::ncd::ncd;
use crate::score::ScoredShape;

/// Cap on file size compared for NCD. This is NOT primarily a cost bound
/// (gzip itself is cheap) -- it is a correctness bound: DEFLATE's sliding
/// window is a hard-capped 32KB, so once two files (or the shared region
/// between them) exceed a few multiples of that window, the compressor
/// genuinely cannot see the similarity anymore and NCD saturates toward
/// 1.0 regardless of how similar the files actually are. Confirmed
/// empirically: two real files 179,821 and 180,004 bytes, ~99% textually
/// identical (a handful of changed lines in an otherwise-identical
/// ~1300-line shader), scored NCD=0.982 (near-maximal "different") before
/// this cap existed -- a false negative caused by file size, not a flaw in
/// the pair. 96KB (3x the DEFLATE window) is a deliberately conservative
/// cap that keeps NCD meaningful; files above it are simply never
/// compared (no near_duplicates entry), not an error -- see the crate
/// README for the tradeoff this implies (near-dup detection covers
/// small-to-medium files reliably, large-file near-duplication is a
/// known gap, not silently wrong).
const MAX_COMPARE_BYTES: u64 = 96 * 1024;

pub struct NearDuplicate {
    pub path: String,
    pub ncd: f64,
}

/// For each selected files-mode shape, finds other files in the corpus
/// whose content compresses near-identically to it (NCD below
/// `threshold`) -- the signal exact-shape hashing structurally cannot see,
/// since two files differing by even one byte hash completely
/// differently, while near-duplicates (a copy-pasted config with one
/// field changed, a vendored variant of the same source) compress
/// together almost as well as either compresses alone.
///
/// Bounded to O(selected * corpus), never O(corpus^2): only the shapes
/// crux already selected as individually rare get compared against the
/// full file list, not every file against every other file. Within that,
/// a same-extension + same-order-of-magnitude-size prefilter skips the
/// gzip calls entirely for pairs NCD could not plausibly call near-dup
/// anyway (two files of very different size cannot compress
/// near-identically together by the metric's own definition), which is
/// the dominant cost saving in practice -- gzip itself is cheap per call,
/// but a full corpus of thousands of files times a handful of selected
/// shapes without this filter still adds up.
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
                // Same-order-of-magnitude prefilter: NCD cannot score near
                // a very size-mismatched pair as similar (the larger
                // file's own compressed size alone exceeds the ratio a low
                // NCD requires), so skip the read+compress entirely rather
                // than compute and discard.
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
