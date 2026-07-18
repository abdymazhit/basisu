//! The conformance matrix: which (source, target, decode-flags) combinations
//! exist and whether each is implemented. The harness pairs every row with the
//! corpus at all levels and asserts byte-for-byte equality with the C++ oracle
//! for each `Done` case. A row is only `Done` once it passes corpus-wide, so a
//! row marked `Done` that has not been wired up turns the suite red.

use basisu::{is_format_supported, AstcBlock, SourceFormat, TargetFormat};

/// Implementation status of a matrix row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Implemented in Rust; the harness enforces byte-equality on the corpus.
    Done,
    /// Not yet implemented; the harness counts it for coverage and skips it.
    Todo,
}

/// One row of the matrix: a (source, target) pair, the decode-flag combinations
/// to test, its implementation status, and a label identifying the code area
/// responsible for that target.
#[derive(Clone, Debug)]
pub struct Case {
    /// Source codec of the input texture.
    pub source: SourceFormat,
    /// Transcode target format.
    pub target: TargetFormat,
    /// Decode-flag bitmasks to run this pair under.
    pub flags: &'static [u32],
    /// Whether the pair is implemented and gated, or still pending.
    pub status: Status,
    /// Label of the code area responsible, for the coverage report.
    pub owner: &'static str,
}

/// Every `transcoder_texture_format`, in numeric order.
pub const ALL_TARGETS: &[TargetFormat] = &[
    TargetFormat::Etc1Rgb,
    TargetFormat::Etc2Rgba,
    TargetFormat::Bc1Rgb,
    TargetFormat::Bc3Rgba,
    TargetFormat::Bc4R,
    TargetFormat::Bc5Rg,
    TargetFormat::Bc7Rgba,
    TargetFormat::Pvrtc1_4Rgb,
    TargetFormat::Pvrtc1_4Rgba,
    TargetFormat::Astc4x4Rgba,
    TargetFormat::AtcRgb,
    TargetFormat::AtcRgba,
    TargetFormat::Rgba32,
    TargetFormat::Rgb565,
    TargetFormat::Bgr565,
    TargetFormat::Rgba4444,
    TargetFormat::Fxt1Rgb,
    TargetFormat::Pvrtc2_4Rgb,
    TargetFormat::Pvrtc2_4Rgba,
    TargetFormat::EacR11,
    TargetFormat::EacRg11,
    TargetFormat::Bc6h,
    TargetFormat::AstcHdr4x4Rgba,
    TargetFormat::RgbHalf,
    TargetFormat::RgbaHalf,
    TargetFormat::Rgb9e5,
    TargetFormat::AstcHdr6x6Rgba,
    TargetFormat::AstcLdr5x4Rgba,
    TargetFormat::AstcLdr5x5Rgba,
    TargetFormat::AstcLdr6x5Rgba,
    TargetFormat::AstcLdr6x6Rgba,
    TargetFormat::AstcLdr8x5Rgba,
    TargetFormat::AstcLdr8x6Rgba,
    TargetFormat::AstcLdr10x5Rgba,
    TargetFormat::AstcLdr10x6Rgba,
    TargetFormat::AstcLdr8x8Rgba,
    TargetFormat::AstcLdr10x8Rgba,
    TargetFormat::AstcLdr10x10Rgba,
    TargetFormat::AstcLdr12x10Rgba,
    TargetFormat::AstcLdr12x12Rgba,
];

/// Every source codec the matrix enumerates: the three 4x4-era codecs, the
/// 14 raw ASTC LDR footprints, and raw ASTC HDR 6x6.
fn sources() -> Vec<SourceFormat> {
    let mut v = vec![
        SourceFormat::Etc1s,
        SourceFormat::UastcLdr,
        SourceFormat::UastcHdr4x4,
    ];
    v.extend(AstcBlock::ALL.map(SourceFormat::AstcLdr));
    v.push(SourceFormat::AstcHdr6x6);
    v.push(SourceFormat::UastcHdr6x6);
    v.extend(AstcBlock::ALL.map(SourceFormat::XuastcLdr));
    v
}

