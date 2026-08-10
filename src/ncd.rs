use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;

/// Compressed size of `data` under gzip -- the cheap proxy for
/// Kolmogorov complexity NCD is built on. Never fails: `GzEncoder` over an
/// in-memory `Vec<u8>` sink has no I/O to fail on.
fn compressed_len(data: &[u8]) -> u64 {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let _ = encoder.write_all(data);
    encoder.finish().map(|v| v.len() as u64).unwrap_or(u64::MAX)
}

/// Normalized Compression Distance: `(C(a+b) - min(C(a),C(b))) /
/// max(C(a),C(b))`, using gzip as the practical stand-in for an
/// incomputable universal compressor. 0.0 = compresses identically
/// together as apart (maximally similar), approaching 1.0 = no shared
/// structure a gzip window can exploit. This is the one place crux's
/// otherwise purely statistical/structural rarity gets a content-aware
/// signal: two files with different bytes but a mostly-shared body (a
/// copy-pasted config with one field changed, a vendored variant) look
/// completely unrelated to exact-shape hashing, but compress
/// near-identically together -- NCD is what surfaces that as "these are
/// basically the same thing" without parsing either file's format.
pub fn ncd(a: &[u8], b: &[u8]) -> f64 {
    let ca = compressed_len(a);
    let cb = compressed_len(b);
    let mut combined = Vec::with_capacity(a.len() + b.len());
    combined.extend_from_slice(a);
    combined.extend_from_slice(b);
    let cab = compressed_len(&combined);

    let min_c = ca.min(cb) as f64;
    let max_c = ca.max(cb) as f64;
    if max_c == 0.0 {
        return 0.0; // both inputs empty -- identically (non-)informative
    }
    ((cab as f64 - min_c) / max_c).clamp(0.0, 1.0)
}
