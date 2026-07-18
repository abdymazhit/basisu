//! Transcode throughput benchmarks. Use criterion baselines to compare two
//! runs on the same machine:
//!   cargo bench -p basisu -- --save-baseline <name>
//!   cargo bench -p basisu -- --baseline <name>
//!
//! Two groups: end-to-end transcode throughput per (asset, target), and the
//! one-time container parse plus codebook decode cost (the `open` group).
//! Absolute numbers are machine-specific, so only compare runs from the same
//! machine.

use basisu::{DecodeFlags, TargetFormat, Transcoder};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::PathBuf;

/// Benchmark corpus: a short label and the smoke-corpus file it names.
const ASSETS: &[(&str, &str)] = &[
    ("etc1s_kodim", "kodim23.ktx2"),
    ("etc1s_alpha", "etc1s_alpha.ktx2"),
    ("uastc", "uastc_medium.ktx2"),
];

/// Transcode targets exercised per asset; unsupported ones are skipped.
const TARGETS: &[(&str, TargetFormat)] = &[
    ("ASTC", TargetFormat::Astc4x4Rgba),
    ("BC7", TargetFormat::Bc7Rgba),
    ("ETC2", TargetFormat::Etc2Rgba),
    ("RGBA32", TargetFormat::Rgba32),
];

/// Read one smoke-corpus asset by file name, panicking if it cannot be read.
fn asset_bytes(file: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus/smoke")
        .join(file);
    std::fs::read(&p).unwrap_or_else(|e| panic!("read bench asset {p:?}: {e}"))
}

/// Benchmark full-image transcode for each asset against every supported
/// target, reporting throughput in blocks per second.
fn bench_transcode(c: &mut Criterion) {
    for (label, file) in ASSETS {
        let data = asset_bytes(file);
        let tex = Transcoder::new(&data).expect("open bench asset");
        let info = tex.image_level_info(0).expect("level 0 info");
        let blocks = (info.num_blocks_x * info.num_blocks_y) as u64;

        let mut g = c.benchmark_group(format!("transcode/{label}"));
        g.throughput(Throughput::Elements(blocks));
        for (tname, target) in TARGETS {
            if !tex.supports(*target) {
                continue;
            }
            g.bench_with_input(BenchmarkId::from_parameter(tname), target, |b, &t| {
                b.iter(|| tex.transcode(0, t, DecodeFlags::NONE).unwrap());
            });
        }
        g.finish();
    }
}

/// Benchmark the one-time open cost: container parse plus codebook decode.
fn bench_open(c: &mut Criterion) {
    let mut g = c.benchmark_group("open");
    for (label, file) in ASSETS {
        let data = asset_bytes(file);
        g.bench_function(*label, |b| {
            b.iter(|| Transcoder::new(std::hint::black_box(&data)).unwrap());
        });
    }
    g.finish();
}

criterion_group!(benches, bench_transcode, bench_open);
criterion_main!(benches);
