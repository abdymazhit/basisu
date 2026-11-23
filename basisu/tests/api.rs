//! Public-API integration tests over the committed smoke fixtures, plus
//! robustness checks: malformed, truncated, and adversarial input must return
//! an error, never panic. The byte-for-byte correctness of the output is the
//! golden test's job; this covers the consumer-facing surface and failure
//! modes.

#![cfg(not(target_arch = "wasm32"))]

use basisu::transcoder::Ktx2Transcoder;
use basisu::{DecodeFlags, Error, SourceFormat, TargetFormat, Transcoder};

// KTX2 smoke fixtures, one per source codec.
const ETC1S: &[u8] = include_bytes!("fixtures/etc1s.ktx2");
const UASTC: &[u8] = include_bytes!("fixtures/uastc.ktx2");
// A single-level 256x256 UASTC KTX2, the base the dimension tests build on.
const UASTC_MEDIUM: &[u8] = include_bytes!("../../corpus/smoke/uastc_medium.ktx2");

// `.basis` container smoke fixtures: an ETC1S file, an ETC1S+alpha file, and a
// UASTC file. The same payloads `Transcoder::new` must auto-detect and open
// exactly like KTX2.
const BASIS_ETC1S: &[u8] = include_bytes!("../../corpus/smoke/kodim20.basis");
const BASIS_ETC1S_ALPHA: &[u8] = include_bytes!("../../corpus/smoke/alpha3.basis");
const BASIS_UASTC: &[u8] = include_bytes!("../../corpus/smoke/kodim03_uastc.basis");

// The two layouts of the intermediate formats' per-image slice descs in the
// supercompression global data: UASTC HDR 6x6 (scheme 4, 12-byte descs, five
// levels) and XUASTC LDR in the old-style scheme-1 presentation (8-byte descs).
const UASTC_HDR_6X6_MIP: &[u8] = include_bytes!("../../corpus/smoke/uastc_hdr_6x6i_mip.ktx2");
const XUASTC_OLD_SCHEME: &[u8] = include_bytes!("../../corpus/smoke/xuastc_5x4_oldscheme.ktx2");

// The texture of a kazmaps-gl fuzz crash (target `glb_tile_etc1s`, input
// `crash-f197dff0687e8dd9af95c39670d2249ccf130ac1`): the KTX2 image's
// bufferView cut out of the fuzzed GLB as is. An 8x8 ETC1S texture with four
// levels whose supercompression global data offset is `u64::MAX`.
const FUZZ_SGD_OFFSET_MAX: &[u8] = include_bytes!("fixtures/fuzz_etc1s_sgd_offset_max.ktx2");

// An ETC1S video (`tex_type` video frames, P-frame conditional replenishment).
// Decodes through the stateful video API, one frame at a time in order; the
// stateless entry points reject it.
const BASIS_VIDEO: &[u8] = include_bytes!("fixtures/video.basis");

/// The 12-byte KTX2 file identifier.
const KTX2_MAGIC: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

/// Both KTX2 fixtures open, report the right source codec, and expose sane
/// dimensions and level counts.
#[test]
fn opens_and_reports_metadata() {
    let t = Transcoder::new(ETC1S).unwrap();
    assert_eq!(t.source_format(), SourceFormat::Etc1s);
    let (w, h) = t.base_dimensions();
    assert!(w > 0 && h > 0);
    assert!(t.level_count() >= 1);

    let u = Transcoder::new(UASTC).unwrap();
    assert_eq!(u.source_format(), SourceFormat::UastcLdr);
}

/// `transcode` and `output_size` must agree, so callers can size buffers up
/// front for both block-compressed and raster targets.
#[test]
fn transcode_len_matches_output_size_for_every_supported_target() {
    let t = Transcoder::new(ETC1S).unwrap();
    let targets = [
        TargetFormat::Bc1Rgb,
        TargetFormat::Bc3Rgba,
        TargetFormat::Bc7Rgba,
        TargetFormat::Etc2Rgba,
        TargetFormat::Astc4x4Rgba,
        TargetFormat::AtcRgb,
        TargetFormat::Rgba32,
    ];
    for target in targets {
        if !t.supports(target) {
            continue;
        }
        let out = t.transcode(0, target, DecodeFlags::NONE).unwrap();
        assert_eq!(out.len(), t.output_size(0, target).unwrap(), "{target:?}");
        assert!(!out.is_empty(), "{target:?}");
    }
}

/// The caller-buffer path (`transcode_into`) produces the same bytes as the
/// allocating path.
#[test]
#[cfg(feature = "bc")]
fn transcode_into_matches_transcode() {
    let t = Transcoder::new(UASTC).unwrap();
    let target = TargetFormat::Bc7Rgba;
    let owned = t.transcode(0, target, DecodeFlags::NONE).unwrap();

    let mut buf = vec![0u8; t.output_size(0, target).unwrap()];
    t.transcode_into(0, target, DecodeFlags::NONE, &mut buf)
        .unwrap();
    assert_eq!(owned, buf);
}

