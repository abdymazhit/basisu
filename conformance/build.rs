//! Compiles the vendored Basis Universal transcoder + the oracle wrapper into a
//! static lib. `oracle.cpp` `#include`s `basisu_transcoder.cpp` into its own
//! translation unit (to reach the transcoder's file-static tables), so that
//! .cpp must not be compiled separately.
//!
//! `-ffp-contract=off` is mandatory: it disables FMA contraction so the C++
//! float math (e.g. the BC7 pbit / EAC error searches `a - b*c`) matches
//! portable non-contracted IEEE f32, exactly what the Rust port computes. The
//! Rust transcoder is the deterministic source of truth; the oracle is built to
//! match it.

/// Locate a system zstd (header dir + lib dir) so the oracle can be built with
/// `BASISD_SUPPORT_KTX2_ZSTD=1` and produce ground truth for zstd-supercompressed
/// levels. Honors `ZSTD_INCLUDE_DIR` / `ZSTD_LIB_DIR`, else asks `pkg-config`
/// (which knows distro-specific layouts such as Debian/Ubuntu multiarch
/// `lib/x86_64-linux-gnu`), else probes Homebrew and common system prefixes,
/// including their `lib64` and multiarch subdirectories. Returns `None` if not
/// found (the oracle then keeps zstd off and the harness counts zstd files as
/// `zstd-no-oracle` skips; they stay gated by the committed goldens).
fn find_zstd() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    use std::path::PathBuf;
    let has_header = |dir: &PathBuf| dir.join("zstd.h").exists();
    let has_lib = |dir: &PathBuf| {
        ["libzstd.a", "libzstd.dylib", "libzstd.so"]
            .iter()
            .any(|name| dir.join(name).exists())
    };

    if let (Ok(inc), Ok(lib)) = (
        std::env::var("ZSTD_INCLUDE_DIR"),
        std::env::var("ZSTD_LIB_DIR"),
    ) {
        return Some((PathBuf::from(inc), PathBuf::from(lib)));
    }

    // pkg-config reports the actual install layout, so try it first.
    let pkg_config_var = |var: &str| -> Option<PathBuf> {
        let out = std::process::Command::new("pkg-config")
            .args(["--variable", var, "libzstd"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    };
    if let (Some(inc), Some(lib)) = (pkg_config_var("includedir"), pkg_config_var("libdir")) {
        if has_header(&inc) && has_lib(&lib) {
            return Some((inc, lib));
        }
    }

    // Homebrew (Apple Silicon + Intel) and common system prefixes. For each,
    // consider `lib`, `lib64` (Fedora/RHEL), and every immediate subdirectory
    // of `lib` (Debian/Ubuntu multiarch, e.g. `lib/x86_64-linux-gnu`).
    for prefix in [
        "/opt/homebrew/opt/zstd",
        "/usr/local/opt/zstd",
        "/opt/homebrew",
        "/usr/local",
        "/usr",
    ] {
        let prefix = PathBuf::from(prefix);
        let inc = prefix.join("include");
        if !has_header(&inc) {
            continue;
        }
        let mut lib_dirs = vec![prefix.join("lib"), prefix.join("lib64")];
        if let Ok(entries) = std::fs::read_dir(prefix.join("lib")) {
            lib_dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
        if let Some(lib) = lib_dirs.into_iter().find(|dir| has_lib(dir)) {
            return Some((inc, lib));
        }
    }
    None
}

/// Compile the oracle static library, probing for a system zstd first and
/// emitting the cargo link/cfg directives that go with it.
fn main() {
    let zstd = find_zstd();
    if zstd.is_none() {
        // Not fatal (zstd corpus files then stay gated by the committed
        // goldens), but say so loudly: a silently zstd-less oracle hides a
        // coverage gap. Install the zstd dev package (Debian/Ubuntu:
        // libzstd-dev, Fedora: libzstd-devel, macOS: brew install zstd) or set
        // ZSTD_INCLUDE_DIR / ZSTD_LIB_DIR.
        println!(
            "cargo:warning=conformance oracle built WITHOUT zstd \
             (no system libzstd found): zstd-supercompressed corpus files will \
             be skipped as `zstd-no-oracle` instead of oracle-gated"
        );
    }

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        // Release-semantics oracle: C asserts abort the whole test process
        // (uncatchable), and the reference's shipped behavior on malformed
        // streams is the error return after the assert (e.g. arith
        // decode_gamma's runaway-prefix guard). Parity targets the release
        // behavior.
        .define("NDEBUG", None)
        .define("BASISD_SUPPORT_KTX2", "1")
        .define(
            "BASISD_SUPPORT_KTX2_ZSTD",
            if zstd.is_some() { "1" } else { "0" },
        )
        .flag_if_supported("-fno-strict-aliasing")
        .flag_if_supported("-fvisibility=hidden")
        .flag_if_supported("-ffp-contract=off")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("-Wno-deprecated-copy")
        .flag_if_supported("-Wno-deprecated-builtins")
        .flag_if_supported("-Wno-deprecated-declarations")
        .flag_if_supported("-Wno-class-memaccess")
        .include("csrc")
        .include("csrc/basisu_transcoder")
        .file("csrc/oracle.cpp");

    // When a system zstd is available, add its include dir (so our shim's
    // `#include <zstd.h>` resolves) and link the library. This lets the oracle
    // decompress KTX2 zstd-supercompressed levels as ground truth. `csrc/zstd`
    // is deliberately kept off the include path: the vendored transcoder finds
    // our shim via the relative `"../zstd/zstd.h"` include, and the shim's
    // angle-bracket include must resolve to the system header, not itself.
    if let Some((inc, _lib)) = &zstd {
        build.include(inc);
    }

    build.compile("basisu_oracle");

    if let Some((_inc, lib)) = &zstd {
        println!("cargo:rustc-link-search=native={}", lib.display());
        println!("cargo:rustc-link-lib=zstd");
        println!("cargo:rustc-cfg=oracle_zstd");
    }

    println!("cargo:rustc-check-cfg=cfg(oracle_zstd)");
    println!("cargo:rerun-if-changed=csrc/oracle.cpp");
    println!("cargo:rerun-if-changed=csrc/zstd/zstd.h");
    println!("cargo:rerun-if-env-changed=ZSTD_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=ZSTD_LIB_DIR");
    println!("cargo:rerun-if-changed=csrc/basisu_transcoder/basisu_transcoder.cpp");
    println!("cargo:rerun-if-changed=csrc/basisu_transcoder/basisu_transcoder.h");
}
