//! Global-codebook conformance: the Rust global-codebook decode must match the
//! C++ oracle's `set_global_codebooks` path byte-for-byte.
//!
//! No corpus file uses a global codebook (they are all self-contained) and there
//! is no encoder to make one, so this synthesizes a dependent file from every
//! self-contained ETC1S `.basis` by setting the global-codebook flag, then
//! supplies that same file's own codebook. Both sides decode the synthesized
//! file, the Rust `Transcoder::new_with_codebook` against the oracle's
//! `basisu_transcoder::set_global_codebooks`, and every (target, level, flag)
//! must agree exactly.

use basisu::{DecodeFlags, SourceFormat, Transcoder};
use conformance::matrix::{matrix, Status};
use conformance::{corpus_files, OracleBasis};

/// `cBASISHeaderFlagUsesGlobalCodebook`, bit 3 of the little-endian `m_flags`
/// field at header offset 21.
const FLAG_USES_GLOBAL_CODEBOOK: u8 = 8;
const BASIS_TEX_FORMAT_ETC1S: u32 = 0;
const BASIS_TEX_TYPE_VIDEO: u32 = 3;

/// A copy of `bytes` with the global-codebook flag set.
fn as_global_codebook_file(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[21] |= FLAG_USES_GLOBAL_CODEBOOK;
    out
}

/// Synthesize a global-codebook file from each self-contained ETC1S `.basis`
/// donor, then assert the Rust global-codebook decode matches the oracle's
/// `set_global_codebooks` path for every done ETC1S row, level, and flag.
#[test]
fn global_codebook_matches_oracle() {
    let done: Vec<_> = matrix()
        .into_iter()
        .filter(|c| c.status == Status::Done && c.source == SourceFormat::Etc1s)
        .collect();
    let files = corpus_files();
    assert!(
        !files.is_empty(),
        "no corpus files found (need corpus/smoke)"
    );

    let (mut files_seen, mut passes) = (0usize, 0u64);

    for path in &files {
        if path.extension().is_none_or(|x| x != "basis") {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        // Self-contained ETC1S only: the codebook donor. Skip video (its frames
        // need the stateful path; the global-codebook mechanism is orthogonal
        // and covered on the still images).
        let Some(donor) = OracleBasis::open(&bytes) else {
            continue;
        };
        if donor.info.tex_format != BASIS_TEX_FORMAT_ETC1S
            || donor.info.tex_type == BASIS_TEX_TYPE_VIDEO
        {
            continue;
        }

        let codebook = Transcoder::new(&bytes)
            .expect("open self-contained ETC1S")
            .etc1s_codebook()
            .expect("self-contained ETC1S exposes its codebook");

        let global_bytes = as_global_codebook_file(&bytes);
        let rust = Transcoder::new_with_codebook(&global_bytes, &codebook)
            .unwrap_or_else(|e| panic!("rust global open failed on {path:?}: {e:?}"));
        let oracle = OracleBasis::open_with_global_codebook(&bytes, &global_bytes)
            .unwrap_or_else(|| panic!("oracle global open failed on {path:?}"));

        files_seen += 1;
        for case in &done {
            for level in 0..oracle.info.levels {
                for &flag in case.flags {
                    let Some(want) = oracle.transcode(0, level, case.target.as_i32(), flag) else {
                        continue;
                    };
                    let got = rust
                        .transcode(level, case.target, DecodeFlags::from_bits(flag))
                        .unwrap_or_else(|e| {
                            panic!(
                                "rust {:?} L{level} flags={flag} on {path:?}: {e:?}",
                                case.target
                            )
                        });
                    assert_eq!(
                        got, want.data,
                        "{:?} L{level} flags={flag} differs on {path:?}",
                        case.target
                    );
                    passes += 1;
                }
            }
        }
    }

    assert!(files_seen > 0, "no ETC1S .basis donor files in the corpus");
    eprintln!("global codebook: {files_seen} files, {passes} byte-identical passes vs the oracle");
}
