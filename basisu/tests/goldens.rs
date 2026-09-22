//! Golden regression test that needs no C++ toolchain to run.
//!
//! Replays the committed `goldens/manifest.tsv`, which holds the SHA-256 of the
//! expected transcoder output for each (asset, target, flags, level) row, and
//! asserts the Rust transcoder reproduces every hash exactly. The manifest is
//! regenerated with `cargo xtask bake-goldens`; any change in output shows up as
//! a diff in the committed hashes.

#![cfg(not(target_arch = "wasm32"))]

use basisu::{DecodeFlags, TargetFormat, Transcoder};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Lowercase hex SHA-256 of `bytes`, in the form stored in the manifest.
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Transcode every manifest row and assert the output length and SHA-256 match
/// the committed golden values.
#[test]
fn rust_matches_committed_goldens() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("../goldens/manifest.tsv"))
        .expect("read goldens/manifest.tsv (run `cargo xtask bake-goldens`)");

    let mut checked = 0usize;
    let mut skipped = 0usize;
    let mut cache: Option<(String, Vec<u8>)> = None;

    for line in manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 6, "bad manifest row: {line}");
        let (asset, target, flags, level, len, sha) = (
            f[0],
            f[1].parse::<i32>().unwrap(),
            f[2].parse::<u32>().unwrap(),
            f[3].parse::<u32>().unwrap(),
            f[4].parse::<usize>().unwrap(),
            f[5],
        );

        // Load each asset once (rows are grouped by asset).
        if cache.as_ref().map(|(n, _)| n.as_str()) != Some(asset) {
            let data = std::fs::read(root.join("../corpus/smoke").join(asset))
                .unwrap_or_else(|e| panic!("read smoke asset {asset}: {e}"));
            cache = Some((asset.to_string(), data));
        }
        let data = &cache.as_ref().unwrap().1;

        let tex = Transcoder::new(data).expect("open asset");
        let fmt = TargetFormat::from_i32(target).expect("known target");
        // A build without the row's codec feature refuses the pair; the rows it
        // does compile must still be byte-perfect.
        if !tex.supports(fmt) {
            assert!(
                matches!(
                    tex.transcode(level, fmt, DecodeFlags::from_bits(flags)),
                    Err(basisu::Error::Unsupported { .. })
                ),
                "{asset} {fmt:?}: an unsupported pair must be refused, not decoded"
            );
            skipped += 1;
            continue;
        }
        let got = tex
            .transcode(level, fmt, DecodeFlags::from_bits(flags))
            .unwrap_or_else(|e| panic!("rust transcode {asset} {fmt:?} L{level}: {e:?}"));

        assert_eq!(got.len(), len, "{asset} {fmt:?} L{level} length");
        assert_eq!(
            sha256_hex(&got),
            sha,
            "{asset} {fmt:?} L{level} flags={flags} sha256"
        );
        checked += 1;
    }
    assert!(checked > 0, "no golden rows checked");
    let every_codec = cfg!(all(
        feature = "astc-ldr",
        feature = "xuastc",
        feature = "hdr",
        feature = "bc",
        feature = "etc",
        feature = "eac",
        feature = "astc",
        feature = "pvrtc1",
        feature = "pvrtc2",
        feature = "atc",
        feature = "fxt1",
        feature = "packed",
    ));
    assert!(
        !every_codec || skipped == 0,
        "a build with every codec feature must check every row ({skipped} skipped)"
    );
    eprintln!(
        "goldens: {checked} rows byte-perfect (pure Rust, no C++), {skipped} skipped by features"
    );
}
