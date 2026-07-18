//! Rust bindings to the C++ oracle (the vendored transcoder compiled by
//! build.rs). This is the golden reference the Rust port is diffed against.
//!
//! The `orc_ktx2_*` hooks drive the upstream public `ktx2_transcoder` for any
//! (level, target format, decode flags) combination, the ground truth for the
//! conformance matrix. The other `orc_*` leaf hooks expose internal tables and
//! per-function entry points for finer-grained checks.

use std::os::raw::{c_int, c_void};
use std::path::PathBuf;

pub mod matrix;

/// All Basis container files in the conformance corpus (`.ktx2` and `.basis`):
/// the committed `corpus/smoke` set always, plus the fetched
/// `corpus/{cts,gltf,binomial}` trees if present. The harness filters to Basis
/// payloads the oracle accepts.
pub fn corpus_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let mut out = Vec::new();
    for sub in ["smoke", "cts", "gltf", "binomial"] {
        collect_basis_files(&root.join(sub), &mut out);
    }
    out.sort();
    out
}

/// Just the committed `corpus/smoke` assets (`.ktx2` and `.basis`): the stable
/// set the golden manifest is baked from and the pure-Rust golden test replays.
pub fn smoke_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_basis_files(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/smoke"),
        &mut out,
    );
    out.sort();
    out
}

/// Collect `.ktx2` and `.basis` Basis container files under `dir`, recursively.
fn collect_basis_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    // Skip negative-test trees: the KTX-Software CTS `validate/` files are
    // intentionally malformed (they test a validator), not textures to
    // transcode, and the reference transcoder asserts or aborts on them.
    if dir.file_name().is_some_and(|n| n == "validate") {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_basis_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "ktx2" || x == "basis")
            && !p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("error_"))
        {
            out.push(p);
        }
    }
}

