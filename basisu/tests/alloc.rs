//! Allocation bounds: a size field in the file must not size an allocation
//! before it is checked against what the container's dimensions imply. A
//! counting allocator records the largest single request each test thread
//! makes; a corrupt size that used to be allocated as declared shows up here
//! even when the transcode then fails cleanly.

#![cfg(not(target_arch = "wasm32"))]

use basisu::{DecodeFlags, TargetFormat, Transcoder};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// The system allocator, recording the largest request of each thread.
struct Largest;

thread_local! {
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

fn record(size: usize) {
    let _ = LARGEST.try_with(|l| l.set(l.get().max(size)));
}

// SAFETY: every call forwards to `System` with the caller's arguments; the
// bookkeeping touches only a thread-local `Cell` and never allocates.
unsafe impl GlobalAlloc for Largest {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: Largest = Largest;

/// The largest single allocation `f` makes on this thread.
fn largest_allocation(f: impl FnOnce()) -> usize {
    LARGEST.with(|l| l.set(0));
    f();
    LARGEST.with(|l| l.get())
}

/// The little-endian u32 / u64 at byte `at` of `file`.
fn read_u32(file: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(file[at..at + 4].try_into().unwrap())
}
fn read_u64(file: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(file[at..at + 8].try_into().unwrap())
}

/// KTX2 header fields: `sgdByteOffset` (u64) and level 0's `byteOffset` and
/// `byteLength` (u64 each).
const KTX2_SGD_BYTE_OFFSET: usize = 64;
const KTX2_LEVEL0_BYTE_OFFSET: usize = 80;
const KTX2_LEVEL0_BYTE_LENGTH: usize = 88;

/// No test here needs more than this in one piece (the largest output is a
/// 768x512 RGBA32 image, 1.5 MiB).
const BOUND: usize = 16 << 20;

/// A UASTC HDR 6x6 intermediate stream codes its own pixel size (u16 width and
/// height after the u16 signature). The decoder allocated its block buffer
/// from that size, up to 32768 x 32768 (half a gigabyte), and compared it with
/// the container's only after decoding. The size must match the level's
/// before anything is allocated.
#[test]
#[cfg(feature = "hdr")]
fn a_6x6_stream_header_cannot_size_the_block_buffer() {
    let file = include_bytes!("../../corpus/smoke/uastc_hdr_6x6i_mip.ktx2");
    let sgd = read_u64(file, KTX2_SGD_BYTE_OFFSET) as usize;
    let level0 = read_u64(file, KTX2_LEVEL0_BYTE_OFFSET) as usize;
    let stream = level0 + read_u32(file, sgd) as usize;
    assert_eq!(read_u32(file, stream + 2), 16 | 16 << 16);

    let mut data = file.to_vec();
    data[stream + 2..stream + 6].copy_from_slice(&(32768u32 | 32768 << 16).to_le_bytes());
    let t = Transcoder::new(&data).unwrap();
    let largest = largest_allocation(|| {
        assert!(t
            .transcode(0, TargetFormat::RgbaHalf, DecodeFlags::NONE)
            .is_err());
    });
    assert!(largest < BOUND, "{largest} bytes in one allocation");
}

/// An XUASTC side channel is a zstd frame whose header declares its content
/// size, up to `i32::MAX`. The channel decoder allocated that size up front;
/// it now grows the output with what the frame decodes. The level's first
/// channel frame is rebuilt to declare 2 GiB with a 4-byte content size and a
/// 128 KiB window (the stream, its slice desc and the level grow by the 3
/// header bytes this adds).
#[test]
#[cfg(all(feature = "xuastc", feature = "zstd"))]
fn an_xuastc_channel_cannot_size_its_buffer_by_its_header() {
    let file = include_bytes!("../../corpus/smoke/xuastc_zstd_12x12.ktx2");
    let sgd = read_u64(file, KTX2_SGD_BYTE_OFFSET) as usize;
    let level0 = read_u64(file, KTX2_LEVEL0_BYTE_OFFSET) as usize;
    let level0_len = read_u64(file, KTX2_LEVEL0_BYTE_LENGTH) as usize;
    // Level 0 holds the one image and ends the file, so growing it moves
    // nothing else.
    assert_eq!(level0 + level0_len, file.len());
    let stream = level0 + read_u32(file, sgd) as usize;
    assert_eq!(file[stream], 2, "full-zstd syntax");
    // The 85-byte stream header, the raw bits, then the mode channel.
    let frame = stream + 85 + read_u32(file, stream + 1) as usize;
    assert_eq!(file[frame..frame + 5], [0x28, 0xB5, 0x2F, 0xFD, 0x60]);

    let huge = [0x28, 0xB5, 0x2F, 0xFD, 0x80, 0x38, 0xFF, 0xFF, 0xFF, 0x7F];
    let mut data = file[..frame].to_vec();
    data.extend_from_slice(&huge);
    data.extend_from_slice(&file[frame + 7..]);
    let grow = |data: &mut Vec<u8>, at: usize, width: usize| {
        let mut v = [0u8; 8];
        v[..width].copy_from_slice(&data[at..at + width]);
        let v = u64::from_le_bytes(v) + 3;
        data[at..at + width].copy_from_slice(&v.to_le_bytes()[..width]);
    };
    grow(&mut data, stream + 5, 4);
    grow(&mut data, sgd + 4, 4);
    grow(&mut data, KTX2_LEVEL0_BYTE_LENGTH, 8);

    let t = Transcoder::new(&data).unwrap();
    let largest = largest_allocation(|| {
        let _ = t.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE);
    });
    assert!(largest < BOUND, "{largest} bytes in one allocation");
}
