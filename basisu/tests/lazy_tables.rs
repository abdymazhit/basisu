//! The ETC1S solution tables can arrive at runtime as a bundle: the bundle is
//! the embedded tables byte for byte, a target is refused until its family is
//! installed, and a transcode after install matches the embedded build.
use basisu::{
    bundle_family, decode_bundle, install_tables, tables_installed, DecodeFlags, TableFamily,
    TablesError, TargetFormat, Transcoder,
};
use sha2::{Digest, Sha256};

const ETC1S: &[u8] = include_bytes!("fixtures/etc1s.ktx2");
const ASTC_BUNDLE: &[u8] = include_bytes!("fixtures/tables/astc.bin");
const BC_BUNDLE: &[u8] = include_bytes!("fixtures/tables/bc.bin");

// The embedded build's output for the fixture: the runtime build must match it.
const ASTC_SHA256: &str = "087190f3b403db7c91e8729d33e9cb66b62abfff8f771e5974844be6575d3941";
const BC7_SHA256: &str = "5fd316a27dfa37d6150717b1e5efe234f75155b750bc22216268a82e82babef4";
const BC1_SHA256: &str = "788925bc9bbcec3d2331ee16fda3181cfea258c890d91a9fa9924183403194cf";

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn transcode(target: TargetFormat) -> Vec<u8> {
    Transcoder::new(ETC1S)
        .unwrap()
        .transcode(0, target, DecodeFlags::NONE)
        .unwrap()
}

#[test]
fn the_shipped_bundles_are_the_embedded_tables_byte_for_byte() {
    for (bundle, family) in [
        (ASTC_BUNDLE, TableFamily::Astc),
        (BC_BUNDLE, TableFamily::Bc),
    ] {
        assert_eq!(bundle_family(bundle), Ok(family));
        assert_eq!(bundle.len(), family.bundle_len());
        #[cfg(feature = "embedded-tables")]
        assert_eq!(bundle, basisu::embedded_table_bundle(family).as_slice());
        let (decoded_family, tables) = decode_bundle(bundle).unwrap();
        assert_eq!(decoded_family, family);
        assert_eq!(tables.len(), family.table_count());
    }
}

#[test]
fn a_bad_bundle_is_refused_with_the_reason() {
    assert_eq!(bundle_family(b"nope"), Err(TablesError::BadMagic));
    let mut wrong_family = ASTC_BUNDLE.to_vec();
    wrong_family[4] = 9;
    assert_eq!(
        bundle_family(&wrong_family),
        Err(TablesError::UnknownFamily(9))
    );
    assert_eq!(
        bundle_family(&ASTC_BUNDLE[..ASTC_BUNDLE.len() - 1]),
        Err(TablesError::BadLength {
            expected: TableFamily::Astc.bundle_len(),
            actual: ASTC_BUNDLE.len() - 1,
        })
    );
}

/// Without `embedded-tables` a family's targets are refused until its bundle
/// is installed; with it they are always available. Either way the transcode
/// after install is the embedded build's output.
#[test]
fn a_target_is_refused_until_its_tables_are_installed_and_then_matches() {
    let texture = Transcoder::new(ETC1S).unwrap();
    let embedded = cfg!(feature = "embedded-tables");
    assert_eq!(tables_installed(TableFamily::Astc), embedded);
    assert_eq!(texture.supports(TargetFormat::Astc4x4Rgba), embedded);
    assert_eq!(texture.supports(TargetFormat::Bc7Rgba), embedded);
    // RGBA32 and ETC1 need no tables.
    assert!(texture.supports(TargetFormat::Rgba32));
    assert!(texture.supports(TargetFormat::Etc1Rgb));
    if !embedded {
        assert!(texture
            .transcode(0, TargetFormat::Astc4x4Rgba, DecodeFlags::NONE)
            .is_err());
    }
    assert_eq!(install_tables(ASTC_BUNDLE), Ok(TableFamily::Astc));
    assert_eq!(install_tables(BC_BUNDLE), Ok(TableFamily::Bc));
    // A second install keeps the first tables (idempotent, no error).
    assert_eq!(install_tables(ASTC_BUNDLE), Ok(TableFamily::Astc));
    assert!(tables_installed(TableFamily::Astc) && tables_installed(TableFamily::Bc));
    assert!(texture.supports(TargetFormat::Astc4x4Rgba));
    assert_eq!(sha256(&transcode(TargetFormat::Astc4x4Rgba)), ASTC_SHA256);
    assert_eq!(sha256(&transcode(TargetFormat::Bc7Rgba)), BC7_SHA256);
    assert_eq!(sha256(&transcode(TargetFormat::Bc1Rgb)), BC1_SHA256);
}

/// Regenerates the shipped bundles from the embedded tables:
/// `cargo test --test lazy_tables -- --ignored write_bundles`.
#[cfg(feature = "embedded-tables")]
#[test]
#[ignore]
fn write_bundles() {
    for family in TableFamily::ALL {
        let name = match family {
            TableFamily::Astc => "astc",
            TableFamily::Bc => "bc",
        };
        std::fs::write(
            format!(
                "{}/tests/fixtures/tables/{name}.bin",
                env!("CARGO_MANIFEST_DIR")
            ),
            basisu::embedded_table_bundle(family),
        )
        .unwrap();
    }
}

/// Prints the embedded build's hashes for the constants above:
/// `cargo test --test lazy_tables -- --ignored --nocapture print_hashes`.
#[cfg(feature = "embedded-tables")]
#[test]
#[ignore]
fn print_hashes() {
    println!("ASTC {}", sha256(&transcode(TargetFormat::Astc4x4Rgba)));
    println!("BC7 {}", sha256(&transcode(TargetFormat::Bc7Rgba)));
    println!("BC1 {}", sha256(&transcode(TargetFormat::Bc1Rgb)));
}
