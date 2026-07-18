//! Transcode embedded `.ktx2` fixtures on an emulated Cortex-M3 and check the
//! output against values computed on the host. Proves the pure-Rust no_std
//! transcoder runs correctly on real bare-metal, not just that it builds. Both
//! source codecs are exercised: ETC1S, and UASTC (which drives the atomic
//! lazy-table init, `OnceBox`, on the chip).

#![no_std]
#![no_main]

extern crate alloc;

use core::mem::MaybeUninit;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use embedded_alloc::Heap;
use panic_semihosting as _;

use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};

/// Heap backing `alloc`; the transcoder allocates its tables and output here.
#[global_allocator]
static HEAP: Heap = Heap::empty();

// Fixtures + expected `*.ktx2` -> BC7 level 0 checksums, computed on the host.
const ETC1S: &[u8] = include_bytes!("../../basisu/tests/fixtures/etc1s.ktx2");
const UASTC: &[u8] = include_bytes!("../../basisu/tests/fixtures/uastc.ktx2");
const ETC1S_FNV: u64 = 0x8e13f3fea6487113;
const UASTC_FNV: u64 = 0x503cc57b8a28cbff;

/// FNV-1a 64-bit hash of `bytes`, used as a compact checksum of transcoded
/// output so the expected value fits in a single host-computed constant.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Transcode one fixture to BC7 level 0 and check it matches the host value.
fn check(name: &str, data: &[u8], want_codec: SourceFormat, want_fnv: u64) -> bool {
    let t = match Transcoder::new(data) {
        Ok(t) => t,
        Err(_) => {
            hprintln!("{}: open failed", name);
            return false;
        }
    };
    let out = match t.transcode(0, TargetFormat::Bc7Rgba, DecodeFlags::NONE) {
        Ok(v) => v,
        Err(_) => {
            hprintln!("{}: transcode failed", name);
            return false;
        }
    };
    let got = fnv1a(&out);
    let ok = t.source_format() == want_codec && got == want_fnv;
    hprintln!(
        "{} -> BC7 L0: len={} fnv={:#018x} -> {}",
        name,
        out.len(),
        got,
        if ok { "pass" } else { "fail" }
    );
    ok
}

/// Firmware entry point: initialize the heap, run both fixture checks, then
/// exit through the semihosting debug interface with pass/fail.
#[entry]
fn main() -> ! {
    const HEAP_SIZE: usize = 48 * 1024;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    // bitwise & rather than && so the second check still runs and prints its
    // result when the first fails
    let ok = check("etc1s.ktx2", ETC1S, SourceFormat::Etc1s, ETC1S_FNV)
        & check("uastc.ktx2", UASTC, SourceFormat::UastcLdr, UASTC_FNV);

    hprintln!("result: {}", if ok { "all pass" } else { "failed" });
    debug::exit(if ok {
        debug::EXIT_SUCCESS
    } else {
        debug::EXIT_FAILURE
    });
    loop {}
}
