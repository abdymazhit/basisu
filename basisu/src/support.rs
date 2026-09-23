//! The source-codec by target-format validity matrix. With every codec feature
//! on, ETC1S can reach every LDR target and UASTC LDR lacks only a few niche
//! ones. HDR targets are reachable only from an HDR source, and vice versa: the
//! LDR sources carry no HDR data and the HDR sources have no LDR decode path.
//! A pair whose codec feature is off is unsupported, whatever the matrix says.

use crate::api::{SourceFormat, TargetFormat};

/// Whether this build compiled the decoder for `source` (ETC1S and UASTC LDR
/// always are).
fn source_compiled(source: SourceFormat) -> bool {
    match source {
        SourceFormat::Etc1s | SourceFormat::UastcLdr => true,
        SourceFormat::UastcHdr4x4 | SourceFormat::AstcHdr6x6 | SourceFormat::UastcHdr6x6 => {
            cfg!(feature = "hdr")
        }
        SourceFormat::AstcLdr(_) => cfg!(feature = "astc-ldr"),
        SourceFormat::XuastcLdr(_) => cfg!(feature = "xuastc"),
    }
}

/// Whether this build compiled the encoder for `target` (RGBA32 always is).
/// The ASTC pass-throughs need no encoder: a raw ASTC source copies through
/// to its own block size regardless of the `astc` feature.
fn target_compiled(target: TargetFormat, source: SourceFormat) -> bool {
    use TargetFormat::*;
    let passthrough = match source {
        SourceFormat::AstcLdr(b) | SourceFormat::XuastcLdr(b) => target == b.passthrough_target(),
        _ => false,
    };
    if passthrough {
        return true;
    }
    match target {
        Rgba32 => true,
        Etc1Rgb | Etc2Rgba => cfg!(feature = "etc"),
        Bc1Rgb | Bc3Rgba | Bc4R | Bc5Rg | Bc7Rgba => cfg!(feature = "bc"),
        EacR11 | EacRg11 => cfg!(feature = "eac"),
        Astc4x4Rgba => cfg!(feature = "astc"),
        Pvrtc1_4Rgb | Pvrtc1_4Rgba => cfg!(feature = "pvrtc1"),
        Pvrtc2_4Rgb | Pvrtc2_4Rgba => cfg!(feature = "pvrtc2"),
        AtcRgb | AtcRgba => cfg!(feature = "atc"),
        Fxt1Rgb => cfg!(feature = "fxt1"),
        Rgb565 | Bgr565 | Rgba4444 => cfg!(feature = "packed"),
        Bc6h | AstcHdr4x4Rgba | RgbHalf | RgbaHalf | Rgb9e5 | AstcHdr6x6Rgba => {
            cfg!(feature = "hdr")
        }
        AstcLdr5x4Rgba | AstcLdr5x5Rgba | AstcLdr6x5Rgba | AstcLdr6x6Rgba | AstcLdr8x5Rgba
        | AstcLdr8x6Rgba | AstcLdr10x5Rgba | AstcLdr10x6Rgba | AstcLdr8x8Rgba | AstcLdr10x8Rgba
        | AstcLdr10x10Rgba | AstcLdr12x10Rgba | AstcLdr12x12Rgba => {
            cfg!(feature = "astc-ldr")
        }
    }
}

/// Whether `target` is one of the ASTC pass-through targets (any block size,
/// LDR or HDR). `is_format_supported` uses this to single out the non-4x4 LDR
/// pass-throughs, which no ETC1S or UASTC path can produce: only the ASTC LDR
/// source of the matching block size reaches them.
fn is_astc_target(target: TargetFormat) -> bool {
    use TargetFormat::*;
    matches!(
        target,
        Astc4x4Rgba
            | AstcHdr6x6Rgba
            | AstcLdr5x4Rgba
            | AstcLdr5x5Rgba
            | AstcLdr6x5Rgba
            | AstcLdr6x6Rgba
            | AstcLdr8x5Rgba
            | AstcLdr8x6Rgba
            | AstcLdr10x5Rgba
            | AstcLdr10x6Rgba
            | AstcLdr8x8Rgba
            | AstcLdr10x8Rgba
            | AstcLdr10x10Rgba
            | AstcLdr12x10Rgba
            | AstcLdr12x12Rgba
    )
}

/// Whether `target` can be transcoded from `source` by this build.
pub fn is_format_supported(target: TargetFormat, source: SourceFormat) -> bool {
    use TargetFormat::*;
    if !source_compiled(source) || !target_compiled(target, source) {
        return false;
    }
    // An ETC1S target whose solution tables are supplied at runtime is
    // unsupported until they are installed (`tables::lazy`).
    if !crate::tables::lazy::ready_for(target, source) {
        return false;
    }
    let hdr_target = matches!(
        target,
        Bc6h | AstcHdr4x4Rgba | AstcHdr6x6Rgba | RgbHalf | RgbaHalf | Rgb9e5
    );
    // The non-4x4 ASTC LDR pass-through targets are reachable only from the ASTC
    // LDR source of the matching block size (raw ASTC, or the XUASTC intermediate
    // that defers to the same rule).
    let non4x4_ldr_astc = is_astc_target(target) && !hdr_target && target != Astc4x4Rgba;
    match source {
        // XUASTC LDR reaches exactly the raw-ASTC LDR target set: the 15
        // LDR targets plus its own block size's ASTC pass-through. It defers to
        // the raw-ASTC LDR rule for its block size.
        #[allow(clippy::needless_return)]
        SourceFormat::XuastcLdr(b) => {
            return crate::support::is_format_supported(target, SourceFormat::AstcLdr(b));
        }
        // UASTC LDR supports every LDR target except PVRTC2, ATC, FXT1, and the
        // non-4x4 ASTC pass-throughs.
        SourceFormat::UastcLdr => {
            !hdr_target
                && !non4x4_ldr_astc
                && !matches!(
                    target,
                    Pvrtc2_4Rgb | Pvrtc2_4Rgba | AtcRgb | AtcRgba | Fxt1Rgb
                )
        }
        // ETC1S reaches every LDR target except the non-4x4 ASTC pass-throughs.
        SourceFormat::Etc1s => !hdr_target && !non4x4_ldr_astc,
        // UASTC HDR 4x4 reaches exactly the 4x4-era HDR targets.
        SourceFormat::UastcHdr4x4 => {
            matches!(target, Bc6h | AstcHdr4x4Rgba | RgbHalf | RgbaHalf | Rgb9e5)
        }
        // Raw ASTC LDR reaches 15 fixed LDR targets plus exactly the ASTC
        // pass-through target of its own block size.
        SourceFormat::AstcLdr(b) => {
            target == b.passthrough_target()
                || matches!(
                    target,
                    Etc1Rgb
                        | Etc2Rgba
                        | Bc1Rgb
                        | Bc3Rgba
                        | Bc4R
                        | Bc5Rg
                        | Bc7Rgba
                        | EacR11
                        | EacRg11
                        | Pvrtc1_4Rgb
                        | Pvrtc1_4Rgba
                        | Rgba32
                        | Rgb565
                        | Bgr565
                        | Rgba4444
                )
        }
        // The 6x6 HDR sources (raw ASTC and the UASTC intermediate) reach the
        // 6x6 pass-through and the shared HDR targets; ASTC HDR 4x4 is not
        // valid from the 6x6 sources.
        SourceFormat::AstcHdr6x6 | SourceFormat::UastcHdr6x6 => {
            matches!(target, AstcHdr6x6Rgba | Bc6h | RgbHalf | RgbaHalf | Rgb9e5)
        }
    }
}
