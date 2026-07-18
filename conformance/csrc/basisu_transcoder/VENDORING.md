# Vendored: Basis Universal transcoder

This directory contains a **verbatim, transcoder-only subset** of
[Basis Universal](https://github.com/BinomialLLC/basis_universal). It is the
golden oracle the pure-Rust `basisu` crate is diffed against by the
`conformance` crate. It is dev-only: nothing in this directory ships in the
published crate.

## Provenance

- **Upstream**: BinomialLLC/basis_universal, `transcoder/` directory.
- **Pinned ref**: see `UPSTREAM_VERSION` in this directory.
- **Obtained via**: `cargo xtask vendor <ref>` (`tools/vendor.sh`), which
  copies the upstream `transcoder/` file set verbatim.
- **License**: Apache-2.0 (see `LICENSE`, copied verbatim from upstream).

## What was vendored (and what was not)

The transcoder-only file set: every `*.h`, `*.cpp`, `*.inl`, and `*.inc` from
upstream `transcoder/` (one `.cpp`, `basisu_transcoder.cpp`, which `#include`s
everything else it needs), plus the upstream `LICENSE`. The encoder, the
`basisu_tool` CLI, and the bundled zstd are deliberately excluded: this repo
transcodes only, and zstd comes from the system when available (see below).

The upstream files are **unmodified** byte-for-byte copies. All project code
lives outside this directory: the oracle wrapper is `../oracle.cpp`, which
`#include`s `basisu_transcoder.cpp` into its own translation unit so it can
export the transcoder's file-static tables and internal entry points to the
Rust test harness.

## How it is built

`conformance/build.rs` compiles `csrc/oracle.cpp` (and, through the include,
this directory's transcoder) with the `cc` crate into the `basisu_oracle`
static library. The build contract that defines byte parity with the Rust
port:

- `-std=c++17` (v2 requires it: constexpr-if, `std::size`)
- `-ffp-contract=off` (no FMA contraction, so float math matches the Rust
  port's portable IEEE f32 arithmetic)
- `BASISD_SUPPORT_KTX2=1`
- `BASISD_SUPPORT_KTX2_ZSTD=1` when a system zstd is found (Homebrew,
  pkg-config, or `ZSTD_INCLUDE_DIR`/`ZSTD_LIB_DIR`), else `0`; with `0` the
  harness gates zstd-supercompressed files against the committed goldens
  instead of the oracle.

## Updating

Run `cargo xtask vendor <new-ref>`, update `UPSTREAM_VERSION`, then
`cargo xtask verify`: the conformance suite localizes any output change to
exactly the (format, file) rows that diverged. Re-port the Rust side until
green, then `cargo xtask bake-goldens` to refresh the committed goldens.