/// `transcode_into` writes directly into the caller's buffer, so it must not
/// depend on that buffer arriving zeroed: a dirty, oversized buffer gets the
/// exact same `output_size` bytes as the allocating path, and the tail beyond
/// them is left untouched. Runs a block target, a raster target, and the
/// zero-padded FXT1 layout over both source codecs' fixtures.
#[test]
fn transcode_into_dirty_buffer_matches_and_leaves_tail() {
    let cases: [(&[u8], TargetFormat); 5] = [
        (UASTC, TargetFormat::Bc7Rgba),
        (UASTC, TargetFormat::Rgba32),
        (ETC1S, TargetFormat::Etc2Rgba),
        (ETC1S, TargetFormat::Rgb565),
        (ETC1S, TargetFormat::Fxt1Rgb),
    ];
    for (data, target) in cases {
        let t = Transcoder::new(data).unwrap();
        if !t.supports(target) {
            continue;
        }
        let owned = t.transcode(0, target, DecodeFlags::NONE).unwrap();
        let needed = t.output_size(0, target).unwrap();
        assert_eq!(owned.len(), needed, "{target:?}");

        let mut buf = vec![0xAAu8; needed + 32];
        t.transcode_into(0, target, DecodeFlags::NONE, &mut buf)
            .unwrap();
        assert_eq!(owned, buf[..needed], "{target:?}: dirty-buffer bytes");
        assert!(
            buf[needed..].iter().all(|&b| b == 0xAA),
            "{target:?}: tail past output_size must be untouched"
        );
    }
}

/// A codec family left out of the build (its Cargo feature off) is reported
/// unsupported and refused with `Error::Unsupported`; it is never decoded
/// through another family's path. RGBA32 is always compiled.
#[test]
fn a_codec_family_left_out_of_the_build_is_refused() {
    let t = Transcoder::new(ETC1S).unwrap();
    let families = [
        (TargetFormat::AtcRgb, cfg!(feature = "atc")),
        (TargetFormat::Pvrtc2_4Rgb, cfg!(feature = "pvrtc2")),
        (TargetFormat::Pvrtc1_4Rgb, cfg!(feature = "pvrtc1")),
        (TargetFormat::Fxt1Rgb, cfg!(feature = "fxt1")),
        (TargetFormat::EacR11, cfg!(feature = "eac")),
        (TargetFormat::Rgb565, cfg!(feature = "packed")),
        (TargetFormat::Bc7Rgba, cfg!(feature = "bc")),
        (TargetFormat::Etc2Rgba, cfg!(feature = "etc")),
        (TargetFormat::Astc4x4Rgba, cfg!(feature = "astc")),
        (TargetFormat::Rgba32, true),
    ];
    for (target, compiled) in families {
        assert_eq!(t.supports(target), compiled, "{target:?}");
        // A compiled pair may still fail on the fixture itself (PVRTC1 wants
        // power-of-two dimensions); what it must not do is refuse the pair.
        let refused = matches!(
            t.transcode(0, target, DecodeFlags::NONE),
            Err(Error::Unsupported { .. })
        );
        assert_eq!(refused, !compiled, "{target:?}");
    }
}

/// An undersized output buffer fails with `OutputTooSmall` carrying the exact
/// size required.
#[test]
#[cfg(feature = "bc")]
fn output_too_small_reports_needed() {
    let t = Transcoder::new(UASTC).unwrap();
    let target = TargetFormat::Bc7Rgba;
    let mut tiny = [0u8; 4];
    match t.transcode_into(0, target, DecodeFlags::NONE, &mut tiny) {
        Err(Error::OutputTooSmall { needed }) => {
            assert_eq!(needed, t.output_size(0, target).unwrap());
        }
        other => panic!("expected OutputTooSmall, got {other:?}"),
    }
}

/// A level index past the image's mip count is a clean `InvalidImageOrLevel`.
#[test]
fn out_of_range_level_errors() {
    let t = Transcoder::new(ETC1S).unwrap();
    assert!(matches!(
        t.transcode(9999, TargetFormat::Rgba32, DecodeFlags::NONE),
        Err(Error::InvalidImageOrLevel)
    ));
}

/// Bytes that are not any supported container (including empty and tiny
/// inputs) fail with an error instead of panicking.
#[test]
fn non_ktx2_input_errors_cleanly() {
    assert!(matches!(
        Transcoder::new(b"definitely not a ktx2 file"),
        Err(Error::InvalidData)
    ));
    assert!(Transcoder::new(&[]).is_err());
    assert!(Transcoder::new(&[0u8; 3]).is_err());
}

/// Every prefix of a valid file must error or open, but never panic.
#[test]
fn truncated_input_never_panics() {
    for n in 0..=ETC1S.len() {
        let _ = Transcoder::new(&ETC1S[..n]);
    }
    for n in 0..=UASTC.len() {
        let _ = Transcoder::new(&UASTC[..n]);
    }
}

/// A correct KTX2 identifier followed by junk: the parser must fail on the
/// bogus header/level-index rather than index out of bounds.
#[test]
fn valid_magic_with_garbage_body_never_panics() {
    for tail_len in [0usize, 16, 80, 200, 1024] {
        let mut data = KTX2_MAGIC.to_vec();
        data.extend(std::iter::repeat_n(0xCD, tail_len));
        let _ = Transcoder::new(&data);
    }
}

/// KTX2 header fields the offset tests below patch: `layerCount` (u32),
/// `sgdByteOffset` (u64), and level 0's `byteOffset` (u64), the first field
/// of the level index.
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

