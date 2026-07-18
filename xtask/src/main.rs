//! Dev orchestration for the basisu workspace. Run via `cargo xtask <command>`:
//!
//! ```text
//! doctor        Report what this machine can build/run and how to fix gaps.
//! verify        Full gate: conformance (byte-equality vs the C++ oracle) then
//!               the benchmarks vs the local pre-opt baseline, if one exists.
//! check         Fast pure-Rust path (no C++ toolchain): fmt + clippy + golden replay.
//! wasm          Run the wasm32 runtime tests under node.
//! qemu          Run the bare-metal harnesses on emulated Cortex-M3 and M0.
//! all           Everything: check + verify + wasm + qemu. Hard-fails on
//!               missing tooling (see doctor) rather than skipping.
//! ci            The full local gate, run manually on this machine (this
//!               project uses no cloud CI): all + MSRV + no_std + package
//!               dry-run, with a timestamped report under target/ci/.
//! install-hooks Wire git's pre-push hook to `cargo xtask check`.
//! coverage      Print the conformance-matrix coverage report.
//! corpus        Fetch/link the full conformance corpus.
//! bake-goldens  Re-bake goldens/manifest.tsv from the oracle (after vendoring).
//! vendor <ref>  Re-vendor the upstream Basis Universal transcoder at <ref>.
//! ```

use std::process::{Command, ExitCode};

/// One `doctor` line: is `what` usable, and if not, what to do about it.
/// `required` failures fail the doctor exit code; optional gaps only inform.
fn doctor_line(ok: bool, required: bool, what: &str, fix: &str) -> bool {
    let mark = match (ok, required) {
        (true, _) => "ok  ",
        (false, true) => "MISS",
        (false, false) => "opt ",
    };
    if ok {
        eprintln!("  [{mark}] {what}");
    } else {
        eprintln!("  [{mark}] {what} - {fix}");
    }
    ok || !required
}

