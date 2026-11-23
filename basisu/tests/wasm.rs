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

use basisu::{DecodeFlags, Error, SourceFormat, TargetFormat, Transcoder};
use wasm_bindgen_test::wasm_bindgen_test;

/// KTX2 fixture with an ETC1S (BasisLZ) payload.
const ETC1S: &[u8] = include_bytes!("fixtures/etc1s.ktx2");
/// KTX2 fixture with a UASTC LDR payload.
const UASTC: &[u8] = include_bytes!("fixtures/uastc.ktx2");
/// `.basis` fixture with an ETC1S payload.
const BASIS: &[u8] = include_bytes!("fixtures/etc1s.basis");
/// The kazmaps-gl fuzz crash texture: ETC1S KTX2 whose supercompression global
/// data offset is `u64::MAX` (see `tests/api.rs`).
const FUZZ_SGD_OFFSET_MAX: &[u8] = include_bytes!("fixtures/fuzz_etc1s_sgd_offset_max.ktx2");
/// A UASTC HDR 4x4 KTX2 whose one level is Zstandard-supercompressed.
const UASTC_HDR_ZSTD: &[u8] = include_bytes!("../../corpus/smoke/uastc_hdr_zstd.ktx2");

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

/// KTX2 header fields the offset test patches: `layerCount` (u32),
/// `sgdByteOffset` (u64), and level 0's `byteOffset` (u64).
const KTX2_LAYER_COUNT: usize = 32;
const KTX2_SGD_BYTE_OFFSET: usize = 64;
const KTX2_LEVEL0_BYTE_OFFSET: usize = 80;

/// The little-endian u64 at byte `at` of `file`.
fn read_u64(file: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(file[at..at + 8].try_into().unwrap())
}

/// A copy of `file` with the little-endian u64 at byte `at` set to `value`.
fn with_u64(file: &[u8], at: usize, value: u64) -> Vec<u8> {
    let mut data = file.to_vec();
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
    data
}

/// A copy of `file` with the little-endian u32 at byte `at` set to `value`.
fn with_u32(file: &[u8], at: usize, value: u32) -> Vec<u8> {
    let mut data = file.to_vec();
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    data
}

/// `usize` is 32 bits here. A u64 file offset past 4 GiB is refused rather
/// than cut to its low bits (which read the codebooks or a level from the
/// wrong place without an error), and a sum of an in-file offset and a u32
/// length or count cannot wrap: each of these used to panic or misread. The
/// host runs the same cases in `tests/api.rs`.
#[wasm_bindgen_test]
fn offsets_and_sums_past_4_gib_are_refused_under_wasm() {
    let sgd = read_u64(ETC1S, KTX2_SGD_BYTE_OFFSET);
    let refused = [
        FUZZ_SGD_OFFSET_MAX.to_vec(),
        with_u64(ETC1S, KTX2_SGD_BYTE_OFFSET, u64::MAX),
        with_u64(ETC1S, KTX2_SGD_BYTE_OFFSET, sgd + (1 << 32)),
        with_u32(ETC1S, sgd as usize + 4, u32::MAX),
        with_u32(ETC1S, KTX2_LAYER_COUNT, u32::MAX),
    ];
    for data in refused {
        assert!(Transcoder::new(&data).is_err());
    }
    for file in [ETC1S, UASTC] {
        let level0 = read_u64(file, KTX2_LEVEL0_BYTE_OFFSET);
        let data = with_u64(file, KTX2_LEVEL0_BYTE_OFFSET, level0 + (1 << 32));
        let t = Transcoder::new(&data).unwrap();
        assert!(t
            .transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE)
            .is_err());
    }
}

/// KTX2 header fields the dimension test patches: `pixelWidth` and
/// `pixelHeight` (u32 each).
const KTX2_PIXEL_WIDTH: usize = 20;
const KTX2_PIXEL_HEIGHT: usize = 24;

/// `usize` is 32 bits here. 32768 x 32768 RGBA32 is 2^30 pixels, a count that
/// fits, but 4 GiB, a byte size that does not: `output_size` multiplied it
/// unchecked (a panic with overflow checks, a 0-byte buffer in release). It
/// is refused, and the transcode fails the same way before allocating. The
/// host runs the count cases in `tests/api.rs`.
#[wasm_bindgen_test]
fn an_output_size_past_usize_is_refused_under_wasm() {
    let dims = |side: u32| {
        let data = with_u32(UASTC, KTX2_PIXEL_WIDTH, side);
        with_u32(&data, KTX2_PIXEL_HEIGHT, side)
    };
    let data = dims(32768);
    let t = Transcoder::new(&data).unwrap();
    assert_eq!(
        t.output_size(0, TargetFormat::Rgba32),
        Err(Error::InvalidData)
    );
    assert_eq!(
        t.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE),
        Err(Error::InvalidData)
    );
    let data = dims(16384);
    let t = Transcoder::new(&data).unwrap();
    assert_eq!(t.output_size(0, TargetFormat::Rgba32), Ok(1 << 30));
}

/// A zstd level's `uncompressedByteLength` sized the inflate buffer as is:
/// here a length just under 4 GiB fits `usize`, and the buffer it asked for
/// cannot exist. It must equal the size the level's dimensions imply, so the
/// level is refused before anything is allocated. The host runs this in
/// `tests/api.rs`.
#[wasm_bindgen_test]
fn a_zstd_length_past_the_level_size_is_refused_under_wasm() {
    let ulen = KTX2_LEVEL0_BYTE_OFFSET + 16;
    let data = with_u64(UASTC_HDR_ZSTD, ulen, u64::from(u32::MAX) - 15);
    let t = Transcoder::new(&data).unwrap();
    assert!(t
        .transcode(0, TargetFormat::RgbaHalf, DecodeFlags::NONE)
        .is_err());
    let t = Transcoder::new(UASTC_HDR_ZSTD).unwrap();
    assert!(t
        .transcode(0, TargetFormat::RgbaHalf, DecodeFlags::NONE)
        .is_ok());
}
