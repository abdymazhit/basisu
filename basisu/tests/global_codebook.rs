//! Global-codebook `.basis` decode, verified without a C++ toolchain.
//!
//! Basis Universal lets several `.basis` files share one ETC1S codebook: a
//! codebook-carrying file plus dependent files that hold only their Huffman
//! tables and slices and set the global-codebook flag. There is no such file in
//! the smoke corpus and no encoder to make one, so each test synthesizes a
//! dependent file from a real self-contained ETC1S file by flipping that flag
//! (the transcoder's open path validates no header checksum, so the byte edit
//! is accepted). Decoding the synthesized file with the original's own codebook
//! must reproduce the original's output exactly. Since the self-contained
//! decode is already gated byte-for-byte against the C++ oracle by the
//! conformance suite, this transitively proves the global-codebook path.

#![cfg(not(target_arch = "wasm32"))]

use basisu::{DecodeFlags, TargetFormat, Transcoder, VideoState};
use std::path::PathBuf;

/// `cBASISHeaderFlagUsesGlobalCodebook`, bit 3 of the little-endian `m_flags`
/// field at header offset 21.
const FLAG_USES_GLOBAL_CODEBOOK: u8 = 8;
/// Header offset of `m_total_endpoints` (little-endian u16).
const TOTAL_ENDPOINTS_OFS: usize = 39;

/// Read the named asset from the shared `corpus/smoke` directory.
fn smoke(name: &str) -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::read(root.join("../corpus/smoke").join(name))
        .unwrap_or_else(|e| panic!("read smoke asset {name}: {e}"))
}

/// A copy of `bytes` with the global-codebook flag set, turning a self-contained
/// ETC1S file into one that references an external codebook.
fn as_global_codebook_file(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[21] |= FLAG_USES_GLOBAL_CODEBOOK;
    out
}

/// Every `TargetFormat`, discovered through the public numeric mapping so the
/// list needs no maintenance as targets are added.
fn all_targets() -> Vec<TargetFormat> {
    (0..=40).filter_map(TargetFormat::from_i32).collect()
}

