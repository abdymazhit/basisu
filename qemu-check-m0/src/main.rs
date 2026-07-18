//! Exercise `once::OnceBox` on an emulated Cortex-M0 (no native atomics), where
//! it uses portable-atomic with a critical section instead of core atomics. The
//! full transcoder's working set does not fit the micro:bit's 16K RAM, so this
//! targets the exact thing the M0 path changes: the CAS-based one-shot init.
//! Output via semihosting; exit 0 on pass.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use core::mem::MaybeUninit;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use embedded_alloc::Heap;
use panic_semihosting as _;

use basisu::once::OnceBox;

/// Allocator backing the `Box` inside `OnceBox`; a small static arena is
/// plenty for the one 256-byte table this test stores.
#[global_allocator]
static HEAP: Heap = Heap::empty();

/// The cell under test, in a static just like the transcoder's cached tables.
static CELL: OnceBox<[u32; 64]> = OnceBox::new();

/// Initialize the heap, run `get_or_init` twice against `CELL`, and exit via
/// semihosting with pass/fail.
#[entry]
fn main() -> ! {
    const HEAP_SIZE: usize = 2 * 1024;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    // First call builds and stores the value via the portable-atomic CAS.
    let built = CELL.get_or_init(|| {
        let mut a = [0u32; 64];
        for (i, v) in a.iter_mut().enumerate() {
            *v = (i as u32).wrapping_mul(2654435761);
        }
        Box::new(a)
    });
    // Second call must observe the already-stored value, not re-run init.
    let again = CELL.get_or_init(|| Box::new([0u32; 64]));

    let ok = core::ptr::eq(built, again)
        && built[10] == 10u32.wrapping_mul(2654435761)
        && again[63] == 63u32.wrapping_mul(2654435761);

    hprintln!(
        "Cortex-M0 (portable-atomic CAS): OnceBox init {} -> {}",
        if core::ptr::eq(built, again) { "once" } else { "TWICE" },
        if ok { "pass" } else { "fail" }
    );
    debug::exit(if ok {
        debug::EXIT_SUCCESS
    } else {
        debug::EXIT_FAILURE
    });
    loop {}
}
