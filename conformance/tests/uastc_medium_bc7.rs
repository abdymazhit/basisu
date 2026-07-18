//! Asserts the crate's UASTC->BC7 output on uastc_medium.ktx2 is byte-identical
//! to the C++ oracle, over every mip level and decode-flag variant. Flag 32
//! (HIGH_QUALITY) selects the slower, higher-quality BC7 packing and changes
//! the output bytes; flag 64 (NO_ETC1S_CHROMA_FILTERING) is ETC1S-only and must
//! be a no-op for UASTC, so it doubles as a regression guard. uastc_medium
//! exercises a wide spread of UASTC modes, so a single byte of divergence in
//! the mode dispatch or BC7 packing shows up here.

use basisu::{DecodeFlags, TargetFormat, Transcoder};
use conformance::OracleKtx2;
use std::path::PathBuf;

/// Transcode every mip level of uastc_medium.ktx2 to BC7 (flags 0, 32, and 64)
/// and assert each result is byte-identical to the oracle.
#[test]
fn uastc_medium_bc7_matches_v2_oracle() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/smoke/uastc_medium.ktx2");
    let data = std::fs::read(&path).expect("read uastc_medium.ktx2");
    let oracle = OracleKtx2::open(&data).expect("v2 oracle open");
    let tex = Transcoder::new(&data).expect("crate open");

    let target = TargetFormat::Bc7Rgba;
    let mut mismatches = 0;
    for flag in [0u32, 32, 64] {
        for level in 0..oracle.info.levels {
            let want = oracle
                .transcode_image_flags(level, 0, 0, target.as_i32(), flag)
                .unwrap_or_else(|| panic!("oracle rejected level {level} flags {flag}"));
            let got = tex
                .transcode_image(level, 0, 0, target, DecodeFlags::from_bits(flag))
                .expect("crate transcode");
            let diff = got.iter().zip(&want.data).filter(|(a, b)| a != b).count();
            let first = got.iter().zip(&want.data).position(|(a, b)| a != b);
            println!(
                "UASTC->BC7 L{level} flags={flag}: crate {} B vs v2-oracle {} B; {} bytes differ, first@{:?}",
                got.len(), want.data.len(), diff, first
            );
            if got != want.data {
                mismatches += 1;
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "crate UASTC->BC7 output diverges from the oracle"
    );
}