/// True if `cmd` can be spawned (used as an existence probe via `--version`).
fn cmd_exists(cmd: &str, arg: &str) -> bool {
    Command::new(cmd)
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Environment report: every capability the workspace's gates use, each with
/// its remedy. Exit code fails only on the hard requirements, so `doctor` can
/// gate scripts while still surfacing the optional tooling.
fn doctor() -> bool {
    eprintln!("xtask doctor: workspace environment report\n");
    eprintln!("required for `cargo xtask check` (pure Rust) and the crate itself:");
    let mut ok = doctor_line(
        cmd_exists("cargo", "--version"),
        true,
        "cargo / rustc",
        "install via rustup: https://rustup.rs",
    );

    eprintln!("\nrequired for `cargo xtask verify` / `bake-goldens` (C++ oracle):");
    ok &= doctor_line(
        cmd_exists("c++", "--version") || cmd_exists("g++", "--version") || cmd_exists("clang++", "--version"),
        true,
        "C++17 compiler (c++/g++/clang++)",
        "Debian/Ubuntu: apt install build-essential; Fedora: dnf install gcc-c++; macOS: xcode-select --install",
    );
    // Light-weight probe mirroring conformance/build.rs::find_zstd (which is
    // authoritative; it also prints a cargo warning when it comes up empty).
    let zstd = std::env::var("ZSTD_LIB_DIR").is_ok()
        || cmd_exists("pkg-config", "--version")
            && Command::new("pkg-config")
                .args(["--exists", "libzstd"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        || [
            "/opt/homebrew/opt/zstd/include",
            "/usr/local/include",
            "/usr/include",
        ]
        .iter()
        .any(|d| std::path::Path::new(d).join("zstd.h").exists());
    ok &= doctor_line(
        zstd,
        false,
        "system zstd dev files (oracle ground truth for zstd-supercompressed levels)",
        "Debian/Ubuntu: apt install libzstd-dev; Fedora: dnf install libzstd-devel; \
         macOS: brew install zstd; or set ZSTD_INCLUDE_DIR + ZSTD_LIB_DIR",
    );

    eprintln!("\nfull-corpus conformance (headline numbers; smoke corpus alone still gates):");
    ok &= doctor_line(
        ["cts", "gltf", "binomial"]
            .iter()
            .all(|d| std::path::Path::new("corpus").join(d).is_dir()),
        false,
        "full corpus (corpus/cts, corpus/gltf, corpus/binomial)",
        "run `cargo xtask corpus` (~220 MB download, git required)",
    );

    eprintln!("\noptional platform checks (`cargo xtask qemu` / `cargo xtask wasm`):");
    let targets = installed_targets();
    ok &= doctor_line(
        targets.contains("thumbv7m-none-eabi") && targets.contains("thumbv6m-none-eabi"),
        false,
        "bare-metal targets for qemu-check/ (thumbv7m + thumbv6m)",
        "rustup target add thumbv7m-none-eabi thumbv6m-none-eabi",
    );
    ok &= doctor_line(
        cmd_exists("qemu-system-arm", "--version"),
        false,
        "qemu-system-arm (runs qemu-check/ + qemu-check-m0/ on emulated Cortex-M)",
        "Debian/Ubuntu: apt install qemu-system-arm; macOS: brew install qemu",
    );
    ok &= doctor_line(
        targets.contains("wasm32-unknown-unknown"),
        false,
        "wasm32-unknown-unknown target (basisu/tests/wasm.rs)",
        "rustup target add wasm32-unknown-unknown",
    );
    ok &= doctor_line(
        cmd_exists("wasm-bindgen-test-runner", "--version"),
        false,
        "wasm-bindgen-test-runner 0.2.118 (pairs with the pinned wasm-bindgen-test 0.3.68)",
        "cargo install wasm-bindgen-cli --version 0.2.118 (also needs node)",
    );
    ok &= doctor_line(
        cmd_exists("node", "--version"),
        false,
        "node (executes the wasm tests)",
        "Debian/Ubuntu: apt install nodejs; macOS: brew install node",
    );

    eprintln!();
    ok
}

/// The `rustup target list --installed` output (empty if rustup is absent).
fn installed_targets() -> String {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Fail fast with the doctor-style remedy when a `wasm`/`qemu` prerequisite is
/// missing, so those commands hard-fail with the fix instead of a cryptic
/// cargo/runner error.
fn preflight(ok: bool, what: &str, fix: &str) -> bool {
    if !ok {
        eprintln!("xtask: missing {what} - {fix} (see `cargo xtask doctor`)");
    }
    ok
}

/// True if a criterion `pre-opt` baseline exists anywhere under
/// `target/criterion` (criterion nests benchmark ids as directories, so the
/// depth varies; a shallow bounded walk is enough).
fn has_pre_opt_baseline(dir: &std::path::Path, depth: u32) -> bool {
    if depth == 0 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "pre-opt") {
                return true;
            }
            if has_pre_opt_baseline(&p, depth - 1) {
                return true;
            }
        }
    }
    false
}

/// The full gate: workspace-wide clippy, then conformance (byte-equality vs
/// the C++ oracle), then the benchmarks. `--bench transcode` targets only the
/// criterion bench (it skips the lib unittest harness, which rejects criterion
/// flags). `--baseline-lenient` compares to the machine-local `pre-opt`
/// baseline if present, otherwise it just runs without erroring, so a fresh
/// checkout with no local baseline still passes the correctness gate. The
/// oracle's C++ is already required here, so this is where the whole workspace
/// (conformance + xtask + basisu, every target) gets clippy-gated, not just
/// the pure-Rust `check` subset.
fn cmd_verify() -> bool {
    if !has_pre_opt_baseline(std::path::Path::new("target/criterion"), 6) {
        eprintln!(
            "xtask: note: no local `pre-opt` bench baseline, so the benchmarks will run \
             without a regression comparison. Create one with \
             `cargo bench -p basisu --bench transcode -- --save-baseline pre-opt`."
        );
    }
    run(&[
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]) && run(&["cargo", "test", "-p", "conformance", "--release"])
        && run(&[
            "cargo",
            "bench",
            "-p",
            "basisu",
            "--bench",
            "transcode",
            "--",
            "--baseline-lenient",
            "pre-opt",
        ])
}

/// Pure-Rust, no C++ toolchain: the fast contributor / CI path. Clippy covers
/// all targets (tests and examples, not just the library) of both pure-Rust
/// crates; the conformance crate's clippy needs the C++ oracle, so it rides
/// `verify` instead.
fn cmd_check() -> bool {
    run(&["cargo", "fmt", "--check"])
        && run(&[
            "cargo",
            "clippy",
            "-p",
            "basisu",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ])
        && run(&[
            "cargo",
            "clippy",
            "-p",
            "xtask",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ])
        && run(&["cargo", "test", "-p", "basisu", "--test", "goldens"])
}

/// The MSRV toolchain name, derived from basisu/Cargo.toml's `rust-version`
/// (e.g. "1.73" -> "1.73.0", the form rustup names toolchains with).
fn msrv_toolchain() -> Option<String> {
    let manifest = std::fs::read_to_string("basisu/Cargo.toml").ok()?;
    let ver = manifest
        .lines()
        .find_map(|l| l.strip_prefix("rust-version"))?
        .split('"')
        .nth(1)?
        .to_string();
    Some(if ver.matches('.').count() == 1 {
        format!("{ver}.0")
    } else {
        ver
    })
}

/// The MSRV gate: the published crate builds on its declared minimum Rust, in
/// both the default and the no_std (libm) configurations.
fn cmd_msrv() -> bool {
    let Some(tc) = msrv_toolchain() else {
        eprintln!("xtask: cannot read rust-version from basisu/Cargo.toml");
        return false;
    };
    let installed = Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&tc))
        .unwrap_or(false);
    let plus = format!("+{tc}");
    preflight(
        installed,
        &format!("the {tc} MSRV toolchain"),
        &format!("rustup toolchain install {tc}"),
    ) && run(&["cargo", &plus, "check", "-p", "basisu"])
        && run(&[
            "cargo",
            &plus,
            "check",
            "-p",
            "basisu",
            "--no-default-features",
            "--features",
            "libm",
        ])
}

