//! The conformance gate: for every `Done` row of the matrix, for every Basis
//! file in the corpus, for every mip level and decode-flag combination, the
//! Rust transcoder must produce byte-for-byte identical output to the C++ oracle
//! (the upstream public `ktx2_transcoder`). This is the core invariant of the
//! crate.

use basisu::{DecodeFlags, SourceFormat, Supercompression, Transcoder, VideoState};
use conformance::matrix::{matrix, Status};
use conformance::{corpus_files, OracleBasis, OracleKtx2};

/// `basis_tex_format` values this crate transcodes: ETC1S (0), UASTC LDR 4x4
/// (1), UASTC HDR 4x4 (2), and raw ASTC LDR (19..32, mapped inline below).
/// Everything else (HDR 6x6, XUASTC) is a feature skip.
const BASIS_TEX_FORMAT_ETC1S: u32 = 0;
const BASIS_TEX_FORMAT_UASTC_LDR_4X4: u32 = 1;
const BASIS_TEX_FORMAT_UASTC_HDR_4X4: u32 = 2;

/// A whole-file skip: a container feature neither side can currently produce
/// canonical ground truth for. This must be decided before any
/// `oracle.transcode` call, because the C++ oracle (built without KTX2 zstd)
/// aborts (SIGABRT, uncatchable from Rust) on zstd/unknown-supercompression
/// levels.
fn file_skip_reason(tex: &Transcoder) -> Option<&'static str> {
    use Supercompression::*;
    let sc = tex.supercompression();
    // Reject malformed (source, supercompression) combos that would make the
    // oracle abort: a valid ETC1S file is BasisLz; a valid UASTC file (LDR or
    // HDR) is None or Zstandard (the reference rejects DEFLATE HDR files at
    // open, so those never reach this check).
    let valid = match tex.source_format() {
        SourceFormat::Etc1s => sc == BasisLz,
        SourceFormat::UastcLdr
        | SourceFormat::UastcHdr4x4
        | SourceFormat::AstcLdr(_)
        | SourceFormat::AstcHdr6x6 => sc == None || sc == Zstandard,
        // The intermediate streams are their own coding: raw `.basis`
        // slices, the KTX2 UASTC_HDR_6x6I (4) / XUASTC_LDR (5) schemes, or
        // the old-style v1.6/v2.0 presentation under the BasisLZ scheme id.
        SourceFormat::UastcHdr6x6 => sc == None || sc == Other(4) || sc == BasisLz,
        SourceFormat::XuastcLdr(_) => sc == None || sc == Other(5) || sc == BasisLz,
        // `SourceFormat` is non-exhaustive: a codec this harness does not
        // know yet is treated as malformed and counted as a loud skip.
        _ => false,
    };
    if !valid {
        return Some("malformed-source-sc");
    }
    match sc {
        None | BasisLz => Option::None,
        // Zstandard is gated only when the oracle was built with zstd support
        // (BASISD_SUPPORT_KTX2_ZSTD=1, see conformance/build.rs). Without it the
        // oracle can't produce ground truth, so we must keep skipping zstd.
        #[cfg(oracle_zstd)]
        Zstandard => Option::None,
        #[cfg(not(oracle_zstd))]
        Zstandard => Some("zstd-no-oracle"),
        // Schemes 4/5 are the intermediate codecs' own coding, not a
        // compressed wrapper; the oracle handles them natively.
        Other(4) | Other(5) => Option::None,
        // Unknown supercompression (any `Other` id, or a future variant of
        // the non-exhaustive enum): oracle would reject/abort.
        _ => Some("other-sc"),
    }
}

