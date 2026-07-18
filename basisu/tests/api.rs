//! Public-API integration tests over the committed smoke fixtures, plus
//! robustness checks: malformed, truncated, and adversarial input must return
//! an error, never panic. The byte-for-byte correctness of the output is the
//! golden test's job; this covers the consumer-facing surface and failure
//! modes.

#![cfg(not(target_arch = "wasm32"))]

use basisu::{DecodeFlags, Error, SourceFormat, TargetFormat, Transcoder};

// KTX2 smoke fixtures, one per source codec.
const ETC1S: &[u8] = include_bytes!("fixtures/etc1s.ktx2");
const UASTC: &[u8] = include_bytes!("fixtures/uastc.ktx2");

// `.basis` container smoke fixtures: an ETC1S file, an ETC1S+alpha file, and a
// UASTC file. The same payloads `Transcoder::new` must auto-detect and open
// exactly like KTX2.
const BASIS_ETC1S: &[u8] = include_bytes!("../../corpus/smoke/kodim20.basis");
const BASIS_ETC1S_ALPHA: &[u8] = include_bytes!("../../corpus/smoke/alpha3.basis");
const BASIS_UASTC: &[u8] = include_bytes!("../../corpus/smoke/kodim03_uastc.basis");

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

/// An undersized output buffer fails with `OutputTooSmall` carrying the exact
/// size required.
#[test]
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
        data.extend(std::iter::repeat(0xCD).take(tail_len));
        let _ = Transcoder::new(&data);
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