/// The no_std gate on the current toolchain: both `#![no_std]` feature
/// configurations of the published crate build.
fn cmd_no_std() -> bool {
    run(&[
        "cargo",
        "check",
        "-p",
        "basisu",
        "--no-default-features",
        "--features",
        "libm",
    ]) && run(&[
        "cargo",
        "check",
        "-p",
        "basisu",
        "--no-default-features",
        "--features",
        "libm,zstd",
    ])
}

/// The packaging gate: `cargo package` still succeeds (README present, no
/// stray or missing files), so publish-time rot is caught continuously.
/// `--allow-dirty` keeps it runnable mid-work; `--offline` keeps the gate
/// hermetic (no index fetch; it needs the committed Cargo.lock and a local
/// cargo cache, which any machine that built the workspace has). The real
/// publish runs strict and online.
fn cmd_package() -> bool {
    run(&[
        "cargo",
        "package",
        "-p",
        "basisu",
        "--allow-dirty",
        "--offline",
    ])
}

/// The full local gate. This project runs no cloud CI: `ci` runs every gate
/// on this machine and writes a timestamped report under `target/ci/`. All
/// gates run even after a failure, so one report shows the full picture; the
/// exit code fails if any gate failed.
fn cmd_ci() -> bool {
    let started = std::time::SystemTime::now();
    let stamp = started
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    type Gate = fn() -> bool;
    let gates: [(&str, Gate); 7] = [
        ("check", cmd_check),
        ("verify", cmd_verify),
        ("wasm", cmd_wasm),
        ("qemu", cmd_qemu),
        ("msrv", cmd_msrv),
        ("no_std", cmd_no_std),
        ("package", cmd_package),
    ];
    let mut results = Vec::new();
    for (name, gate) in gates {
        eprintln!("\nxtask ci: ===== {name} =====");
        let t = std::time::Instant::now();
        let ok = gate();
        results.push((name, ok, t.elapsed().as_secs()));
    }

    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let mut report = format!("cargo xtask ci (epoch {stamp})\n{rustc}\n\n");
    for (name, ok, secs) in &results {
        report.push_str(&format!(
            "  [{}] {name:8} {secs:>5}s\n",
            if *ok { "pass" } else { "fail" }
        ));
    }
    let all_ok = results.iter().all(|(_, ok, _)| *ok);
    report.push_str(if all_ok {
        "\nresult: pass\n"
    } else {
        "\nresult: fail\n"
    });

    eprintln!("\n{report}");
    let dir = std::path::Path::new("target/ci");
    if std::fs::create_dir_all(dir).is_ok() {
        let path = dir.join(format!("ci-{stamp}.log"));
        if std::fs::write(&path, &report).is_ok() {
            eprintln!("xtask ci: report written to {}", path.display());
        }
    }
    all_ok
}

/// The pre-push hook `install-hooks` writes: the fast pure-Rust gate runs
/// before anything leaves this machine (`git push --no-verify` bypasses it in
/// an emergency).
const PRE_PUSH_HOOK: &str = "#!/bin/sh\n\
    # Installed by `cargo xtask install-hooks`: runs the fast pure-Rust gate\n\
    # before every push. Bypass in an emergency with `git push --no-verify`.\n\
    echo 'pre-push: cargo xtask check'\n\
    exec cargo xtask check\n";

/// Wire git's pre-push hook to `cargo xtask check`. Refuses to overwrite a
/// hook it did not install.
fn cmd_install_hooks() -> bool {
    let git_dir = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let Some(git_dir) = git_dir else {
        eprintln!("xtask: not a git repository");
        return false;
    };
    let path = std::path::Path::new(&git_dir).join("hooks/pre-push");
    match std::fs::read_to_string(&path) {
        Ok(existing) if existing == PRE_PUSH_HOOK => {
            eprintln!("xtask: pre-push hook already installed");
            return true;
        }
        Ok(_) => {
            eprintln!(
                "xtask: {} exists and was not installed by xtask; not overwriting",
                path.display()
            );
            return false;
        }
        Err(_) => {}
    }
    if let Err(e) = std::fs::write(&path, PRE_PUSH_HOOK) {
        eprintln!("xtask: writing {}: {e}", path.display());
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)) {
            eprintln!("xtask: chmod {}: {e}", path.display());
            return false;
        }
    }
    eprintln!(
        "xtask: installed {} (runs `cargo xtask check` on every push)",
        path.display()
    );
    true
}