extern "C" {
    fn orc_ktx2_open(
        data: *const u8,
        size: u32,
        out_w: *mut u32,
        out_h: *mut u32,
        out_levels: *mut u32,
        out_has_alpha: *mut u32,
        out_is_uastc: *mut u32,
    ) -> *mut c_void;
    fn orc_ktx2_close(handle: *mut c_void);
    fn orc_ktx2_layers(handle: *mut c_void) -> u32;
    fn orc_ktx2_faces(handle: *mut c_void) -> u32;
    fn orc_ktx2_is_video(handle: *mut c_void) -> u32;
    fn orc_ktx2_level_size(
        handle: *mut c_void,
        level: u32,
        layer: u32,
        face: u32,
        fmt: c_int,
        out_w: *mut u32,
        out_h: *mut u32,
    ) -> u32;
    fn orc_ktx2_transcode(
        handle: *mut c_void,
        level: u32,
        layer: u32,
        face: u32,
        fmt: c_int,
        flags: u32,
        out: *mut u8,
        out_cap: u32,
    ) -> u32;

    fn orc_basis_open(
        data: *const u8,
        size: u32,
        out_w: *mut u32,
        out_h: *mut u32,
        out_levels: *mut u32,
        out_has_alpha: *mut u32,
        out_tex_format: *mut u32,
        out_tex_type: *mut u32,
        out_total_images: *mut u32,
    ) -> *mut c_void;
    fn orc_basis_open_with_global_codebook(
        cb_data: *const u8,
        cb_size: u32,
        file_data: *const u8,
        file_size: u32,
        out_w: *mut u32,
        out_h: *mut u32,
        out_levels: *mut u32,
        out_has_alpha: *mut u32,
        out_tex_format: *mut u32,
        out_tex_type: *mut u32,
        out_total_images: *mut u32,
    ) -> *mut c_void;
    fn orc_basis_close(handle: *mut c_void);
    fn orc_basis_level_size(
        handle: *mut c_void,
        image: u32,
        level: u32,
        fmt: c_int,
        out_w: *mut u32,
        out_h: *mut u32,
    ) -> u32;
    fn orc_basis_transcode(
        handle: *mut c_void,
        image: u32,
        level: u32,
        fmt: c_int,
        flags: u32,
        out: *mut u8,
        out_cap: u32,
    ) -> u32;

    fn orc_init();
    fn orc_bc1_match_table(which: c_int, out_len: *mut u32) -> *const u8;
    fn orc_etc1_g_to_bc7_m5a(out_len: *mut u32) -> *const u8;
    fn orc_const_table(name: *const std::os::raw::c_char, out_len: *mut u32) -> *const u8;

    fn orc_read_bits1_to_9(buf: *const u8, bit_offset: *mut u32, codesize: u32) -> u32;
    fn orc_read_bits64(buf: *const u8, bit_offset: *mut u32, codesize: u32) -> u64;
    fn orc_read_bits1_to_9_fst(buf: *const u8, bit_offset: *mut u32, codesize: u32) -> u32;
    fn orc_read_bit(buf: *const u8, bit_offset: *mut u32) -> u32;

    fn orc_uastc_mode_table(name: *const std::os::raw::c_char, out_len: *mut u32) -> *const u8;
    fn orc_uastc_flat_table(name: *const std::os::raw::c_char, out_len: *mut u32) -> *const u8;

    fn orc_partitions2(bc7: *mut u8, astc: *mut u16, invert: *mut u8);
    fn orc_partitions3(bc7: *mut u8, astc: *mut u16, perm: *mut u8);
    fn orc_bc7_3_astc2_partitions(bc73: *mut u8, astc2: *mut u16, k: *mut u8);

    fn orc_astc_get_levels(range: u32) -> u32;
    fn orc_astc_is_valid_endpoint_range(range: u32) -> u32;
    fn orc_unquant_astc_endpoint_val(packed_val: u32, range: u32) -> u32;
    fn orc_unquant_astc_endpoint(bits: u32, trits: u32, quints: u32, range: u32) -> u32;
    fn orc_astc_unquant_table(out_len: *mut u32) -> *const u8;
    fn orc_bc7_mode6_optimal(out_len: *mut u32) -> *const u8;
    fn orc_bc7_mode5_optimal(out_len: *mut u32) -> *const u8;
    fn orc_uastc_unpack_pixels(src16: *const u8, srgb: u32, out64: *mut u8) -> u32;
    fn orc_bc7f_pack_rgba(pixels64: *const u8, flags: u32, out16: *mut u8);
    fn orc_fast_encode_bc6h(halves96: *const u8, hq: u32, out16: *mut u8);
    fn orc_xuastc_idct_2d(src: *const f32, num_rows: u32, num_cols: u32, dst: *mut f32);
    fn orc_xuastc_decompress(
        comp: *const u8,
        comp_size: u32,
        out_blocks: *mut u8,
        out_blocks_cap: u32,
        out_block_w: *mut u32,
        out_block_h: *mut u32,
        out_w: *mut u32,
        out_h: *mut u32,
        out_has_alpha: *mut u32,
        out_srgb: *mut u32,
    ) -> u32;
    fn orc_decode_6x6_hdr(
        comp: *const u8,
        comp_size: u32,
        out_blocks: *mut u8,
        out_blocks_cap: u32,
        out_w: *mut u32,
        out_h: *mut u32,
    ) -> u32;
    fn orc_astc_unpack_and_decode(
        src16: *const u8,
        bw: u32,
        bh: u32,
        mode: u32,
        out: *mut u8,
    ) -> u32;
    fn orc_transcode_uastc_to_astc(src16: *const u8, out16: *mut u8) -> u32;
    fn orc_transcode_uastc_to_bc7(src16: *const u8, out16: *mut u8) -> u32;
    fn orc_transcode_uastc_to_etc1(src16: *const u8, out8: *mut u8) -> u32;
    fn orc_transcode_uastc_to_etc2_rgba(src16: *const u8, out16: *mut u8) -> u32;

    fn orc_huffman_init(code_sizes: *const u8, n: u32) -> u32;
    fn orc_huffman_lookup(out_len: *mut u32) -> *const i32;
    fn orc_huffman_tree(out_len: *mut u32) -> *const i16;
    fn orc_huffman_code_sizes(out_len: *mut u32) -> *const u8;

    fn orc_bd_new(buf: *const u8, n: u32) -> *mut c_void;
    fn orc_bd_free(h: *mut c_void);
    fn orc_bd_get_bits(h: *mut c_void, n: u32) -> u32;
    fn orc_bd_decode_vlc(h: *mut c_void, chunk_bits: u32) -> u32;
    fn orc_bd_decode_rice(h: *mut c_void, m: u32) -> u32;
    fn orc_bd_decode_truncated_binary(h: *mut c_void, n: u32) -> u32;
    fn orc_bd_decode_huffman(h: *mut c_void) -> u32;

    fn orc_etc1s_open(buf: *const u8, n: u32) -> *mut c_void;
    fn orc_etc1s_free(h: *mut c_void);
    fn orc_etc1s_ok(h: *mut c_void) -> u32;
    fn orc_etc1s_counts(h: *mut c_void, which: u32) -> u32;
    fn orc_etc1s_data(h: *mut c_void, which: u32, len: *mut u32) -> *const u8;
    fn orc_etc1s_endpoint(h: *mut c_void, i: u32, out4: *mut u8);
    fn orc_etc1s_selector(h: *mut c_void, i: u32, out8: *mut u8);
    fn orc_etc1s_dim(h: *mut c_void, which: u32) -> u32;
    fn orc_etc1s_slice(h: *mut c_void, len: *mut u32) -> *const u8;
    fn orc_etc1s_transcode_slice_etc1(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_transcode_slice_rgba32(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_transcode_slice_astc(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_transcode_slice_bc7(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_transcode_slice_eac_a8(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_has_alpha(h: *mut c_void) -> u32;
    fn orc_etc1s_alpha_slice(h: *mut c_void, len: *mut u32) -> *const u8;
    fn orc_etc1s_transcode_etc2_rgba(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_transcode_bc7_rgba(h: *mut c_void, out: *mut u8) -> u32;
    fn orc_etc1s_transcode_astc_rgba(h: *mut c_void, out: *mut u8) -> u32;

    fn orc_astc_init_table(name: *const std::os::raw::c_char, out_len: *mut u32) -> *const u8;
    fn orc_astc_pack_cem(which: u32, endpoints: *const u8, weights: *const u8, out16: *mut u8);

    fn orc_amf_new(n: u32) -> *mut c_void;
    fn orc_amf_free(h: *mut c_void);
    fn orc_amf_add(h: *mut c_void, v: i32);
    fn orc_amf_use(h: *mut c_void, i: u32);
    fn orc_amf_get(h: *mut c_void, i: u32) -> i32;
}

/// Oracle ASTC CEM pack: which 0=cem12_r2, 1=cem12_r0, 2=cem4_r2, 3=cem8_r2.
pub fn astc_pack_cem(which: u32, endpoints: &[u8; 10], weights: &[u8; 32]) -> [u8; 16] {
    let mut out = [0u8; 16];
    unsafe {
        orc_astc_pack_cem(
            which,
            endpoints.as_ptr(),
            weights.as_ptr(),
            out.as_mut_ptr(),
        )
    };
    out
}

/// Raw bytes of a named ETC1S->ASTC init-built table (calls [`init`] first).
pub fn astc_init_table(name: &str) -> &'static [u8] {
    init();
    let c = std::ffi::CString::new(name).unwrap();
    let mut len = 0u32;
    let ptr = unsafe { orc_astc_init_table(c.as_ptr(), &mut len) };
    assert!(!ptr.is_null(), "oracle: unknown astc init table {name:?}");
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Oracle `approx_move_to_front`, driven in lockstep with the Rust port.
pub struct OracleAmf {
    h: *mut c_void,
}
impl OracleAmf {
    /// New oracle move-to-front state holding `n` entries.
    pub fn new(n: u32) -> Self {
        Self {
            h: unsafe { orc_amf_new(n) },
        }
    }
    /// Append value `v` as the next entry.
    pub fn add(&self, v: i32) {
        unsafe { orc_amf_add(self.h, v) }
    }
    /// Mark entry `i` as used, moving it toward the front.
    pub fn use_index(&self, i: u32) {
        unsafe { orc_amf_use(self.h, i) }
    }
    /// Value currently at index `i`.
    pub fn get(&self, i: u32) -> i32 {
        unsafe { orc_amf_get(self.h, i) }
    }
}
impl Drop for OracleAmf {
    /// Frees the C++ approximate-move-to-front oracle handle.
    fn drop(&mut self) {
        unsafe { orc_amf_free(self.h) }
    }
}

/// An opened ETC1S KTX2 in the oracle: the parsed global-data ranges and the
/// decoded codebooks (endpoints + selectors).
pub struct OracleEtc1s {
    h: *mut c_void,
    pub ok: bool,
    pub num_endpoints: u32,
    pub num_selectors: u32,
}
impl OracleEtc1s {
    /// Open an ETC1S KTX2 file in the oracle and decode its codebooks. Check
    /// `ok` before reading any field.
    pub fn open(file: &[u8]) -> Self {
        let h = unsafe { orc_etc1s_open(file.as_ptr(), file.len() as u32) };
        let ok = unsafe { orc_etc1s_ok(h) != 0 };
        Self {
            h,
            ok,
            num_endpoints: unsafe { orc_etc1s_counts(h, 0) },
            num_selectors: unsafe { orc_etc1s_counts(h, 1) },
        }
    }
    /// Raw global-data range: 0=endpoints, 1=selectors, 2=tables.
    pub fn data(&self, which: u32) -> &[u8] {
        let mut len = 0u32;
        let p = unsafe { orc_etc1s_data(self.h, which, &mut len) };
        unsafe { std::slice::from_raw_parts(p, len as usize) }
    }
    /// Decoded endpoint i: [r5, g5, b5, inten5].
    pub fn endpoint(&self, i: u32) -> [u8; 4] {
        let mut out = [0u8; 4];
        unsafe { orc_etc1s_endpoint(self.h, i, out.as_mut_ptr()) };
        out
    }
    /// Decoded selector `i`, as 8 bytes (`selectors[4]` then `bytes[4]`).
    pub fn selector(&self, i: u32) -> [u8; 8] {
        let mut out = [0u8; 8];
        unsafe { orc_etc1s_selector(self.h, i, out.as_mut_ptr()) };
        out
    }
    /// Level-0 image-0 block dimensions (num_blocks_x, num_blocks_y).
    pub fn dims(&self) -> (u32, u32) {
        unsafe { (orc_etc1s_dim(self.h, 0), orc_etc1s_dim(self.h, 1)) }
    }
    /// Level-0 pixel dimensions (width, height).
    pub fn wh(&self) -> (u32, u32) {
        unsafe { (orc_etc1s_dim(self.h, 2), orc_etc1s_dim(self.h, 3)) }
    }
    /// Level-0 image-0 raw RGB slice data.
    pub fn slice(&self) -> &[u8] {
        let mut len = 0u32;
        let p = unsafe { orc_etc1s_slice(self.h, &mut len) };
        unsafe { std::slice::from_raw_parts(p, len as usize) }
    }
    /// Oracle `transcode_slice` to ETC1 (8 bytes/block) for level-0 image-0.
    pub fn transcode_slice_etc1(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 8];
        let ok = unsafe { orc_etc1s_transcode_slice_etc1(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Oracle `transcode_slice` to RGBA32 (width*height*4) for level-0 image-0.
    pub fn transcode_slice_rgba32(&self, width: u32, height: u32) -> Option<Vec<u8>> {
        let mut out = vec![0u8; (width * height * 4) as usize];
        let ok = unsafe { orc_etc1s_transcode_slice_rgba32(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Oracle `transcode_slice` to ASTC 4x4 (16 b/block) for level-0 image-0.
    pub fn transcode_slice_astc(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 16];
        let ok = unsafe { orc_etc1s_transcode_slice_astc(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Oracle `transcode_slice` to BC7 mode 5 color (16 b/block) for level-0 image-0.
    pub fn transcode_slice_bc7(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 16];
        let ok = unsafe { orc_etc1s_transcode_slice_bc7(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Oracle `transcode_slice` to ETC2 EAC-A8 (8 b/block) for level-0 image-0.
    pub fn transcode_slice_eac_a8(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 8];
        let ok = unsafe { orc_etc1s_transcode_slice_eac_a8(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Whether level-0 image-0 has an alpha slice.
    pub fn has_alpha(&self) -> bool {
        unsafe { orc_etc1s_has_alpha(self.h) != 0 }
    }
    /// Level-0 image-0 raw alpha slice data (empty if no alpha).
    pub fn alpha_slice(&self) -> &[u8] {
        let mut len = 0u32;
        let p = unsafe { orc_etc1s_alpha_slice(self.h, &mut len) };
        unsafe { std::slice::from_raw_parts(p, len as usize) }
    }
    /// Combined ETC1S -> ETC2_RGBA (16 b/block) for level-0 image-0 (needs alpha).
    pub fn transcode_etc2_rgba(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 16];
        let ok = unsafe { orc_etc1s_transcode_etc2_rgba(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Combined ETC1S -> BC7 mode 5 RGBA (16 b/block) for level-0 image-0.
    pub fn transcode_bc7_rgba(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 16];
        let ok = unsafe { orc_etc1s_transcode_bc7_rgba(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
    /// Combined ETC1S -> ASTC 4x4 RGBA (16 b/block) for level-0 image-0.
    pub fn transcode_astc_rgba(&self) -> Option<Vec<u8>> {
        let (bx, by) = self.dims();
        let mut out = vec![0u8; (bx * by) as usize * 16];
        let ok = unsafe { orc_etc1s_transcode_astc_rgba(self.h, out.as_mut_ptr()) };
        (ok != 0).then_some(out)
    }
}
impl Drop for OracleEtc1s {
    /// Frees the C++ ETC1S transcoder oracle handle.
    fn drop(&mut self) {
        unsafe { orc_etc1s_free(self.h) }
    }
}

/// A handle to an oracle `bitwise_decoder` over a copy of `buf`. Used to drive
/// the C++ decoder in lockstep with the Rust port (intermediate-stream diff).
pub struct OracleBd {
    h: *mut c_void,
}
impl OracleBd {
    /// A new oracle bitwise decoder over a copy of `buf`.
    pub fn new(buf: &[u8]) -> Self {
        Self {
            h: unsafe { orc_bd_new(buf.as_ptr(), buf.len() as u32) },
        }
    }
    /// Read the next `n` bits as an unsigned value.
    pub fn get_bits(&self, n: u32) -> u32 {
        unsafe { orc_bd_get_bits(self.h, n) }
    }
    /// Decode one VLC value with `chunk_bits` bits per chunk.
    pub fn decode_vlc(&self, chunk_bits: u32) -> u32 {
        unsafe { orc_bd_decode_vlc(self.h, chunk_bits) }
    }
    /// Decode one Rice-coded value with parameter `m`.
    pub fn decode_rice(&self, m: u32) -> u32 {
        unsafe { orc_bd_decode_rice(self.h, m) }
    }
    /// Decode one truncated-binary value over `n` symbols.
    pub fn decode_truncated_binary(&self, n: u32) -> u32 {
        unsafe { orc_bd_decode_truncated_binary(self.h, n) }
    }
    /// Decodes using the oracle's persistent Huffman table; call
    /// [`huffman_init`] first to set it.
    pub fn decode_huffman(&self) -> u32 {
        unsafe { orc_bd_decode_huffman(self.h) }
    }
}
impl Drop for OracleBd {
    /// Frees the C++ bitwise-decoder oracle handle.
    fn drop(&mut self) {
        unsafe { orc_bd_free(self.h) }
    }
}

/// Build a Huffman table in the oracle and return (ok, lookup, tree,
/// code_sizes) as owned copies. The oracle holds a single persistent table
/// (the one [`OracleBd::decode_huffman`] uses), so each call replaces the
/// previous table.
pub fn huffman_init(code_sizes: &[u8]) -> (bool, Vec<i32>, Vec<i16>, Vec<u8>) {
    let ok = unsafe { orc_huffman_init(code_sizes.as_ptr(), code_sizes.len() as u32) };
    let mut ll = 0u32;
    let lp = unsafe { orc_huffman_lookup(&mut ll) };
    let lookup = if lp.is_null() {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(lp, ll as usize) }.to_vec()
    };
    let mut tl = 0u32;
    let tp = unsafe { orc_huffman_tree(&mut tl) };
    let tree = if tp.is_null() {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(tp, tl as usize) }.to_vec()
    };
    let mut cl = 0u32;
    let cp = unsafe { orc_huffman_code_sizes(&mut cl) };
    let cs = if cp.is_null() {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(cp, cl as usize) }.to_vec()
    };
    (ok != 0, lookup, tree, cs)
}

/// Oracle `transcode_uastc_to_etc1`: 16-byte UASTC block -> 8-byte ETC1 block.
pub fn transcode_uastc_to_etc1(src: &[u8; 16]) -> Option<[u8; 8]> {
    let mut out = [0u8; 8];
    let ok = unsafe { orc_transcode_uastc_to_etc1(src.as_ptr(), out.as_mut_ptr()) };
    (ok != 0).then_some(out)
}

/// Oracle `transcode_uastc_to_etc2_rgba`: 16-byte UASTC block -> 16-byte ETC2.
pub fn transcode_uastc_to_etc2_rgba(src: &[u8; 16]) -> Option<[u8; 16]> {
    let mut out = [0u8; 16];
    let ok = unsafe { orc_transcode_uastc_to_etc2_rgba(src.as_ptr(), out.as_mut_ptr()) };
    (ok != 0).then_some(out)
}

/// Oracle `transcode_uastc_to_astc`: 16-byte UASTC block -> 16-byte ASTC block.
pub fn transcode_uastc_to_astc(src: &[u8; 16]) -> Option<[u8; 16]> {
    let mut out = [0u8; 16];
    let ok = unsafe { orc_transcode_uastc_to_astc(src.as_ptr(), out.as_mut_ptr()) };
    (ok != 0).then_some(out)
}

/// Oracle `transcode_uastc_to_bc7`: 16-byte UASTC block -> 16-byte BC7 block.
pub fn transcode_uastc_to_bc7(src: &[u8; 16]) -> Option<[u8; 16]> {
    let mut out = [0u8; 16];
    let ok = unsafe { orc_transcode_uastc_to_bc7(src.as_ptr(), out.as_mut_ptr()) };
    (ok != 0).then_some(out)
}

/// Oracle `unpack_uastc(blk, pixels, srgb)`: 16-byte block -> 64-byte RGBA
/// pixels. `None` if the block's mode is invalid.
pub fn uastc_unpack_pixels(src: &[u8; 16], srgb: bool) -> Option<[u8; 64]> {
    let mut out = [0u8; 64];
    let ok = unsafe { orc_uastc_unpack_pixels(src.as_ptr(), srgb as u32, out.as_mut_ptr()) };
    (ok != 0).then_some(out)
}

/// Oracle `bc7f::fast_pack_bc7_auto_rgba`: 16 RGBA pixels -> one BC7 block
/// under the given cPackBC7Flag bitmask.
pub fn bc7f_pack_rgba(pixels: &[u8; 64], flags: u32) -> [u8; 16] {
    let mut out = [0u8; 16];
    unsafe { orc_bc7f_pack_rgba(pixels.as_ptr(), flags, out.as_mut_ptr()) };
    out
}

/// The block-buffer capacity both sides of the XUASTC leaf test use. The
/// oracle hook's init callback rejects streams whose decoded block count
/// would not fit, so the Rust side must apply the same cap for the
/// accept/reject comparison to stay symmetric.
pub const XUASTC_LEAF_CAP: usize = 1 << 22;

/// The header fields an XUASTC stream reports:
/// `(block_w, block_h, width, height, has_alpha, srgb)`.
pub type XuastcHeader = (u32, u32, u32, u32, bool, bool);

/// A decompressed XUASTC image: packed physical ASTC blocks plus the stream's
/// [`XuastcHeader`].
pub type XuastcDecoded = (Vec<u8>, XuastcHeader);

/// Oracle XUASTC LDR decompress: stream -> packed physical ASTC blocks +
/// header fields. `None` when the reference rejects the stream (or it exceeds
/// [`XUASTC_LEAF_CAP`]).
pub fn xuastc_decompress(comp: &[u8]) -> Option<XuastcDecoded> {
    let mut blocks = vec![0u8; XUASTC_LEAF_CAP];
    let (mut bw, mut bh, mut w, mut h, mut alpha, mut srgb) = (0u32, 0, 0, 0, 0u32, 0u32);
    let ok = unsafe {
        orc_xuastc_decompress(
            comp.as_ptr(),
            comp.len() as u32,
            blocks.as_mut_ptr(),
            blocks.len() as u32,
            &mut bw,
            &mut bh,
            &mut w,
            &mut h,
            &mut alpha,
            &mut srgb,
        )
    };
    if ok == 0 {
        return None;
    }
    let total = (w.div_ceil(bw) as usize) * (h.div_ceil(bh) as usize) * 16;
    blocks.truncate(total);
    Some((blocks, (bw, bh, w, h, alpha != 0, srgb != 0)))
}

/// Oracle `astc_ldr_t::idct_2d`: the XUASTC weight-grid 2D inverse DCT.
pub fn xuastc_idct_2d(src: &[f32], num_rows: u32, num_cols: u32, dst: &mut [f32]) {
    assert!(src.len() >= (num_rows * num_cols) as usize);
    assert!(dst.len() >= (num_rows * num_cols) as usize);
    unsafe { orc_xuastc_idct_2d(src.as_ptr(), num_rows, num_cols, dst.as_mut_ptr()) };
}

/// Oracle `astc_6x6_hdr::decode_6x6_hdr`: decompress a UASTC HDR 6x6
/// intermediate stream to physical ASTC 6x6 blocks. Returns the block bytes
/// and coded pixel dimensions, or `None` when the reference rejects the
/// stream.
pub fn decode_6x6_hdr(comp: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // The oracle fills w and h with the coded dimensions, and it writes them
    // before it checks that the output buffer fits.
    let mut w = 0u32;
    let mut h = 0u32;
    // Try a 1 MB buffer first; on a capacity overflow the dims above give the
    // exact block count, so reallocate to it and decode once more.
    let mut blocks = vec![0u8; 1 << 20];
    let ok = unsafe {
        orc_decode_6x6_hdr(
            comp.as_ptr(),
            comp.len() as u32,
            blocks.as_mut_ptr(),
            blocks.len() as u32,
            &mut w,
            &mut h,
        )
    };
    if ok == 0 {
        if w == 0 || h == 0 {
            return None;
        }
        let need = (w.div_ceil(6) as usize) * (h.div_ceil(6) as usize) * 16;
        if need <= blocks.len() {
            return None; // real rejection, not a capacity miss
        }
        blocks = vec![0u8; need];
        let ok2 = unsafe {
            orc_decode_6x6_hdr(
                comp.as_ptr(),
                comp.len() as u32,
                blocks.as_mut_ptr(),
                blocks.len() as u32,
                &mut w,
                &mut h,
            )
        };
        if ok2 == 0 {
            return None;
        }
    }
    let total = (w.div_ceil(6) as usize) * (h.div_ceil(6) as usize) * 16;
    blocks.truncate(total);
    Some((blocks, w, h))
}

/// Oracle `astc_6x6_hdr::fast_encode_bc6h`: 16 RGB half-float texels (48
/// halves, LE bytes) -> one BC6H block. `hq` selects the HIGH_QUALITY
/// 2-subset search the 6x6 transcode path uses.
pub fn fast_encode_bc6h(halves: &[u8; 96], hq: bool, out: &mut [u8; 16]) {
    unsafe { orc_fast_encode_bc6h(halves.as_ptr(), hq as u32, out.as_mut_ptr()) };
}

/// What the oracle's ASTC unpack+decode did with a block.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AstcDecodeResult {
    /// `astc_helpers::unpack_block` rejected the physical block.
    UnpackRejected,
    /// The unpack succeeded but `decode_block` rejected the logical config
    /// (or the mode cannot represent it, e.g. an HDR block in LDR8).
    DecodeRejected,
    /// Decoded texels: `bw*bh` entries of 4 bytes (SRGB8/LDR8/RGB9E5) or 8
    /// bytes (HDR16), row-major, native layout.
    Decoded(Vec<u8>),
}

/// Oracle `astc_helpers::unpack_block` + `decode_block` on one physical ASTC
/// block of the given footprint. `mode` is the `decode_mode` value (0 SRGB8,
/// 1 LDR8, 2 HDR16, 3 RGB9E5).
pub fn astc_unpack_and_decode(src: &[u8; 16], bw: u32, bh: u32, mode: u32) -> AstcDecodeResult {
    let texel_bytes = if mode == 2 { 8 } else { 4 };
    let mut out = vec![0u8; (bw * bh) as usize * texel_bytes];
    let r = unsafe { orc_astc_unpack_and_decode(src.as_ptr(), bw, bh, mode, out.as_mut_ptr()) };
    match r {
        0 => AstcDecodeResult::UnpackRejected,
        1 => AstcDecodeResult::DecodeRejected,
        _ => AstcDecodeResult::Decoded(out),
    }
}

/// Oracle `astc_get_levels`.
pub fn astc_get_levels(range: u32) -> u32 {
    unsafe { orc_astc_get_levels(range) }
}

/// Oracle `astc_is_valid_endpoint_range`.
pub fn astc_is_valid_endpoint_range(range: u32) -> bool {
    unsafe { orc_astc_is_valid_endpoint_range(range) != 0 }
}

/// Oracle `unquant_astc_endpoint_val`.
pub fn unquant_astc_endpoint_val(packed_val: u32, range: u32) -> u32 {
    unsafe { orc_unquant_astc_endpoint_val(packed_val, range) }
}

/// Oracle `unquant_astc_endpoint`.
pub fn unquant_astc_endpoint(bits: u32, trits: u32, quints: u32, range: u32) -> u32 {
    unsafe { orc_unquant_astc_endpoint(bits, trits, quints, range) }
}

/// Raw bytes of the init-built `g_astc_unquant[21][256]` table (calls
/// [`init`] first).
pub fn astc_unquant_table() -> &'static [u8] {
    init();
    let mut len = 0u32;
    let ptr = unsafe { orc_astc_unquant_table(&mut len) };
    assert!(!ptr.is_null());
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Raw bytes of the init-built BC7 mode-6 optimal endpoints (`[256][2]`).
pub fn bc7_mode6_optimal() -> &'static [u8] {
    init();
    let mut len = 0u32;
    let ptr = unsafe { orc_bc7_mode6_optimal(&mut len) };
    assert!(!ptr.is_null());
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Raw bytes of the init-built BC7 mode-5 optimal endpoints (`[256]`).
pub fn bc7_mode5_optimal() -> &'static [u8] {
    init();
    let mut len = 0u32;
    let ptr = unsafe { orc_bc7_mode5_optimal(&mut len) };
    assert!(!ptr.is_null());
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Field columns of `g_astc_bc7_common_partitions2` (bc7, astc, invert).
pub fn partitions2() -> (Vec<u8>, Vec<u16>, Vec<u8>) {
    let (mut bc7, mut astc, mut invert) = (vec![0u8; 30], vec![0u16; 30], vec![0u8; 30]);
    unsafe { orc_partitions2(bc7.as_mut_ptr(), astc.as_mut_ptr(), invert.as_mut_ptr()) };
    (bc7, astc, invert)
}

/// Field columns of `g_astc_bc7_common_partitions3` (bc7, astc, perm).
pub fn partitions3() -> (Vec<u8>, Vec<u16>, Vec<u8>) {
    let (mut bc7, mut astc, mut perm) = (vec![0u8; 11], vec![0u16; 11], vec![0u8; 11]);
    unsafe { orc_partitions3(bc7.as_mut_ptr(), astc.as_mut_ptr(), perm.as_mut_ptr()) };
    (bc7, astc, perm)
}

/// Field columns of `g_bc7_3_astc2_common_partitions` (bc73, astc2, k).
pub fn bc7_3_astc2_partitions() -> (Vec<u8>, Vec<u16>, Vec<u8>) {
    let (mut bc73, mut astc2, mut k) = (vec![0u8; 19], vec![0u16; 19], vec![0u8; 19]);
    unsafe { orc_bc7_3_astc2_partitions(bc73.as_mut_ptr(), astc2.as_mut_ptr(), k.as_mut_ptr()) };
    (bc73, astc2, k)
}

/// Raw bytes of a named flat (padding-free) table the oracle exposes for the
/// UASTC transcode paths, exactly as the C++ holds it in memory. Panics on an
/// unknown name. Valid names span the UASTC mode/permutation/BISE tables
/// (`mode_huff_codes`, `astc_to_bc7_perm`, `bc7_to_astc_perm`,
/// `astc_bise_range_table`, `huff_modes`), the shared ASTC/BC7 partition-pattern
/// tables and their anchors (`patterns2`, `patterns3`, `bc7_3_astc2_patterns2`,
/// `pattern2_anchors`, `pattern3_anchors`, `bc7_3_astc2_patterns2_anchors`), the
/// ETC1/EAC helper tables (`etc1_inten_tables`, `eac_modifier_table`,
/// `etc1_pixel_coords`, `etc1_solid_selectors`, `etc2_eac_a8_sel4`,
/// `selector_index_to_etc1`, `etc_5_to_8`), the ASTC trit/quint encoders
/// (`astc_trit_encode`, `astc_quint_encode`), and the BC7 format descriptor
/// tables (`bc7_partition1`, `bc7_partition2`, `bc7_partition3`,
/// `bc7_anchor_second`, `bc7_anchor_third_1`, `bc7_anchor_third_2`,
/// `bc7_num_subsets`, `bc7_partition_bits`, `bc7_color_index_bitcount`,
/// `bc7_alpha_index_bitcount`, `bc7_mode_has_p_bits`,
/// `bc7_mode_has_shared_p_bits`, `bc7_color_precision`, `bc7_alpha_precision`).
pub fn uastc_flat_table(name: &str) -> &'static [u8] {
    let c = std::ffi::CString::new(name).unwrap();
    let mut len = 0u32;
    let ptr = unsafe { orc_uastc_flat_table(c.as_ptr(), &mut len) };
    assert!(!ptr.is_null(), "oracle: unknown uastc flat table {name:?}");
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Raw bytes (19) of a named UASTC per-mode property table. Valid names:
/// `weight_bits`, `weight_ranges`, `endpoint_ranges`, `subsets`, `planes`,
/// `comps`, `has_etc1_bias`, `has_bc1_hint0`, `has_bc1_hint1`, `has_alpha`,
/// `is_la`, `cem`, `total_hint_bits`.
pub fn uastc_mode_table(name: &str) -> &'static [u8] {
    let c = std::ffi::CString::new(name).unwrap();
    let mut len = 0u32;
    let ptr = unsafe { orc_uastc_mode_table(c.as_ptr(), &mut len) };
    assert!(!ptr.is_null(), "oracle: unknown uastc mode table {name:?}");
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Oracle `read_bits1_to_9`: returns (value, new_bit_offset).
pub fn read_bits1_to_9(buf: &[u8], bit_offset: u32, codesize: u32) -> (u32, u32) {
    let mut off = bit_offset;
    let v = unsafe { orc_read_bits1_to_9(buf.as_ptr(), &mut off, codesize) };
    (v, off)
}

/// Oracle `read_bits1_to_9_fst`: returns (value, new_bit_offset).
pub fn read_bits1_to_9_fst(buf: &[u8], bit_offset: u32, codesize: u32) -> (u32, u32) {
    let mut off = bit_offset;
    let v = unsafe { orc_read_bits1_to_9_fst(buf.as_ptr(), &mut off, codesize) };
    (v, off)
}

/// Oracle `read_bits64`: returns (value, new_bit_offset).
pub fn read_bits64(buf: &[u8], bit_offset: u32, codesize: u32) -> (u64, u32) {
    let mut off = bit_offset;
    let v = unsafe { orc_read_bits64(buf.as_ptr(), &mut off, codesize) };
    (v, off)
}

/// Oracle `read_bit`: returns (bit, new_bit_offset).
pub fn read_bit(buf: &[u8], bit_offset: u32) -> (u32, u32) {
    let mut off = bit_offset;
    let v = unsafe { orc_read_bit(buf.as_ptr(), &mut off) };
    (v, off)
}

/// Raw bytes of a named constant table, exactly as the C++ transcoder holds
/// it in memory. Panics on an unknown name. Valid names: the `.inc`
/// "solution" tables `etc1_to_astc`, `etc1_to_astc_0_255`,
/// `etc1_to_bc7_m5_color`, `etc1_to_dxt_5`, `etc1_to_dxt_6`,
/// `etc1s_to_atc_55`, `etc1s_to_atc_56`, `etc1s_to_pvrtc2_45`, plus the
/// grayscale conversion tables `etc1_g_to_dxt5a`, `etc1_g_to_etc2_a8`,
/// `etc1_g_to_etc2_r11` and the BC7 mode-5 match table `bc7_m5_equals_1`.
pub fn const_table(name: &str) -> &'static [u8] {
    let c = std::ffi::CString::new(name).unwrap();
    let mut len = 0u32;
    let ptr = unsafe { orc_const_table(c.as_ptr(), &mut len) };
    assert!(!ptr.is_null(), "oracle: unknown const table {name:?}");
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Raw bytes of the `.inc` constant table `g_etc1_g_to_bc7_m5a`, exactly as the
/// C++ transcoder holds it in memory (1536 entries x 3 bytes).
pub fn etc1_g_to_bc7_m5a() -> &'static [u8] {
    let mut len = 0u32;
    let ptr = unsafe { orc_etc1_g_to_bc7_m5a(&mut len) };
    assert!(!ptr.is_null());
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Run the C++ transcoder's one-time table init. Idempotent; call before
/// reading any init-built table. (The transcode entry points init themselves,
/// so this is only needed for direct table access.)
pub fn init() {
    // Serialize the one-time C++ `basisu_transcoder_init` (its
    // `g_transcoder_initialized` guard is not atomic): a concurrent first-init
    // from two test threads can corrupt the lazily-built tables and SIGSEGV.
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe { orc_init() });
}

/// Which BC1 single-color match table to fetch from the oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bc1MatchTable {
    /// `g_bc1_match5_equals_1`
    Match5Equals1 = 0,
    /// `g_bc1_match6_equals_1`
    Match6Equals1 = 1,
    /// `g_bc1_match5_equals_0`
    Match5Equals0 = 2,
    /// `g_bc1_match6_equals_0`
    Match6Equals0 = 3,
}

/// Raw bytes of an init-built BC1 single-color match table, exactly as the C++
/// transcoder holds it in memory (256 entries x 2 bytes: `m_hi`, `m_lo`).
/// Calls [`init`] first so the table is populated.
pub fn bc1_match_table(which: Bc1MatchTable) -> &'static [u8] {
    init();
    let mut len = 0u32;
    let ptr = unsafe { orc_bc1_match_table(which as c_int, &mut len) };
    assert!(!ptr.is_null(), "oracle returned null for {which:?}");
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Header info reported when a KTX2 Basis payload is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ktx2Info {
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub layers: u32,
    pub faces: u32,
    pub has_alpha: bool,
    pub is_uastc: bool,
    pub is_video: bool,
}

/// One transcoded level: output bytes + the level's logical dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A KTX2 Basis transcoder handle from the C++ oracle. Owns the C++ instance.
pub struct OracleKtx2 {
    handle: *mut c_void,
    pub info: Ktx2Info,
}

impl OracleKtx2 {
    /// Parse + prepare a `.ktx2` Basis payload. `None` if the oracle rejects it.
    pub fn open(bytes: &[u8]) -> Option<Self> {
        init(); // serialize the one-time table init before any parallel oracle use
        let (mut w, mut h, mut levels, mut has_alpha, mut is_uastc) = (0, 0, 0, 0, 0);
        let handle = unsafe {
            orc_ktx2_open(
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut w,
                &mut h,
                &mut levels,
                &mut has_alpha,
                &mut is_uastc,
            )
        };
        if handle.is_null() {
            return None;
        }
        let layers = unsafe { orc_ktx2_layers(handle) };
        let faces = unsafe { orc_ktx2_faces(handle) };
        let is_video = unsafe { orc_ktx2_is_video(handle) != 0 };
        Some(Self {
            handle,
            info: Ktx2Info {
                width: w,
                height: h,
                levels,
                layers,
                faces,
                has_alpha: has_alpha != 0,
                is_uastc: is_uastc != 0,
                is_video,
            },
        })
    }

    /// Bytes that transcoding image `(level, layer, face)` to `fmt` will
    /// produce, and the level's logical dimensions. `None` on failure.
    pub fn level_size(
        &self,
        level: u32,
        layer: u32,
        face: u32,
        fmt: i32,
    ) -> Option<(u32, u32, u32)> {
        let (mut w, mut h) = (0u32, 0u32);
        let n =
            unsafe { orc_ktx2_level_size(self.handle, level, layer, face, fmt, &mut w, &mut h) };
        (n != 0).then_some((n, w, h))
    }

    /// Transcode image `(level, layer, face)` to `fmt` with no decode flags.
    pub fn transcode(&self, level: u32, fmt: i32) -> Option<Level> {
        self.transcode_image_flags(level, 0, 0, fmt, 0)
    }

    /// Transcode `(level, 0, 0)` to `fmt` with `flags`.
    pub fn transcode_flags(&self, level: u32, fmt: i32, flags: u32) -> Option<Level> {
        self.transcode_image_flags(level, 0, 0, fmt, flags)
    }

    /// Transcode image `(level, layer, face)` to `fmt` with `flags`
    /// (`basisd_decode_flags` bits).
    pub fn transcode_image_flags(
        &self,
        level: u32,
        layer: u32,
        face: u32,
        fmt: i32,
        flags: u32,
    ) -> Option<Level> {
        let (bytes, w, h) = self.level_size(level, layer, face, fmt)?;
        let mut data = vec![0u8; bytes as usize];
        let wrote = unsafe {
            orc_ktx2_transcode(
                self.handle,
                level,
                layer,
                face,
                fmt,
                flags,
                data.as_mut_ptr(),
                bytes,
            )
        };
        (wrote == bytes).then_some(Level {
            data,
            width: w,
            height: h,
        })
    }
}

impl Drop for OracleKtx2 {
    /// Closes the C++ KTX2 oracle handle.
    fn drop(&mut self) {
        unsafe { orc_ktx2_close(self.handle) }
    }
}

/// Header info reported when a `.basis` Basis payload is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasisInfo {
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub total_images: u32,
    pub has_alpha: bool,
    /// Raw `basis_tex_format` byte (0 ETC1S, 1 UASTC 4x4, 2+ HDR/XUASTC/ASTC).
    pub tex_format: u32,
    /// Raw `basis_texture_type` byte (3 == video frames).
    pub tex_type: u32,
}

/// A `.basis` Basis transcoder handle from the C++ oracle (the upstream public
/// `basisu_transcoder`). Owns the C++ instance and a copy of the file data.
pub struct OracleBasis {
    handle: *mut c_void,
    pub info: BasisInfo,
}

impl OracleBasis {
    /// Parse + prepare a `.basis` Basis payload (decoding its codebooks via
    /// `start_transcoding`). `None` if the oracle rejects it.
    pub fn open(bytes: &[u8]) -> Option<Self> {
        init(); // serialize the one-time table init before any parallel oracle use
        let (mut w, mut h, mut levels, mut has_alpha) = (0, 0, 0, 0);
        let (mut tex_format, mut tex_type, mut total_images) = (0, 0, 0);
        let handle = unsafe {
            orc_basis_open(
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut w,
                &mut h,
                &mut levels,
                &mut has_alpha,
                &mut tex_format,
                &mut tex_type,
                &mut total_images,
            )
        };
        if handle.is_null() {
            return None;
        }
        Some(Self {
            handle,
            info: BasisInfo {
                width: w,
                height: h,
                levels,
                total_images,
                has_alpha: has_alpha != 0,
                tex_format,
                tex_type,
            },
        })
    }

    /// Open a global-codebook `.basis` file, supplying the shared codebook from
    /// a self-contained `.basis` (`set_global_codebooks`). `None` if the oracle
    /// rejects either file. The codebook file is kept alive inside the handle.
    pub fn open_with_global_codebook(codebook: &[u8], file: &[u8]) -> Option<Self> {
        init(); // serialize the one-time table init before any parallel oracle use
        let (mut w, mut h, mut levels, mut has_alpha) = (0, 0, 0, 0);
        let (mut tex_format, mut tex_type, mut total_images) = (0, 0, 0);
        let handle = unsafe {
            orc_basis_open_with_global_codebook(
                codebook.as_ptr(),
                codebook.len() as u32,
                file.as_ptr(),
                file.len() as u32,
                &mut w,
                &mut h,
                &mut levels,
                &mut has_alpha,
                &mut tex_format,
                &mut tex_type,
                &mut total_images,
            )
        };
        if handle.is_null() {
            return None;
        }
        Some(Self {
            handle,
            info: BasisInfo {
                width: w,
                height: h,
                levels,
                total_images,
                has_alpha: has_alpha != 0,
                tex_format,
                tex_type,
            },
        })
    }

    /// Bytes that transcoding `(image, level)` to `fmt` will produce, and the
    /// level's logical dimensions. `None` on failure.
    pub fn level_size(&self, image: u32, level: u32, fmt: i32) -> Option<(u32, u32, u32)> {
        let (mut w, mut h) = (0u32, 0u32);
        let n = unsafe { orc_basis_level_size(self.handle, image, level, fmt, &mut w, &mut h) };
        (n != 0).then_some((n, w, h))
    }

    /// Transcode `(image, level)` to `fmt` with `flags` (`basisd_decode_flags`
    /// bits). `None` if the oracle declines the combination.
    pub fn transcode(&self, image: u32, level: u32, fmt: i32, flags: u32) -> Option<Level> {
        let (bytes, w, h) = self.level_size(image, level, fmt)?;
        let mut data = vec![0u8; bytes as usize];
        let wrote = unsafe {
            orc_basis_transcode(
                self.handle,
                image,
                level,
                fmt,
                flags,
                data.as_mut_ptr(),
                bytes,
            )
        };
        (wrote == bytes).then_some(Level {
            data,
            width: w,
            height: h,
        })
    }
}

impl Drop for OracleBasis {
    /// Closes the C++ `.basis` oracle handle.
    fn drop(&mut self) {
        unsafe { orc_basis_close(self.handle) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGBA32: i32 = 13;

    /// Read a fixture from the committed `corpus/smoke` tree.
    fn smoke(rel: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../corpus/smoke/").to_string() + rel;
        std::fs::read(&path).unwrap_or_else(|e| panic!("read smoke fixture {path}: {e}"))
    }

    /// The oracle compiles, links, and transcodes a real ETC1S fixture to
    /// uncompressed RGBA8 via the upstream public ktx2_transcoder.
    #[test]
    fn oracle_transcodes_etc1s_to_rgba32() {
        let bytes = smoke("etc1s.ktx2");
        let k = OracleKtx2::open(&bytes).expect("oracle should open the ETC1S fixture");
        assert!(!k.info.is_uastc, "etc1s.ktx2 should not be UASTC");
        assert!(k.info.levels >= 1 && k.info.width > 0 && k.info.height > 0);

        let level = k.transcode(0, RGBA32).expect("transcode level 0 to RGBA32");
        assert_eq!(level.width, k.info.width);
        assert_eq!(level.height, k.info.height);
        assert_eq!(level.data.len(), (level.width * level.height * 4) as usize);
    }

    /// The oracle also transcodes the UASTC fixture (the other source codec).
    #[test]
    fn oracle_transcodes_uastc_to_rgba32() {
        let bytes = smoke("uastc.ktx2");
        let k = OracleKtx2::open(&bytes).expect("oracle should open the UASTC fixture");
        assert!(k.info.is_uastc, "uastc.ktx2 should be UASTC");
        let level = k.transcode(0, RGBA32).expect("transcode level 0 to RGBA32");
        assert_eq!(level.data.len(), (level.width * level.height * 4) as usize);
    }
}