/// The supercompression global data offset is a u64 straight from the file.
/// Near `u64::MAX` the end of the ETC1S global data header (`offset + 20`)
/// overflowed: a panic with overflow checks, and in release a wrapped sum
/// that passed the bounds check and indexed past the file. Such a file is
/// refused like any other out-of-range offset.
#[test]
fn an_etc1s_sgd_offset_near_u64_max_is_refused() {
    for offset in [u64::MAX, u64::MAX - 19, u64::MAX - 20, 1 << 63] {
        let data = with_u64(ETC1S, KTX2_SGD_BYTE_OFFSET, offset);
        assert!(
            matches!(Transcoder::new(&data), Err(Error::InvalidData)),
            "{offset:#x}"
        );
    }
}

/// The fuzzed texture itself: refused, not a panic.
#[test]
fn the_fuzzed_etc1s_texture_is_refused() {
    assert_eq!(
        read_u64(FUZZ_SGD_OFFSET_MAX, KTX2_SGD_BYTE_OFFSET),
        u64::MAX
    );
    assert!(matches!(
        Transcoder::new(FUZZ_SGD_OFFSET_MAX),
        Err(Error::InvalidData)
    ));
}

/// The intermediate formats' slice descs sit at the same offset, and their
/// end overflowed the same way. So did the image count `levels * layers *
/// faces`, a `u32` product, under a huge layer count.
#[test]
fn an_intermediate_sgd_end_or_image_count_that_overflows_is_refused() {
    for file in [UASTC_HDR_6X6_MIP, XUASTC_OLD_SCHEME] {
        assert!(Transcoder::new(file).is_ok());
        for offset in [u64::MAX, u64::MAX - 7] {
            let data = with_u64(file, KTX2_SGD_BYTE_OFFSET, offset);
            assert!(
                matches!(Transcoder::new(&data), Err(Error::InvalidData)),
                "{offset:#x}"
            );
        }
    }
    let data = with_u32(UASTC_HDR_6X6_MIP, KTX2_LAYER_COUNT, u32::MAX);
    assert!(matches!(Transcoder::new(&data), Err(Error::InvalidData)));
}

/// A level's data offset near `u64::MAX`: the end of the level (raw UASTC)
/// or of an image's slice within it (ETC1S, XUASTC) overflowed. The level
/// index is only read at transcode time, so the file opens and the transcode
/// fails.
#[test]
fn a_level_offset_near_u64_max_fails_the_transcode() {
    for file in [ETC1S, UASTC, XUASTC_OLD_SCHEME] {
        let data = with_u64(file, KTX2_LEVEL0_BYTE_OFFSET, u64::MAX - 1);
        let t = Transcoder::new(&data).unwrap();
        assert!(t
            .transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE)
            .is_err());
    }
}