/// The wasm32 runtime tests (basisu/tests/wasm.rs) under node, via the
/// `wasm-bindgen-test-runner` configured in `.cargo/config.toml`.
fn cmd_wasm() -> bool {
    preflight(
        installed_targets().contains("wasm32-unknown-unknown"),
        "the wasm32-unknown-unknown target",
        "rustup target add wasm32-unknown-unknown",
    ) && preflight(
        cmd_exists("wasm-bindgen-test-runner", "--version"),
        "wasm-bindgen-test-runner",
        "cargo install wasm-bindgen-cli --version 0.2.118",
    ) && preflight(
        cmd_exists("node", "--version"),
        "node",
        "Debian/Ubuntu: apt install nodejs; macOS: brew install node",
    ) && run(&[
        "cargo",
        "test",
        "-p",
        "basisu",
        "--test",
        "wasm",
        "--target",
        "wasm32-unknown-unknown",
    ])
}

/// The bare-metal harnesses under QEMU: qemu-check/ (Cortex-M3, a real
/// end-to-end transcode) and qemu-check-m0/ (Cortex-M0, the no-native-atomics
/// OnceBox path). Each is its own single-crate workspace whose
/// `.cargo/config.toml` sets the target and the QEMU runner, so `cargo run`
/// inside the directory is the whole invocation.
fn cmd_qemu() -> bool {
    let targets = installed_targets();
    preflight(
        targets.contains("thumbv7m-none-eabi") && targets.contains("thumbv6m-none-eabi"),
        "the thumbv7m/thumbv6m bare-metal targets",
        "rustup target add thumbv7m-none-eabi thumbv6m-none-eabi",
    ) && preflight(
        cmd_exists("qemu-system-arm", "--version"),
        "qemu-system-arm",
        "Debian/Ubuntu: apt install qemu-system-arm; macOS: brew install qemu",
    ) && run_in("qemu-check", &["cargo", "run", "--release"])
        && run_in("qemu-check-m0", &["cargo", "run", "--release"])
}

/// Run `args` as a subprocess, echoing the command line first. True on a
/// zero exit status; false if the process fails to spawn or exits non-zero.
fn run(args: &[&str]) -> bool {
    eprintln!("xtask: $ {}", args.join(" "));
    Command::new(args[0])
        .args(&args[1..])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Like [`run`], but from inside `dir` (for the qemu-check crates, which are
/// their own workspaces and must be built from their own directories).
fn run_in(dir: &str, args: &[&str]) -> bool {
    eprintln!("xtask: $ (cd {dir} && {})", args.join(" "));
    Command::new(args[0])
        .args(&args[1..])
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Dispatch the single `cargo xtask` subcommand, exiting nonzero if any step
/// of its command sequence fails.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let ok = match cmd {
        "doctor" => doctor(),
        // Correctness first (the hard gate), then performance.
        "verify" => cmd_verify(),
        "check" => cmd_check(),
        "wasm" => cmd_wasm(),
        "qemu" => cmd_qemu(),
        // Every gate on one command. Deliberately hard-fails (rather than
        // skips) when tooling is missing, so a green `all` always means the
        // same thing on every machine.
        "all" => cmd_check() && cmd_verify() && cmd_wasm() && cmd_qemu(),
        "ci" => cmd_ci(),
        "install-hooks" => cmd_install_hooks(),
        "coverage" => run(&[
            "cargo",
            "test",
            "-p",
            "conformance",
            "--release",
            "--",
            "--nocapture",
            "coverage_report",
        ]),
        "corpus" => run(&["bash", "corpus/fetch.sh"]),
        "bake-goldens" => run(&["cargo", "run", "-p", "conformance", "--bin", "bake-goldens"]),
        "vendor" => match args.get(1) {
            Some(r) => run(&["bash", "tools/vendor.sh", r]),
            None => {
                eprintln!("usage: cargo xtask vendor <git-ref>");
                false
            }
        },
        other => {
            eprintln!(
                "unknown command {other:?}\n  \
                 doctor | verify | check | wasm | qemu | all | ci | install-hooks | \
                 coverage | corpus | bake-goldens | vendor <ref>"
            );
            false
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