/// Whether a `(source, target)` is wired up and corpus-proven byte-identical to
/// the pinned upstream (currently v2_1_0).
fn is_done(source: SourceFormat, t: TargetFormat) -> bool {
    use TargetFormat::*;
    // The HDR source reaches only the five HDR targets; all five are done.
    if source == SourceFormat::UastcHdr4x4 {
        return matches!(t, Bc6h | AstcHdr4x4Rgba | RgbHalf | RgbaHalf | Rgb9e5);
    }
    // Raw ASTC LDR sources are done for the matching-block-size pass-through,
    // the four uncompressed targets (including deblock filtering), the reused
    // BCn/EAC encoders, the ETC1/ETC2 packers, PVRTC1, and BC7.
    if let SourceFormat::AstcLdr(b) = source {
        return t == b.passthrough_target()
            || matches!(
                t,
                Rgba32
                    | Rgb565
                    | Bgr565
                    | Rgba4444
                    | Bc1Rgb
                    | Bc3Rgba
                    | Bc4R
                    | Bc5Rg
                    | EacR11
                    | EacRg11
                    | Etc1Rgb
                    | Etc2Rgba
                    | Pvrtc1_4Rgb
                    | Pvrtc1_4Rgba
                    | Bc7Rgba
            );
    }
    // XUASTC LDR: the ASTC pass-through and every generic LDR target route
    // through the decompressor plus the byte-proven raw-ASTC slice machinery.
    // BC7 is handled by the dedicated fast tile arms just below.
    if let SourceFormat::XuastcLdr(b) = source {
        // BC7 is done for every source: the 4x4 per-block and 8x6/6x6 tile
        // fast arms at default flags, the generic pixel encoder for the
        // HQ/deblock/fast-disabled variants and the other block sizes.
        if t == Bc7Rgba {
            return true;
        }
        return t == b.passthrough_target()
            || matches!(
                t,
                Rgba32
                    | Rgb565
                    | Bgr565
                    | Rgba4444
                    | Bc1Rgb
                    | Bc3Rgba
                    | Bc4R
                    | Bc5Rg
                    | EacR11
                    | EacRg11
                    | Etc1Rgb
                    | Etc2Rgba
                    | Pvrtc1_4Rgb
                    | Pvrtc1_4Rgba
            );
    }
    // The 6x6 HDR sources: all five HDR targets are done. AstcHdr6x6 reaches
    // AstcHdr6x6Rgba by pass-through; BC6H goes through fast_encode_bc6h and
    // the half/9E5 rasters; the UastcHdr6x6 intermediate decompressor feeds
    // the same slice paths.
    if matches!(source, SourceFormat::AstcHdr6x6 | SourceFormat::UastcHdr6x6) {
        return matches!(t, AstcHdr6x6Rgba | Bc6h | RgbHalf | RgbaHalf | Rgb9e5);
    }
    // ATC and PVRTC2 only exist from ETC1S (UASTC does not support them, so
    // is_format_supported filters those rows out before they reach here).
    if source == SourceFormat::Etc1s && matches!(t, AtcRgb | AtcRgba) {
        return true;
    }
    if source == SourceFormat::Etc1s && matches!(t, Pvrtc2_4Rgb | Pvrtc2_4Rgba) {
        return true;
    }
    // FXT1 (8x4) is ETC1S-only; UASTC doesn't support it so it never reaches here.
    if source == SourceFormat::Etc1s && t == Fxt1Rgb {
        return true;
    }
    matches!(
        t,
        Etc1Rgb
            | Etc2Rgba
            | Bc1Rgb
            | Bc3Rgba
            | Bc4R
            | Bc5Rg
            | Bc7Rgba
            | Pvrtc1_4Rgb
            | Pvrtc1_4Rgba
            | Astc4x4Rgba
            | Rgba32
            | Rgb565
            | Bgr565
            | Rgba4444
            | EacR11
            | EacRg11
    )
}

/// The decode-flag combinations to test for a target. `HighQuality` (32)
/// changes the UASTC to BCn and EAC output, so those targets are tested both
/// ways.
fn flag_variants(source: SourceFormat, t: TargetFormat) -> &'static [u32] {
    use TargetFormat::*;
    match (source, t) {
        (SourceFormat::UastcLdr, Bc1Rgb | Bc3Rgba | Bc4R | Bc5Rg | Bc7Rgba | EacR11 | EacRg11) => {
            &[0, 32]
        }
        // ETC1S to BC7: default chroma filtering on (0) and the opt-out (64).
        (SourceFormat::Etc1s, Bc7Rgba) => &[0, 64],
        // Raw ASTC to non-ASTC targets: the three deblock flags change output
        // bytes (128 disables the default-on filter for >8x6 sources, 512
        // forces it on for the small sources, 256 upgrades the tap math where
        // it isn't already stronger by size).
        (SourceFormat::AstcLdr(_), Rgba32 | Rgb565 | Bgr565 | Rgba4444) => &[0, 128, 256, 512],
        // The reused BCn/EAC encoders additionally honor HIGH_QUALITY (32);
        // BC4 and EAC-R11 also honor alpha-to-opaque (4, channel select).
        (SourceFormat::AstcLdr(_), Bc1Rgb | Bc3Rgba | Bc5Rg | EacRg11) => &[0, 32, 128, 512],
        (SourceFormat::AstcLdr(_), Bc4R | EacR11) => &[0, 4, 32, 128, 512],
        // BC7 (bc7f): HIGH_QUALITY selects the partially-analytical pipelines.
        (SourceFormat::AstcLdr(_), Bc7Rgba) => &[0, 32, 128, 512],
        // PVRTC1: alpha-to-opaque changes the RGB target's output.
        (SourceFormat::AstcLdr(_), Pvrtc1_4Rgb) => &[0, 4, 128, 512],
        (SourceFormat::AstcLdr(_), Pvrtc1_4Rgba) => &[0, 128, 512],
        // ETC1 honors alpha-to-opaque (4, output changes on alpha files);
        // ETC2's alpha half honors HIGH_QUALITY.
        (SourceFormat::AstcLdr(_), Etc1Rgb) => &[0, 4, 128, 512],
        (SourceFormat::AstcLdr(_), Etc2Rgba) => &[0, 32, 128, 512],
        // The 6x6 HDR sources to BC6H: HIGH_QUALITY turns on the encoder's
        // 2-subset search, changing output bytes.
        (SourceFormat::AstcHdr6x6 | SourceFormat::UastcHdr6x6, Bc6h) => &[0, 32],
        // XUASTC mirrors the raw-ASTC flag surfaces (the deblock flags gate
        // on the source block size the same way; alpha-to-opaque and
        // HIGH_QUALITY reach the same re-encoders).
        (SourceFormat::XuastcLdr(_), Rgba32 | Rgb565 | Bgr565 | Rgba4444) => &[0, 128, 256, 512],
        (SourceFormat::XuastcLdr(_), Bc1Rgb | Bc3Rgba | Bc5Rg | EacRg11) => &[0, 32, 128, 512],
        (SourceFormat::XuastcLdr(_), Bc4R | EacR11) => &[0, 4, 32, 128, 512],
        (SourceFormat::XuastcLdr(_), Pvrtc1_4Rgb) => &[0, 4, 128, 512],
        (SourceFormat::XuastcLdr(_), Pvrtc1_4Rgba) => &[0, 128, 512],
        (SourceFormat::XuastcLdr(_), Etc1Rgb) => &[0, 4, 128, 512],
        (SourceFormat::XuastcLdr(_), Etc2Rgba) => &[0, 32, 128, 512],
        // BC7: HIGH_QUALITY and the deblock flags reroute to the generic
        // pixel encoder; 1024 disables the fast tile arms.
        (SourceFormat::XuastcLdr(_), Bc7Rgba) => &[0, 32, 128, 512, 1024],
        _ => &[0],
    }
}