/// Decoding a synthesized global-codebook file with the original file's own
/// codebook reproduces the self-contained decode byte-for-byte, across every
/// supported target, level, and a representative set of decode flags.
#[test]
fn global_codebook_matches_self_contained() {
    let flags = [
        DecodeFlags::NONE,
        DecodeFlags::HIGH_QUALITY,
        DecodeFlags::TRANSCODE_ALPHA_TO_OPAQUE,
    ];
    let mut checked = 0usize;

    for name in ["kodim20.basis", "alpha3.basis"] {
        let bytes = smoke(name);
        let original = Transcoder::new(&bytes).expect("open self-contained ETC1S");
        assert_eq!(
            original.source_format(),
            basisu::SourceFormat::Etc1s,
            "{name} is expected to be ETC1S"
        );
        let codebook = original
            .etc1s_codebook()
            .expect("self-contained ETC1S exposes its codebook");

        let global_bytes = as_global_codebook_file(&bytes);
        let global = Transcoder::new_with_codebook(&global_bytes, &codebook)
            .expect("open global-codebook file with its codebook");

        assert_eq!(
            global.level_count(),
            original.level_count(),
            "{name} levels"
        );
        assert_eq!(
            global.base_dimensions(),
            original.base_dimensions(),
            "{name} dimensions"
        );

        for level in 0..original.level_count() {
            for &target in &all_targets() {
                if !original.supports(target) {
                    continue;
                }
                for &flag in &flags {
                    let want = original.transcode(level, target, flag);
                    let got = global.transcode(level, target, flag);
                    assert_eq!(
                        got.is_ok(),
                        want.is_ok(),
                        "{name} {target:?} L{level} flags={flag:?}: outcome differs"
                    );
                    if let (Ok(want), Ok(got)) = (want, got) {
                        assert_eq!(
                            got, want,
                            "{name} {target:?} L{level} flags={flag:?}: bytes differ"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 0, "no global-codebook transcodes were checked");
    eprintln!("global codebook: {checked} transcodes byte-identical to self-contained");
}

/// A global-codebook file cannot be opened without its codebook: the stateless
/// entry point rejects it rather than decoding garbage.
#[test]
fn global_codebook_file_rejected_without_codebook() {
    let global_bytes = as_global_codebook_file(&smoke("kodim20.basis"));
    assert!(
        Transcoder::new(&global_bytes).is_err(),
        "a global-codebook file must not open without its codebook"
    );
}

/// A codebook whose size does not match the counts declared in the file's
/// header is refused, rather than used to decode out of bounds.
#[test]
fn global_codebook_size_mismatch_rejected() {
    let bytes = smoke("kodim20.basis");
    let codebook = Transcoder::new(&bytes)
        .unwrap()
        .etc1s_codebook()
        .expect("codebook");

    // Corrupt only the declared endpoint count so it no longer matches the
    // (correct) codebook we supply.
    let mut global_bytes = as_global_codebook_file(&bytes);
    let bumped = u16::from_le_bytes([
        global_bytes[TOTAL_ENDPOINTS_OFS],
        global_bytes[TOTAL_ENDPOINTS_OFS + 1],
    ])
    .wrapping_add(1)
    .to_le_bytes();
    global_bytes[TOTAL_ENDPOINTS_OFS] = bumped[0];
    global_bytes[TOTAL_ENDPOINTS_OFS + 1] = bumped[1];

    assert!(
        Transcoder::new_with_codebook(&global_bytes, &codebook).is_err(),
        "a codebook whose size disagrees with the header must be refused"
    );
}

/// Supplying a codebook to a self-contained file is a harmless no-op: it opens
/// and transcodes exactly as the plain entry point would.
#[test]
fn self_contained_file_ignores_supplied_codebook() {
    let bytes = smoke("kodim20.basis");
    let plain = Transcoder::new(&bytes).unwrap();
    let codebook = plain.etc1s_codebook().expect("codebook");
    let with_cb = Transcoder::new_with_codebook(&bytes, &codebook)
        .expect("self-contained file opens with a codebook supplied");

    for &target in &all_targets() {
        if !plain.supports(target) {
            continue;
        }
        assert_eq!(
            with_cb.transcode(0, target, DecodeFlags::NONE).ok(),
            plain.transcode(0, target, DecodeFlags::NONE).ok(),
            "{target:?}: supplying a codebook changed a self-contained decode"
        );
    }
}

/// Only self-contained ETC1S `.basis` files expose a codebook: KTX2 embeds its
/// codebook (no external-codebook variant exists) and UASTC has none.
#[test]
fn no_codebook_for_ktx2_or_uastc() {
    assert!(
        Transcoder::new(&smoke("etc1s.ktx2"))
            .unwrap()
            .etc1s_codebook()
            .is_none(),
        "KTX2 has no external codebook to share"
    );
    assert!(
        Transcoder::new(&smoke("kodim03_uastc.basis"))
            .unwrap()
            .etc1s_codebook()
            .is_none(),
        "UASTC has no ETC1S codebook"
    );
}

/// A global codebook works with ETC1S video too: decoding the frames in order
/// through the stateful entry point reproduces the self-contained video
/// frame-for-frame. Video P-frames replenish from the previous frame, so both
/// sides advance their own `VideoState` in lockstep.
#[test]
fn global_codebook_video_matches_self_contained() {
    let bytes = smoke("video.basis");
    let original = Transcoder::new(&bytes).expect("open ETC1S video");
    assert!(original.is_video(), "video.basis is expected to be a video");
    let codebook = original
        .etc1s_codebook()
        .expect("video exposes its codebook");

    let global_bytes = as_global_codebook_file(&bytes);
    let global = Transcoder::new_with_codebook(&global_bytes, &codebook)
        .expect("open global-codebook video");
    assert_eq!(global.layer_count(), original.layer_count(), "frame count");

    let targets = [
        TargetFormat::Etc1Rgb,
        TargetFormat::Etc2Rgba,
        TargetFormat::Bc1Rgb,
        TargetFormat::Bc7Rgba,
        TargetFormat::Astc4x4Rgba,
        TargetFormat::Rgba32,
    ];
    let frames = original.layer_count().max(1);
    let mut checked = 0usize;

    for target in targets {
        if !original.supports(target) {
            continue;
        }
        let mut want_state = VideoState::new();
        let mut got_state = VideoState::new();
        for frame in 0..frames {
            let want = original
                .transcode_video_frame(&mut want_state, 0, frame, target, DecodeFlags::NONE)
                .expect("self-contained frame");
            let got = global
                .transcode_video_frame(&mut got_state, 0, frame, target, DecodeFlags::NONE)
                .expect("global-codebook frame");
            assert_eq!(got, want, "{target:?} frame {frame}: bytes differ");
            checked += 1;
        }
    }
    assert!(checked > 0, "no video frames were checked");
    eprintln!("global codebook (video): {checked} frames byte-identical to self-contained");
}