/// On a 32-bit `usize` (wasm32) an offset past 4 GiB lost its high bits to
/// an `as usize` cast, so the codebooks or a level were read from the wrong
/// place without an error, and a sum of in-file offsets and u32 lengths (the
/// codebook lengths, the image count) could wrap. All of them are refused on
/// every target; on a 64-bit host they simply lie past the end of the file.
/// `tests/wasm.rs` runs the same cases under wasm32.
#[test]
fn offsets_and_sums_past_4_gib_are_refused() {
    let sgd = read_u64(ETC1S, KTX2_SGD_BYTE_OFFSET);
    let past_4_gib = with_u64(ETC1S, KTX2_SGD_BYTE_OFFSET, sgd + (1 << 32));
    let endpoints_len = with_u32(ETC1S, sgd as usize + 4, u32::MAX);
    let layers = with_u32(ETC1S, KTX2_LAYER_COUNT, u32::MAX);
    for data in [past_4_gib, endpoints_len, layers] {
        assert!(matches!(Transcoder::new(&data), Err(Error::InvalidData)));
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

/// KTX2 header fields the dimension tests patch: `pixelWidth` and
/// `pixelHeight` (u32 each).
const KTX2_PIXEL_WIDTH: usize = 20;
const KTX2_PIXEL_HEIGHT: usize = 24;

/// A copy of `file` with its base dimensions set to `width` x `height`.
fn with_dims(file: &[u8], width: u32, height: u32) -> Vec<u8> {
    let data = with_u32(file, KTX2_PIXEL_WIDTH, width);
    with_u32(&data, KTX2_PIXEL_HEIGHT, height)
}

/// A single-level UASTC KTX2 of `width` x `height` texels on the header of
/// `uastc_medium.ktx2`: its level holds that file's own (valid) blocks,
/// repeated to cover the new block grid.
fn uastc_ktx2(width: u32, height: u32) -> Vec<u8> {
    let level0 = read_u64(UASTC_MEDIUM, KTX2_LEVEL0_BYTE_OFFSET) as usize;
    let len = width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16;
    let mut data = with_dims(&UASTC_MEDIUM[..level0], width, height);
    data.extend(UASTC_MEDIUM[level0..].iter().copied().cycle().take(len));
    let data = with_u64(&data, KTX2_LEVEL0_BYTE_OFFSET + 8, len as u64);
    with_u64(&data, KTX2_LEVEL0_BYTE_OFFSET + 16, len as u64)
}

/// `output_size` multiplied the header's dimensions in `u32`: 65536 x 65536
/// RGBA32 is 2^32 pixels, a panic with overflow checks and a 0-byte buffer
/// in release. A block or pixel count past `u32` (what one image can
/// address, and what the reference transcoder sizes buffers in) is refused,
/// and so is a byte size past `usize`; the transcode fails the same way
/// before allocating.
#[test]
fn output_sizes_past_u32_counts_are_refused() {
    let cases = [
        (65536, 65536, TargetFormat::Rgba32),
        (u32::MAX, 2, TargetFormat::Rgba32),
        (1 << 31, 1 << 31, TargetFormat::Bc7Rgba),
        (u32::MAX, u32::MAX, TargetFormat::Etc1Rgb),
        (u32::MAX, u32::MAX, TargetFormat::Fxt1Rgb),
    ];
    for (w, h, target) in cases {
        let data = with_dims(UASTC_MEDIUM, w, h);
        let t = Transcoder::new(&data).unwrap();
        assert_eq!(
            t.output_size(0, target),
            Err(Error::InvalidData),
            "{w}x{h} {target:?}"
        );
        if t.supports(target) {
            assert_eq!(
                t.transcode(0, target, DecodeFlags::NONE),
                Err(Error::InvalidData),
                "{w}x{h} {target:?}"
            );
        }
    }
    // The largest square that still fits: 65535^2 pixels, 16 GiB of RGBA32
    // on a 64-bit host.
    let data = with_dims(UASTC_MEDIUM, 65535, 65535);
    let t = Transcoder::new(&data).unwrap();
    let want = 65535usize.checked_mul(65535 * 4);
    assert_eq!(t.output_size(0, TargetFormat::Rgba32).ok(), want);
}

/// The ETC1S slice decoder multiplied the level's block counts in `u32`
/// before looking at the output, so a huge header reached through the
/// internal transcoder (which takes the caller's buffer as is) panicked
/// there. It now refuses, like every other codec.
#[test]
fn block_counts_past_u32_fail_in_the_codecs() {
    for file in [ETC1S, UASTC] {
        let data = with_dims(file, 1 << 20, 1 << 20);
        let t = Ktx2Transcoder::new(&data).unwrap();
        let mut out = [0u8; 64];
        for target in (0..=40).filter_map(TargetFormat::from_i32) {
            assert!(t
                .transcode_image_level(0, target.as_i32(), &mut out)
                .is_none());
        }
    }
}

/// `level_blocks` shifted the base dimensions right by the level index:
/// at 32 and up that overflows the shift (a panic with overflow checks).
/// The internal transcoder takes the level unvalidated, so a level past the
/// level index is refused before anything is derived from it.
#[test]
fn a_level_past_the_shift_width_is_refused() {
    let t = Ktx2Transcoder::new(UASTC).unwrap();
    let mut out = vec![0u8; 16];
    for level in [32, 33, 40, u32::MAX] {
        assert!(t
            .transcode_image_level(level, TargetFormat::Rgba32.as_i32(), &mut out)
            .is_none());
        assert_eq!(t.header.level_dims(level), (1, 1));
    }
    assert_eq!(t.header.level_dims(0), (40, 40));
    assert_eq!(t.header.level_dims(3), (5, 5));
}

/// PVRTC1 Morton-interleaves block coordinates through a 256-entry table,
/// 8 bits at a time: a grid wider than 65536 blocks indexed past it (a
/// panic, with or without overflow checks). 262144 x 4 texels is the widest
/// that fits; one doubling more is refused.
#[test]
#[cfg(feature = "pvrtc1")]
fn a_pvrtc1_grid_past_the_swizzle_table_is_refused() {
    let widest = uastc_ktx2(1 << 18, 4);
    let t = Transcoder::new(&widest).unwrap();
    let out = t
        .transcode(0, TargetFormat::Pvrtc1_4Rgb, DecodeFlags::NONE)
        .unwrap();
    assert_eq!(out.len(), (1 << 16) * 8);

    let wider = uastc_ktx2(1 << 19, 4);
    let t = Transcoder::new(&wider).unwrap();
    assert!(t
        .transcode(0, TargetFormat::Pvrtc1_4Rgb, DecodeFlags::NONE)
        .is_err());
}

/// The single level of `uastc_hdr_zstd.ktx2` is Zstandard-supercompressed:
/// 644x874 texels, 161x219 blocks, so 564144 bytes inflated.
const UASTC_HDR_ZSTD: &[u8] = include_bytes!("../../corpus/smoke/uastc_hdr_zstd.ktx2");
/// Level 0's `uncompressedByteLength` (u64), the third field of the level
/// index entry.
const KTX2_LEVEL0_UNCOMPRESSED_LENGTH: usize = KTX2_LEVEL0_BYTE_OFFSET + 16;

/// The zstd inflate allocated the level's `uncompressedByteLength` up front,
/// a size straight from the file: `u64::MAX` asked for a buffer past
/// `isize::MAX` (a capacity-overflow panic), and anything short of that a
/// buffer as large as the field claimed. The field must now equal the
/// level size the dimensions imply (KTX2 defines it as the size of the
/// level's data), checked before the buffer is allocated.
#[test]
#[cfg(all(feature = "zstd", feature = "hdr"))]
fn a_zstd_length_that_disagrees_with_the_dimensions_is_refused() {
    let exact = read_u64(UASTC_HDR_ZSTD, KTX2_LEVEL0_UNCOMPRESSED_LENGTH);
    assert_eq!(exact, 161 * 219 * 16);
    let target = TargetFormat::RgbaHalf;
    let t = Transcoder::new(UASTC_HDR_ZSTD).unwrap();
    assert!(t.transcode(0, target, DecodeFlags::NONE).is_ok());

    for ulen in [u64::MAX, 1 << 63, exact + 16, exact - 16, exact * 2, 1] {
        let data = with_u64(UASTC_HDR_ZSTD, KTX2_LEVEL0_UNCOMPRESSED_LENGTH, ulen);
        let t = Transcoder::new(&data).unwrap();
        assert!(
            t.transcode(0, target, DecodeFlags::NONE).is_err(),
            "{ulen:#x}"
        );
    }

    // The same through a mip chain: only the level whose length disagrees
    // fails.
    let mip = include_bytes!("../../corpus/smoke/astc_hdr_6x6_zstd.ktx2");
    let data = with_u64(mip, KTX2_LEVEL0_UNCOMPRESSED_LENGTH, u64::MAX);
    let t = Transcoder::new(&data).unwrap();
    assert!(t.transcode(0, target, DecodeFlags::NONE).is_err());
    for level in 1..t.level_count() {
        assert!(
            t.transcode(level, target, DecodeFlags::NONE).is_ok(),
            "level {level}"
        );
    }
}

/// Header fields the global data tests patch: `supercompressionScheme` and
/// `sgdByteLength` (u64), and the DFD byte length (u32), whose 60-byte form
/// declares an ETC1S file's alpha slices.
const KTX2_SUPERCOMPRESSION: usize = 44;
const KTX2_DFD_BYTE_LENGTH: usize = 52;
const KTX2_SGD_BYTE_LENGTH: usize = 72;

// UASTC HDR 6x6 and XUASTC under their Khronos schemes (4 and 5), with the
// 12-byte slice descs.
const XUASTC_ARITH: &[u8] = include_bytes!("../../corpus/smoke/xuastc_arith_12x12.ktx2");

/// The ETC1S global data must lie past the 80-byte header and inside the
/// file, as `ktx2_transcoder::init` requires of every scheme that carries
/// it. The port read it from wherever the offset pointed (the header itself
/// included) and never looked at its length, so a global data block that
/// ran off the end of the file (or whose end wrapped) still opened.
#[test]
fn global_data_outside_the_file_or_inside_the_header_is_refused() {
    let sgd = read_u64(ETC1S, KTX2_SGD_BYTE_OFFSET);
    let sgd_len = read_u64(ETC1S, KTX2_SGD_BYTE_LENGTH);
    let past_end = ETC1S.len() as u64 - sgd + 1;
    for len in [past_end, u64::MAX, u64::MAX - sgd + 1] {
        let data = with_u64(ETC1S, KTX2_SGD_BYTE_LENGTH, len);
        assert!(
            matches!(Transcoder::new(&data), Err(Error::InvalidData)),
            "{len:#x}"
        );
    }
    // Exactly to the end of the file is fine.
    let data = with_u64(ETC1S, KTX2_SGD_BYTE_LENGTH, ETC1S.len() as u64 - sgd);
    assert!(Transcoder::new(&data).is_ok());
    assert!(sgd_len < ETC1S.len() as u64 - sgd);

    // The intermediate formats' slice descs: an offset inside the header
    // read header bytes as descs. 76 makes XUASTC's single desc the
    // header's last word and the level offset, a nonzero length.
    for (file, offset) in [
        (XUASTC_OLD_SCHEME, 76),
        (XUASTC_ARITH, 0),
        (UASTC_HDR_6X6_MIP, 20),
        (UASTC_HDR_6X6_MIP, 79),
    ] {
        assert!(Transcoder::new(file).is_ok());
        let data = with_u64(file, KTX2_SGD_BYTE_OFFSET, offset);
        assert!(
            matches!(Transcoder::new(&data), Err(Error::InvalidData)),
            "{offset}"
        );
    }
}

/// `ktx2_transcoder::decompress_etc1s_global_data` requires the global data
/// header, one 20-byte desc per image, the endpoint, selector and table
/// payloads and the extended data to fit `sgdByteLength`, all three payloads
/// and both codebook counts to be nonzero, and every image to have a color
/// slice (and an alpha slice in an alpha file). The port checked only that
/// the payloads ended inside the file, so it read past the global data into
/// the level data, and opened files whose transcodes could only fail.
#[test]
fn etc1s_global_data_is_checked_like_the_reference() {
    let sgd = read_u64(ETC1S, KTX2_SGD_BYTE_OFFSET) as usize;
    let sgd_len = read_u64(ETC1S, KTX2_SGD_BYTE_LENGTH);
    let first_desc = sgd + 20;
    let refused = [
        // The parts fill the global data exactly: one byte less, or one
        // byte of extended data more, no longer fits.
        with_u64(ETC1S, KTX2_SGD_BYTE_LENGTH, sgd_len - 1),
        with_u64(ETC1S, KTX2_SGD_BYTE_LENGTH, 19),
        with_u32(ETC1S, sgd + 16, 1),
        // Empty codebooks.
        with_u32(ETC1S, sgd, 0),
        with_u32(ETC1S, sgd, 0x0011_0000),
        with_u32(ETC1S, sgd, 0x0000_0047),
        // An image with no color slice.
        with_u32(ETC1S, first_desc + 8, 0),
        // A 60-byte DFD declares alpha, and no image has an alpha slice.
        with_u32(ETC1S, KTX2_DFD_BYTE_LENGTH, 60),
    ];
    for (i, data) in refused.iter().enumerate() {
        assert!(
            matches!(Transcoder::new(data), Err(Error::InvalidData)),
            "case {i}"
        );
    }

    // ETC1S global data exists only under BasisLZ: under no
    // supercompression the file opens without codebooks and its transcodes
    // fail, as the reference never decodes it there.
    let data = with_u32(ETC1S, KTX2_SUPERCOMPRESSION, 0);
    let t = Transcoder::new(&data).unwrap();
    assert!(t
        .transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE)
        .is_err());
}

/// Header fields the DFD and key/value data tests patch: `dfdByteOffset`,
/// `kvdByteOffset` and `kvdByteLength` (u32 each).
const KTX2_DFD_BYTE_OFFSET: usize = 48;
const KTX2_KVD_BYTE_OFFSET: usize = 56;
const KTX2_KVD_BYTE_LENGTH: usize = 60;

/// The little-endian u32 at byte `at` of `file`.
fn read_u32(file: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(file[at..at + 4].try_into().unwrap())
}

/// A copy of `file` that declares no key/value data (offset and length 0),
/// so no rule that involves it applies.
fn without_kvd(file: &[u8]) -> Vec<u8> {
    let data = with_u32(file, KTX2_KVD_BYTE_OFFSET, 0);
    with_u32(&data, KTX2_KVD_BYTE_LENGTH, 0)
}

/// Every case is refused at open with `InvalidData`; a failure names each
/// case that is not, not just the first.
fn assert_refused(cases: &[(&str, Vec<u8>)]) {
    let opened: Vec<&str> = cases
        .iter()
        .filter(|(_, data)| !matches!(Transcoder::new(data), Err(Error::InvalidData)))
        .map(|&(name, _)| name)
        .collect();
    assert!(opened.is_empty(), "not refused: {opened:?}");
}

/// `ktx2_transcoder::init` refuses a DFD that starts inside the 80-byte
/// header or ends past the file. The port read only the 32 bytes it uses,
/// wherever they were.
#[test]
fn a_dfd_inside_the_header_or_past_the_file_is_refused() {
    let dfd = read_u32(UASTC, KTX2_DFD_BYTE_OFFSET) as usize;
    let base = without_kvd(UASTC);
    assert!(Transcoder::new(&base).is_ok());

    // At 52 the DFD's first word is the header's own `dfdByteLength` (44),
    // and its words 3 to 6 (color model, block size, plane bytes) are the
    // global data offset and length, which a file with no supercompression
    // never reads: they get the real DFD's words.
    let mut in_header = with_u32(&base, KTX2_DFD_BYTE_OFFSET, 52);
    in_header[64..80].copy_from_slice(&UASTC[dfd + 12..dfd + 28]);

    // The DFD copied to the end of the file: whole, it opens; 4 bytes short,
    // it is refused.
    let at_end = |len: usize| {
        let mut data = with_u32(&base, KTX2_DFD_BYTE_OFFSET, UASTC.len() as u32);
        data.extend_from_slice(&UASTC[dfd..dfd + len]);
        data
    };
    assert!(Transcoder::new(&at_end(44)).is_ok());
    assert_refused(&[
        ("inside the header", in_header),
        ("past the file", at_end(40)),
    ]);
}

/// KTX2 3.10.3: the DFD's first word, `dfdTotalSize`, repeats
/// `dfdByteLength`, and key/value data starts where the DFD ends
/// (`kvdByteOffset - dfdByteOffset == dfdTotalSize`). `ktx2_transcoder::init`
/// refuses a file where either disagrees; the port read neither.
#[test]
fn a_dfd_whose_size_disagrees_is_refused() {
    let dfd = read_u32(UASTC, KTX2_DFD_BYTE_OFFSET) as usize;
    let kvd = read_u32(UASTC, KTX2_KVD_BYTE_OFFSET) as usize;
    let kvd_end = kvd + read_u32(UASTC, KTX2_KVD_BYTE_LENGTH) as usize;
    assert_eq!(kvd, dfd + 44);
    // Entry padding counts from the start of the file, so the copies below
    // need a 4-aligned end of file.
    let end = UASTC.len() as u32;
    assert_eq!(end % 4, 0);

    // The key/value data copied to the end of the file opens when the DFD
    // moves with it, and is refused when it no longer follows the DFD.
    let both_moved = with_u32(UASTC, KTX2_DFD_BYTE_OFFSET, end);
    let mut both_moved = with_u32(&both_moved, KTX2_KVD_BYTE_OFFSET, end + 44);
    both_moved.extend_from_slice(&UASTC[dfd..kvd_end]);
    assert!(Transcoder::new(&both_moved).is_ok());
    let mut kvd_moved = with_u32(UASTC, KTX2_KVD_BYTE_OFFSET, end);
    kvd_moved.extend_from_slice(&UASTC[kvd..kvd_end]);

    // Without key/value data only `dfdTotalSize` is checked.
    let base = without_kvd(UASTC);
    assert_refused(&[
        ("total size 60", with_u32(&base, dfd, 60)),
        ("total size 0", with_u32(&base, dfd, 0)),
        ("key/value data apart from the DFD", kvd_moved),
    ]);
}

/// `ktx2_transcoder::read_key_values` refuses key/value data with an offset
/// but no length, a block that ends past the file (a checked sum here), and
/// a malformed entry: shorter than 2 bytes, longer than the rest of the
/// block, a key with no NUL, or padding past the end of the block. The port
/// scanned the block only for `KTXanimData` and took any of these for "no
/// such key", so the file opened.
#[test]
fn key_value_data_is_read_like_the_reference() {
    let kvd = read_u32(UASTC, KTX2_KVD_BYTE_OFFSET) as usize;
    let kvd_len = read_u32(UASTC, KTX2_KVD_BYTE_LENGTH);
    // One entry, a 48-byte `KTXwriter`, fills the block.
    assert_eq!(read_u32(UASTC, kvd), kvd_len - 4);
    assert_eq!(&UASTC[kvd + 4..kvd + 14], b"KTXwriter\0");
    let mut no_nul = UASTC.to_vec();
    no_nul[kvd + 4..kvd + 52].fill(b'x');
    // A 45-byte entry needs 3 bytes of padding; the block ends 1 byte after
    // it.
    let short_padding = with_u32(&with_u32(UASTC, kvd, 45), KTX2_KVD_BYTE_LENGTH, 50);
    let past_file = (UASTC.len() - kvd) as u32 + 1;
    assert_refused(&[
        (
            "offset, no length",
            with_u32(UASTC, KTX2_KVD_BYTE_LENGTH, 0),
        ),
        (
            "past the file",
            with_u32(UASTC, KTX2_KVD_BYTE_LENGTH, past_file),
        ),
        (
            "length u32::MAX",
            with_u32(UASTC, KTX2_KVD_BYTE_LENGTH, u32::MAX),
        ),
        ("entry of 1 byte", with_u32(UASTC, kvd, 1)),
        ("entry past the block", with_u32(UASTC, kvd, kvd_len - 3)),
        ("key with no NUL", no_nul),
        ("padding past the block", short_padding),
    ]);
    // No key/value data at all is fine.
    assert!(Transcoder::new(&without_kvd(UASTC)).is_ok());
}

/// Byte offsets the `.basis` tests patch: the header's data size (u32) and,
/// within a 23-byte slice desc, the original width and height and the block
/// counts (u16 each) and the slice's byte length (u32).
const BASIS_DATA_SIZE: usize = 8;
const BASIS_SLICE_DESC_OFS: usize = 65;
const SLICE_ORIG_WIDTH: usize = 5;
const SLICE_ORIG_HEIGHT: usize = 7;
const SLICE_NUM_BLOCKS_X: usize = 9;
const SLICE_NUM_BLOCKS_Y: usize = 11;
const SLICE_FILE_SIZE: usize = 17;

/// A copy of the `.basis` `file` with the u16 or u32 (`width` bytes) at byte
/// `at` of its first slice desc set to `value`.
fn with_slice0(file: &[u8], at: usize, width: usize, value: u32) -> Vec<u8> {
    let desc = BASIS_SLICE_DESC_OFS;
    let desc = u32::from_le_bytes(file[desc..desc + 4].try_into().unwrap()) as usize;
    let mut data = file.to_vec();
    data[desc + at..desc + at + width].copy_from_slice(&value.to_le_bytes()[..width]);
    data
}

/// The header's data size is a u32 added to the header size: on a 32-bit
/// `usize` (wasm32) `77 + u32::MAX` overflowed. Refused on every target;
/// `tests/wasm.rs` runs it under wasm32.
#[test]
fn a_basis_data_size_past_the_file_is_refused() {
    let data = with_u32(BASIS_ETC1S, BASIS_DATA_SIZE, u32::MAX);
    assert!(matches!(Transcoder::new(&data), Err(Error::InvalidData)));
}

/// A `.basis` slice records its block counts and byte length apart from its
/// pixel size, and the codecs index the output by both. A grid wider or
/// taller than the pixel size implies wrote past the FXT1 grid, subtracted
/// past zero in the UASTC HDR, ASTC HDR 6x6 and raw-ASTC reblocking clamps,
/// and read past the decoded 6x6 blocks; a grid with no blocks divided by
/// zero; a byte length shorter than the grid read past the slice. Each was
/// a panic (most of them in release too). Each now fails the transcode.
#[test]
fn basis_slices_that_disagree_with_their_size_fail_the_transcode() {
    let uastc_hdr = include_bytes!("../../corpus/smoke/uastc_hdr.basis");
    let hdr_6x6i = include_bytes!("../../corpus/smoke/uastc_hdr_6x6i.basis");
    let xuastc = include_bytes!("../../corpus/smoke/xuastc_5x4.basis");
    let cases: [(&[u8], usize, usize, u32, TargetFormat); 9] = [
        (BASIS_ETC1S, SLICE_ORIG_WIDTH, 2, 1, TargetFormat::Fxt1Rgb),
        (BASIS_ETC1S, SLICE_ORIG_HEIGHT, 2, 6, TargetFormat::Fxt1Rgb),
        (BASIS_UASTC, SLICE_NUM_BLOCKS_X, 2, 0, TargetFormat::Rgba32),
        (BASIS_UASTC, SLICE_FILE_SIZE, 4, 12, TargetFormat::Bc1Rgb),
        (
            BASIS_UASTC,
            SLICE_FILE_SIZE,
            4,
            0x8000,
            TargetFormat::Bc7Rgba,
        ),
        (uastc_hdr, SLICE_ORIG_WIDTH, 2, 4, TargetFormat::RgbaHalf),
        (hdr_6x6i, SLICE_NUM_BLOCKS_X, 2, 0x7f, TargetFormat::Bc6h),
        (hdr_6x6i, SLICE_NUM_BLOCKS_Y, 2, 1000, TargetFormat::Bc6h),
        (xuastc, SLICE_ORIG_HEIGHT, 2, 0, TargetFormat::Rgba32),
    ];
    for (file, at, width, value, target) in cases {
        let data = with_slice0(file, at, width, value);
        let t = Transcoder::new(&data).unwrap();
        if !t.supports(target) {
            continue;
        }
        assert!(
            t.transcode(0, target, DecodeFlags::NONE).is_err(),
            "slice field {at} = {value:#x}, {target:?}"
        );
    }
}

/// Both Basis Universal containers open through the same `Transcoder::new`,
/// with codec, alpha, and dimension metadata intact.
#[test]
fn basis_container_auto_detects_and_reports_metadata() {
    let e = Transcoder::new(BASIS_ETC1S).unwrap();
    assert_eq!(e.source_format(), SourceFormat::Etc1s);
    assert!(!e.has_alpha());
    let (w, h) = e.base_dimensions();
    assert!(w > 0 && h > 0);
    assert!(e.level_count() >= 1);

    let a = Transcoder::new(BASIS_ETC1S_ALPHA).unwrap();
    assert_eq!(a.source_format(), SourceFormat::Etc1s);
    assert!(a.has_alpha(), "alpha3.basis carries alpha slices");

    let u = Transcoder::new(BASIS_UASTC).unwrap();
    assert_eq!(u.source_format(), SourceFormat::UastcLdr);
}

/// The `transcode`/`output_size` agreement holds for `.basis` inputs too,
/// across all three smoke fixtures.
#[test]
fn basis_transcode_len_matches_output_size() {
    for data in [BASIS_ETC1S, BASIS_ETC1S_ALPHA, BASIS_UASTC] {
        let t = Transcoder::new(data).unwrap();
        for target in [
            TargetFormat::Bc1Rgb,
            TargetFormat::Bc7Rgba,
            TargetFormat::Astc4x4Rgba,
            TargetFormat::Rgba32,
            TargetFormat::Etc2Rgba,
        ] {
            if !t.supports(target) {
                continue;
            }
            let want = t.output_size(0, target).unwrap();
            let got = t.transcode(0, target, DecodeFlags::NONE).unwrap();
            assert_eq!(got.len(), want, "{target:?} output size");
        }
    }
}

/// A video file opens and is detected; the stateless transcode entry points
/// reject it (P-frames need cross-frame state) and point the caller at
/// `transcode_video_frame`.
#[test]
fn video_stateless_entry_points_reject() {
    let t = Transcoder::new(BASIS_VIDEO).unwrap();
    assert_eq!(t.source_format(), SourceFormat::Etc1s);
    assert!(t.is_video());
    assert!(matches!(
        t.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE),
        Err(Error::VideoRequiresState)
    ));
    assert!(matches!(
        t.transcode_image(0, 0, 0, TargetFormat::Etc1Rgb, DecodeFlags::NONE),
        Err(Error::VideoRequiresState)
    ));
    let mut buf = vec![0u8; t.output_size(0, TargetFormat::Rgba32).unwrap()];
    assert!(matches!(
        t.transcode_into(0, TargetFormat::Rgba32, DecodeFlags::NONE, &mut buf),
        Err(Error::VideoRequiresState)
    ));
}