/// A label naming the code area responsible for a target family. Used only to
/// group matrix rows in the coverage report.
fn owner(t: TargetFormat) -> &'static str {
    use TargetFormat::*;
    match t {
        Etc2Rgba | Bc7Rgba | Astc4x4Rgba | Rgba32 | Etc1Rgb => "core",
        Bc1Rgb => "bc1",
        Bc4R => "bc4",
        Bc3Rgba | Bc5Rg => "bc3-bc5",
        Pvrtc1_4Rgb => "pvrtc1-rgb",
        Pvrtc1_4Rgba => "pvrtc1-rgba",
        Pvrtc2_4Rgb | Pvrtc2_4Rgba => "pvrtc2",
        AtcRgb | AtcRgba => "atc",
        Fxt1Rgb => "fxt1",
        EacR11 | EacRg11 => "eac",
        Rgb565 | Bgr565 | Rgba4444 => "packed",
        Bc6h | AstcHdr4x4Rgba | RgbHalf | RgbaHalf | Rgb9e5 => "uastc-hdr",
        AstcHdr6x6Rgba | AstcLdr5x4Rgba | AstcLdr5x5Rgba | AstcLdr6x5Rgba | AstcLdr6x6Rgba
        | AstcLdr8x5Rgba | AstcLdr8x6Rgba | AstcLdr10x5Rgba | AstcLdr10x6Rgba | AstcLdr8x8Rgba
        | AstcLdr10x8Rgba | AstcLdr10x10Rgba | AstcLdr12x10Rgba | AstcLdr12x12Rgba => "astc-pass",
        // `TargetFormat` is non-exhaustive (upstream grows); a target this
        // harness does not know yet must show up loudly in the report, not
        // silently join a family.
        _ => "UNKNOWN-TARGET",
    }
}

/// A label naming the code area for a raw-ASTC-source row: the pass-through
/// is its own area ("astc-pass"); an ASTC LDR source re-encoded to another
/// target is "raw-astc"; a 6x6 HDR source is "astc-hdr-6x6".
fn raw_astc_owner(source: SourceFormat, t: TargetFormat) -> &'static str {
    match source {
        SourceFormat::AstcLdr(b) if t == b.passthrough_target() => "astc-pass",
        SourceFormat::AstcLdr(_) => "raw-astc",
        SourceFormat::AstcHdr6x6 if t == TargetFormat::AstcHdr6x6Rgba => "astc-pass",
        _ => "astc-hdr-6x6",
    }
}

/// Build the full matrix: every valid (source, target), with flag variants,
/// status, and owner.
pub fn matrix() -> Vec<Case> {
    let mut v = Vec::new();
    for &source in &sources() {
        for &target in ALL_TARGETS {
            if !is_format_supported(target, source) {
                continue;
            }
            v.push(Case {
                source,
                target,
                flags: flag_variants(source, target),
                status: if is_done(source, target) {
                    Status::Done
                } else {
                    Status::Todo
                },
                owner: if source == SourceFormat::Etc1s && target == TargetFormat::Bc7Rgba {
                    "bc7-chroma"
                } else if matches!(source, SourceFormat::AstcLdr(_) | SourceFormat::AstcHdr6x6) {
                    raw_astc_owner(source, target)
                } else {
                    owner(target)
                },
            });
        }
    }
    v
}