/// Run the full conformance gate: every done case against every corpus file,
/// asserting byte-for-byte equality with the oracle and panicking on any
/// mismatch or unexpected Rust failure.
#[test]
fn conformance_byte_identical() {
    let cases = matrix();
    let done: Vec<_> = cases.iter().filter(|c| c.status == Status::Done).collect();
    let files = corpus_files();
    assert!(
        !files.is_empty(),
        "no corpus files found (need corpus/smoke)"
    );

    let (mut files_seen, mut passes, mut bytes) = (0usize, 0u64, 0u64);
    let mut skipped_feature = std::collections::BTreeMap::<&str, u64>::new();

    for path in &files {
        // Only the .ktx2 container here; .basis is gated by the test below.
        // (Without this filter, every corpus .basis file would be offered to
        // the KTX2 oracle, fail its open, and mis-count as an oracle reject.)
        if path.extension().is_none_or(|x| x != "ktx2") {
            continue;
        }
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        let rust_tex = Transcoder::new(&data);
        let Some(oracle) = OracleKtx2::open(&data) else {
            // The reference declined the file, so there is no ground truth to
            // gate against, but never drop it silently. If the Rust side
            // *opened* it, a supported file is losing oracle coverage and the
            // skip counter must say why: an oracle built without zstd rejects
            // zstd-supercompressed files here, at open, before
            // `file_skip_reason` ever runs. If neither side opens it, it is
            // not a Basis payload we claim (e.g. a raw non-Basis KTX2 from
            // the corpus).
            let reason = match &rust_tex {
                Ok(tex) if tex.supercompression() == Supercompression::Zstandard => {
                    if cfg!(oracle_zstd) {
                        "zstd-oracle-open-failed"
                    } else {
                        "zstd-no-oracle"
                    }
                }
                Ok(_) => "oracle-rejected-rust-opened",
                Err(_) => "not-basis",
            };
            *skipped_feature.entry(reason).or_default() += 1;
            continue;
        };
        // If Rust declines to open it, it's a source format we don't claim to
        // support (e.g. v2's xUASTC / UASTC-HDR 6x6), so skip rather than fail.
        // Opening a file is the claim of support; a wrong byte on a file we did
        // open is the real bug, and that path is still gated below.
        let Ok(tex) = rust_tex else {
            *skipped_feature
                .entry("rust-unsupported-format")
                .or_default() += 1;
            continue;
        };
        // File-level pre-skip, done before any oracle.transcode (see above).
        if let Some(f) = file_skip_reason(&tex) {
            *skipped_feature.entry(f).or_default() += 1;
            continue;
        }
        files_seen += 1;
        let source = tex.source_format();
        let layers = oracle.info.layers.max(1);
        let faces = oracle.info.faces.max(1);
        // One cross-frame state per file, advanced in the same call order as
        // the oracle handle's internal state, so video P-frames stay in
        // lockstep across every case below.
        let mut vstate = VideoState::new();

        for case in &done {
            if case.source != source {
                continue;
            }

            for level in 0..oracle.info.levels {
                for layer in 0..layers {
                    for face in 0..faces {
                        for &flag in case.flags {
                            let Some(want) = oracle.transcode_image_flags(
                                level,
                                layer,
                                face,
                                case.target.as_i32(),
                                flag,
                            ) else {
                                continue; // oracle declined this combo for this file
                            };
                            let got = if tex.is_video() {
                                // Video: frames are layers; decode through the
                                // stateful entry point in loop order.
                                tex.transcode_video_frame(
                                    &mut vstate,
                                    level,
                                    layer,
                                    case.target,
                                    DecodeFlags::from_bits(flag),
                                )
                            } else {
                                tex.transcode_image(
                                    level,
                                    layer,
                                    face,
                                    case.target,
                                    DecodeFlags::from_bits(flag),
                                )
                            };
                            let got = got.unwrap_or_else(|e| {
                                panic!(
                                    "rust failed {:?} L{level} layer{layer} face{face} \
                                         flags={flag} on {path:?}: {e:?}",
                                    case.target
                                )
                            });
                            assert_eq!(
                                got, want.data,
                                "{:?} L{level} layer{layer} face{face} flags={flag} \
                                 differs on {path:?}",
                                case.target
                            );
                            passes += 1;
                            bytes += got.len() as u64;
                        }
                    }
                }
            }
        }
    }

    let skips: String = skipped_feature
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "conformance: {files_seen} files, {} done-cases, {passes} byte-identical passes, {} MB; \
         feature-skips: [{skips}]",
        done.len(),
        bytes / (1024 * 1024),
    );
}