/// Per-frame FNV-1a 64 checksums of the video fixture's ETC1 output.
const VIDEO_ETC1_FNV: &[u64] = &[
    0x045db398c73040b9,
    0x3e649570caf1509e,
    0xfae7e24cfcc27119,
    0x81b0a6f265dc68d2,
    0xf5fd341c67900130,
    0xb6838fcecbdf2079,
    0x2fc7f67db528ea52,
    0x47e3ae1181b45e3d,
    0xe6d8cdc6e81daf37,
    0x01d598e5b36656e9,
    0xdac01dc62680bae8,
    0xf473c958b31aa4f9,
    0x0f7dd706951de865,
    0xf9cd38ad2465c8ad,
    0x699e54aebd07a507,
];

/// FNV-1a 64-bit hash, a compact whole-frame checksum for the video test.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Every frame of the video fixture transcodes in order through the stateful
/// video API. The per-frame checksums pin the exact output bytes, including
/// the P-frames' conditional-replenishment reads of the previous frame; the
/// values were produced by this transcoder after the conformance suite proved
/// the whole sequence byte-identical to the reference C++ transcoder.
#[test]
#[cfg(feature = "etc")]
fn video_frames_transcode_in_order() {
    let t = Transcoder::new(BASIS_VIDEO).unwrap();
    assert!(t.is_video());
    let frames = t.layer_count();
    assert_eq!(frames as usize, VIDEO_ETC1_FNV.len());

    let mut state = basisu::VideoState::new();
    for f in 0..frames {
        let etc1 = t
            .transcode_video_frame(&mut state, 0, f, TargetFormat::Etc1Rgb, DecodeFlags::NONE)
            .unwrap();
        assert_eq!(fnv1a64(&etc1), VIDEO_ETC1_FNV[f as usize], "frame {f}");
    }

    // A second pass on a reset state reproduces the same frames.
    state.reset();
    let etc1 = t
        .transcode_video_frame(&mut state, 0, 0, TargetFormat::Etc1Rgb, DecodeFlags::NONE)
        .unwrap();
    assert_eq!(fnv1a64(&etc1), VIDEO_ETC1_FNV[0]);
}

/// Flip one byte at a stride of positions through a real file; each mutant
/// must parse-or-error without panicking.
#[test]
fn byte_flipping_a_real_file_never_panics() {
    let mut data = UASTC.to_vec();
    let len = data.len();
    for i in (0..len).step_by(7) {
        let orig = data[i];
        data[i] ^= 0xFF;
        if let Ok(t) = Transcoder::new(&data) {
            // If it still opens, transcoding a level must also not panic.
            let _ = t.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE);
        }
        data[i] = orig;
    }
}
