//! Build script for the bare-metal QEMU harness. It copies the `memory.x`
//! linker script (the mps2-an385 flash/RAM map) into `OUT_DIR` and puts that
//! directory on the linker search path, which is how `cortex-m-rt` locates the
//! memory layout at link time.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Emits `memory.x` into `OUT_DIR` and puts that directory on the linker
/// search path, then reruns only when `memory.x` changes.
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    // Let the linker find memory.x in OUT_DIR.
    println!("cargo:rustc-link-search={}", out.display());
    // Re-link if the memory layout changes.
    println!("cargo:rerun-if-changed=memory.x");
}