/// The `.basis` conformance gate: the same byte-for-byte invariant as the KTX2
/// gate, but over the `.basis` container (the upstream public `basisu_transcoder`
/// as oracle). For every `.basis` corpus file, every `Done` matrix row matching
/// the file's source codec, every image/level, and every flag variant, the Rust
/// output must equal the oracle's exactly, including ETC1S video (frames are
/// decoded in order with cross-frame state on both sides). HDR 6x6/XUASTC/
/// ASTC-LDR `.basis` are detected by tex_format and skipped with a counter,
/// exactly like the KTX2 feature-skips.
#[test]
fn conformance_basis_byte_identical() {
    let cases = matrix();
    let done: Vec<_> = cases.iter().filter(|c| c.status == Status::Done).collect();
    let files = corpus_files();
    assert!(
        !files.is_empty(),
        "no corpus files found (need corpus/smoke)"
    );

    let (mut files_seen, mut passes, mut bytes) = (0usize, 0u64, 0u64);
    let mut skipped_feature = std::collections::BTreeMap::<&str, u64>::new();

    for path in &files {
        // Only the .basis container here; .ktx2 is gated by the test above.
        if path.extension().is_none_or(|x| x != "basis") {
            continue;
        }
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        let Some(oracle) = OracleBasis::open(&data) else {
            // The reference declined (e.g. a global-codebook .basis); skip.
            *skipped_feature.entry("oracle-declined").or_default() += 1;
            continue;
        };

        // Detect unsupported source formats before any transcode and skip
        // them (counted), mirroring the KTX2 feature-skips. This is what keeps
        // an HDR-6x6/XUASTC .basis from being mis-handled: the oracle opens
        // them, but we never transcode them.
        let source = match oracle.info.tex_format {
            BASIS_TEX_FORMAT_ETC1S => SourceFormat::Etc1s,
            BASIS_TEX_FORMAT_UASTC_LDR_4X4 => SourceFormat::UastcLdr,
            BASIS_TEX_FORMAT_UASTC_HDR_4X4 => SourceFormat::UastcHdr4x4,
            // cASTC_LDR_4x4 (19) .. cASTC_LDR_12x12 (32), in AstcBlock::ALL
            // declaration order.
            tf @ 19..=32 => SourceFormat::AstcLdr(basisu::AstcBlock::ALL[(tf - 19) as usize]),
            // cASTC_HDR_6x6 (3) and cUASTC_HDR_6x6_INTERMEDIATE (4).
            3 => SourceFormat::AstcHdr6x6,
            4 => SourceFormat::UastcHdr6x6,
            // cXUASTC_LDR_4x4 (5) .. cXUASTC_LDR_12x12 (18).
            tf @ 5..=18 => SourceFormat::XuastcLdr(basisu::AstcBlock::ALL[(tf - 5) as usize]),
            _ => {
                *skipped_feature
                    .entry("basis-unsupported-format")
                    .or_default() += 1;
                continue;
            }
        };
        // Rust must open every .basis we claim to support. A decline here is a
        // bug (the file passed the feature gate), so fail rather than skip.
        let tex = Transcoder::new(&data)
            .unwrap_or_else(|e| panic!("rust failed to open supported .basis {path:?}: {e:?}"));
        assert_eq!(
            tex.source_format(),
            source,
            "rust/oracle source codec disagree on {path:?}"
        );
        // ETC1S is intrinsically BasisLZ; UASTC is stored raw. No zstd in .basis.
        assert!(tex.supercompression() != Supercompression::Zstandard);

        files_seen += 1;
        let total_images = oracle.info.total_images.max(1);
        // Per-file cross-frame state; see the KTX2 gate above.
        let mut vstate = VideoState::new();

        for case in &done {
            if case.source != source {
                continue;
            }
            for image in 0..total_images {
                for level in 0..oracle.info.levels {
                    for &flag in case.flags {
                        let Some(want) = oracle.transcode(image, level, case.target.as_i32(), flag)
                        else {
                            continue; // oracle declined this combo for this file
                        };
                        // `.basis` exposes images via the `layer` parameter;
                        // video frames go through the stateful entry point.
                        let got = if tex.is_video() {
                            tex.transcode_video_frame(
                                &mut vstate,
                                level,
                                image,
                                case.target,
                                DecodeFlags::from_bits(flag),
                            )
                        } else {
                            tex.transcode_image(
                                level,
                                image,
                                0,
                                case.target,
                                DecodeFlags::from_bits(flag),
                            )
                        };
                        let got = got.unwrap_or_else(|e| {
                            panic!(
                                "rust failed {:?} image{image} L{level} flags={flag} \
                                     on {path:?}: {e:?}",
                                case.target
                            )
                        });
                        assert_eq!(
                            got, want.data,
                            "{:?} image{image} L{level} flags={flag} differs on {path:?}",
                            case.target
                        );
                        passes += 1;
                        bytes += got.len() as u64;
                    }
                }
            }
        }
    }

    let skips: String = skipped_feature
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "conformance(.basis): {files_seen} files, {} done-cases, {passes} byte-identical \
         passes, {} MB; feature-skips: [{skips}]",
        done.len(),
        bytes / (1024 * 1024),
    );
}

/// Print the coverage report: implemented vs. total combos, broken down by owner.
#[test]
fn coverage_report() {
    let cases = matrix();
    let total = cases.len();
    let done = cases.iter().filter(|c| c.status == Status::Done).count();

    let mut by_owner = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for c in &cases {
        let e = by_owner.entry(c.owner).or_default();
        e.1 += 1;
        if c.status == Status::Done {
            e.0 += 1;
        }
    }

    eprintln!("\n=== conformance matrix coverage: {done}/{total} combos done ===");
    for (owner, (d, t)) in &by_owner {
        let mark = if d == t { "DONE" } else { "todo" };
        eprintln!("  [{mark}] {owner:<22} {d}/{t}");
    }
    eprintln!();
}
