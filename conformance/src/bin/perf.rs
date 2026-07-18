//! Apples-to-apples performance: the crate vs the same-version (v2_1_0) C++
//! oracle, both transcoding byte-identical output. Pinning both sides to one
//! upstream version isolates implementation speed from the algorithm changes
//! between v1.16 and v2 that a cross-version benchmark would conflate.
//! Measures the full per-texture cost (open + transcode level 0), min
//! wall-time over N runs, release.
//!
//!   cargo run -p conformance --release --bin perf

use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};
use conformance::OracleKtx2;
use std::path::PathBuf;
use std::time::Instant;

/// The benchmarked (label, target format) pairs; one totals row each.
const TARGETS: [(&str, TargetFormat); 4] = [
    ("BC7", TargetFormat::Bc7Rgba),
    ("ASTC4x4", TargetFormat::Astc4x4Rgba),
    ("ETC2", TargetFormat::Etc2Rgba),
    ("RGBA32", TargetFormat::Rgba32),
];

/// Minimum wall-time of `f` in milliseconds over `iters` runs, after 3 warm-up
/// runs. The minimum (rather than mean) discounts scheduler and cache noise,
/// which only ever slows a run down.
fn min_ms(iters: u32, mut f: impl FnMut()) -> f64 {
    for _ in 0..3 {
        f();
    }
    let mut best = f64::INFINITY;
    for _ in 0..iters {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64() * 1e3);
    }
    best
}

/// Time every (smoke corpus file, target) pair on both sides, verify the
/// outputs are byte-identical, and print per-file and total wall times.
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/smoke");
    let files = [
        "etc1s_alpha.ktx2",
        "kodim23.ktx2",
        "uastc_medium.ktx2",
        "etc1s.ktx2",
        "uastc.ktx2",
    ];

    println!(
        "{:<18} {:<6} {:<8} {:>10} {:>9} {:>9}",
        "file", "src", "target", "C++v2 ms", "Rust ms", "Rust/C++"
    );
    println!("{}", "-".repeat(66));

    let mut tot_cpp = [0.0f64; 4];
    let mut tot_rust = [0.0f64; 4];

    for fname in files {
        let data = std::fs::read(root.join(fname)).expect("read");
        let src = match Transcoder::new(&data).unwrap().source_format() {
            SourceFormat::Etc1s => "etc1s",
            _ => "uastc",
        };
        for (ti, (tname, target)) in TARGETS.iter().enumerate() {
            // Same-work proof: both sides produce identical bytes.
            let rust_out = Transcoder::new(&data)
                .unwrap()
                .transcode_image(0, 0, 0, *target, DecodeFlags::NONE)
                .unwrap();
            let cpp_out = OracleKtx2::open(&data)
                .unwrap()
                .transcode_image_flags(0, 0, 0, target.as_i32(), 0)
                .unwrap();
            let parity = if rust_out == cpp_out.data {
                ""
            } else {
                "  BYTES-DIFFER!"
            };

            let rust = min_ms(40, || {
                let t = Transcoder::new(&data).unwrap();
                let _ = t
                    .transcode_image(0, 0, 0, *target, DecodeFlags::NONE)
                    .unwrap();
            });
            let cpp = min_ms(40, || {
                let o = OracleKtx2::open(&data).unwrap();
                let _ = o
                    .transcode_image_flags(0, 0, 0, target.as_i32(), 0)
                    .unwrap();
            });
            tot_cpp[ti] += cpp;
            tot_rust[ti] += rust;
            println!(
                "{:<18} {:<6} {:<8} {:>10.3} {:>9.3} {:>8.2}x{}",
                fname,
                src,
                tname,
                cpp,
                rust,
                rust / cpp,
                parity
            );
        }
    }
    println!("{}", "-".repeat(66));
    for (ti, (tname, _)) in TARGETS.iter().enumerate() {
        println!(
            "{:<18} {:<6} {:<8} {:>10.3} {:>9.3} {:>8.2}x",
            "TOTAL",
            "",
            tname,
            tot_cpp[ti],
            tot_rust[ti],
            tot_rust[ti] / tot_cpp[ti]
        );
    }
}
