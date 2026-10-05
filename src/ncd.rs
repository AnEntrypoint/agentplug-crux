use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;

fn compressed_len(data: &[u8]) -> u64 {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let _ = encoder.write_all(data);
    encoder.finish().map(|v| v.len() as u64).unwrap_or(u64::MAX)
}

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
