//! Bakes `goldens/manifest.tsv`: the SHA-256 of the C++ oracle's output for every
//! `Done` matrix case over the committed `corpus/smoke` assets. The pure-Rust
//! `basisu/tests/goldens.rs` replays these without a C++ toolchain, so
//! the published crate can verify itself byte-perfect after the oracle is retired.
//! Re-bake (and review the git diff) after any upstream re-vendor.
//!
//! Run: `cargo run -p conformance --bin bake-goldens` (or `cargo xtask bake-goldens`).
//!
//! Hashes, not raw bytes: byte-perfect verification (SHA-256) with a tiny,
//! reviewable, history-friendly manifest instead of tens of MB of binaries.

use basisu::{SourceFormat, Supercompression, Transcoder};
use conformance::matrix::{matrix, Status};
use conformance::{smoke_files, OracleBasis, OracleKtx2};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Oracle transcode for one container, by `(level, target_fmt, flags)`. Boxed so
/// the KTX2 and `.basis` oracles share one driver loop.
type OracleTranscode = Box<dyn Fn(u32, i32, u32) -> Option<Vec<u8>>>;

/// Hash every `Done` matrix case for every smoke asset and write the manifest.
fn main() {
    let cases: Vec<_> = matrix()
        .into_iter()
        .filter(|c| c.status == Status::Done)
        .collect();

    let mut tsv = String::new();
    tsv.push_str(
        "# Golden hashes of the C++ oracle output, baked by `cargo xtask bake-goldens`.\n",
    );
    tsv.push_str("# basisu/tests/goldens.rs replays these in pure Rust (no C++).\n");
    tsv.push_str("# Re-bake after re-vendoring upstream; review the diff.\n");
    tsv.push_str("# asset\ttarget\tflags\tlevel\tlen\tsha256\n");

    let mut rows = 0usize;
    for path in smoke_files() {
        let data = std::fs::read(&path).expect("read smoke asset");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let tex = Transcoder::new(&data).expect("rust open smoke asset");

        // The manifest is the complete pure-Rust byte gate for the smoke set,
        // so skips are limited to what the replay cannot express, and every
        // skip is printed: a silently dropped asset would leave the zstd decode
        // path with no byte-level coverage at all.
        let source = tex.source_format();
        let sc = tex.supercompression();
        let sc_ok = match source {
            SourceFormat::Etc1s => sc == Supercompression::BasisLz,
            // Zstandard-supercompressed levels are decoded by the pure-Rust
            // `zstd` feature at replay time, so they belong in the manifest.
            SourceFormat::UastcLdr
            | SourceFormat::UastcHdr4x4
            | SourceFormat::AstcLdr(_)
            | SourceFormat::AstcHdr6x6 => {
                sc == Supercompression::None || sc == Supercompression::Zstandard
            }
            // The intermediate streams are their own coding; the pure-Rust
            // replay decodes them directly (KTX2 schemes 4/5 report Other).
            SourceFormat::UastcHdr6x6 => {
                sc == Supercompression::None || sc == Supercompression::Other(4)
            }
            // XUASTC: the Khronos scheme (5) or the old-style v1.6/v2.0
            // presentation under the BasisLZ scheme id.
            SourceFormat::XuastcLdr(_) => {
                sc == Supercompression::None
                    || sc == Supercompression::Other(5)
                    || sc == Supercompression::BasisLz
            }
            // `SourceFormat` is non-exhaustive: a codec this baker does not
            // know yet must fail the assert below, not silently bake.
            _ => false,
        };
        assert!(
            sc_ok,
            "smoke asset {name} has unexpected supercompression {sc:?}"
        );
        assert!(
            sc != Supercompression::Zstandard || cfg!(oracle_zstd),
            "smoke asset {name} is zstd-supercompressed but the oracle was built \
             without zstd, so it cannot produce its ground truth; baking would \
             silently drop the asset from the manifest. Install a system zstd \
             (see README \"Development\") and re-run."
        );
        if tex.is_video() {
            eprintln!(
                "bake-goldens: skipping {name}: video needs stateful frame decode; \
                 pinned instead by the frame checksums in basisu/tests/api.rs \
                 and oracle-gated by the conformance suite"
            );
            continue;
        }
        if tex.layer_count() > 1 || tex.face_count() > 1 {
            eprintln!("bake-goldens: skipping {name}: the replay transcodes layer 0 / face 0 only");
            continue;
        }

        // Resolve the oracle for this container: KTX2 or .basis. Both yield the
        // image-0 mip levels and a per-level transcode the pure-Rust replay
        // reproduces byte-for-byte.
        let is_basis = path.extension().is_some_and(|x| x == "basis");
        let (levels, transcode): (u32, OracleTranscode) = if is_basis {
            let oracle = OracleBasis::open(&data).expect("oracle open .basis smoke asset");
            (
                oracle.info.levels,
                Box::new(move |level, fmt, flags| {
                    oracle.transcode(0, level, fmt, flags).map(|l| l.data)
                }),
            )
        } else {
            let oracle = OracleKtx2::open(&data).expect("oracle open .ktx2 smoke asset");
            (
                oracle.info.levels,
                Box::new(move |level, fmt, flags| {
                    oracle.transcode_flags(level, fmt, flags).map(|l| l.data)
                }),
            )
        };

        let rows_before = rows;
        for case in cases.iter().filter(|c| c.source == source) {
            for level in 0..levels {
                for &flags in case.flags {
                    let Some(out) = transcode(level, case.target.as_i32(), flags) else {
                        continue;
                    };
                    let mut hash = String::new();
                    for b in Sha256::digest(&out) {
                        let _ = write!(hash, "{b:02x}");
                    }
                    let _ = writeln!(
                        tsv,
                        "{name}\t{}\t{flags}\t{level}\t{}\t{hash}",
                        case.target.as_i32(),
                        out.len(),
                    );
                    rows += 1;
                }
            }
        }
        assert!(
            rows > rows_before,
            "smoke asset {name} produced no golden rows (oracle declined every case?)"
        );
    }

    let manifest =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../goldens/manifest.tsv");
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(&manifest, tsv).expect("write manifest");
    eprintln!("baked {rows} golden rows -> {}", manifest.display());
}
