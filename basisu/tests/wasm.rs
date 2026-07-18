//! wasm32 runtime tests. These prove the transcoder actually runs under wasm
//! (container parse, codebook decode, the lazy table init, and the
//! per-format transcode), not just that it compiles. They run in Node via
//! `wasm-bindgen-test-runner`, so no browser is required:
//!
//!   cargo test -p basisu --target wasm32-unknown-unknown --test wasm
//!
//! Byte-for-byte correctness is covered on the host by the golden test; wasm
//! runs the identical pure-Rust code, so here we exercise the full pipeline and
//! assert the output length matches `output_size` for every target and level.

#![cfg(target_arch = "wasm32")]

use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};
use wasm_bindgen_test::wasm_bindgen_test;

/// KTX2 fixture with an ETC1S (BasisLZ) payload.
const ETC1S: &[u8] = include_bytes!("fixtures/etc1s.ktx2");
/// KTX2 fixture with a UASTC LDR payload.
const UASTC: &[u8] = include_bytes!("fixtures/uastc.ktx2");
/// `.basis` fixture with an ETC1S payload.
const BASIS: &[u8] = include_bytes!("fixtures/etc1s.basis");

/// The four targets the typical consumer selects from device capabilities.
const TARGETS: [TargetFormat; 4] = [
    TargetFormat::Astc4x4Rgba,
    TargetFormat::Bc7Rgba,
    TargetFormat::Etc2Rgba,
    TargetFormat::Rgba32,
];

/// An ETC1S KTX2 file opens and transcodes to all four targets, with the
/// expected output size each time.
#[wasm_bindgen_test]
fn etc1s_transcodes_under_wasm() {
    let t = Transcoder::new(ETC1S).expect("open etc1s.ktx2");
    assert_eq!(t.source_format(), SourceFormat::Etc1s);
    for target in TARGETS {
        let out = t
            .transcode(0, target, DecodeFlags::NONE)
            .expect("transcode");
        assert_eq!(out.len(), t.output_size(0, target).unwrap());
        assert!(!out.is_empty());
    }
}

/// A UASTC KTX2 file opens and transcodes to all four targets, with the
/// expected output size each time.
#[wasm_bindgen_test]
fn uastc_transcodes_under_wasm() {
    let t = Transcoder::new(UASTC).expect("open uastc.ktx2");
    assert_eq!(t.source_format(), SourceFormat::UastcLdr);
    for target in TARGETS {
        let out = t
            .transcode(0, target, DecodeFlags::NONE)
            .expect("transcode");
        assert_eq!(out.len(), t.output_size(0, target).unwrap());
    }
}

/// Every mip level transcodes, exercising the level-index walk under wasm.
#[wasm_bindgen_test]
fn all_levels_transcode_under_wasm() {
    let t = Transcoder::new(ETC1S).unwrap();
    for level in 0..t.level_count() {
        let out = t
            .transcode(level, TargetFormat::Rgba32, DecodeFlags::NONE)
            .expect("level transcode");
        assert_eq!(
            out.len(),
            t.output_size(level, TargetFormat::Rgba32).unwrap()
        );
    }
}

/// A `.basis` container (not `.ktx2`) opens via the same `Transcoder::new`
/// auto-detect and transcodes under wasm.
#[wasm_bindgen_test]
fn dot_basis_transcodes_under_wasm() {
    let t = Transcoder::new(BASIS).expect("open etc1s.basis");
    assert_eq!(t.source_format(), SourceFormat::Etc1s);
    for target in TARGETS {
        let out = t
            .transcode(0, target, DecodeFlags::NONE)
            .expect("transcode");
        assert_eq!(out.len(), t.output_size(0, target).unwrap());
        assert!(!out.is_empty());
    }
}

/// The HIGH_QUALITY UASTC path (a distinct code path) also runs under wasm.
#[wasm_bindgen_test]
fn uastc_high_quality_bc7_under_wasm() {
    let t = Transcoder::new(UASTC).unwrap();
    let hq = t
        .transcode(0, TargetFormat::Bc7Rgba, DecodeFlags::HIGH_QUALITY)
        .expect("hq transcode");
    assert_eq!(hq.len(), t.output_size(0, TargetFormat::Bc7Rgba).unwrap());
}
