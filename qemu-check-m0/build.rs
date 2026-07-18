//! Copies the `memory.x` linker script into `OUT_DIR` and puts that directory
//! on the linker search path, so the bare-metal Cortex-M0 link picks up the
//! memory layout. Reruns only when `memory.x` changes.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Emits `memory.x` into `OUT_DIR` along with the matching `cargo:` directives.
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
}
