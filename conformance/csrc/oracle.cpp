// Oracle wrapper for differential testing.
//
// The vendored transcoder .cpp is #include'd directly into this translation
// unit. That pulls its file-static (internal-linkage) tables and helpers into
// scope here, so the oracle can hand the Rust port the exact bytes of any
// internal table to diff against. The vendored source itself is never
// modified; it is only compiled from inside our own TU, which is also why
// build.rs must not compile basisu_transcoder.cpp a second time.

#include "basisu_transcoder/basisu_transcoder.cpp"

#include <cstdint>
#include <cstring>
#include <memory>
#include <vector>

using namespace basist;

extern "C" {

// Run the one-time table init (idempotent). Must be called before reading any
// init-built table below.
void orc_init(void) { basisu_transcoder_init(); }

// BC1 single-color match tables (init-built by prepare_bc1_single_color_table).
// which: 0 = match5_equals_1, 1 = match6_equals_1, 2 = match5_equals_0,
// 3 = match6_equals_0. Returns a pointer to the raw table bytes; *out_len is
// the byte length (256 entries * sizeof(bc1_match_entry)). nullptr on bad index.
const uint8_t *orc_bc1_match_table(int which, uint32_t *out_len) {
    const bc1_match_entry *p = nullptr;
    switch (which) {
        case 0: p = g_bc1_match5_equals_1; break;
        case 1: p = g_bc1_match6_equals_1; break;
        case 2: p = g_bc1_match5_equals_0; break;
        case 3: p = g_bc1_match6_equals_0; break;
        default: *out_len = 0; return nullptr;
    }
    *out_len = static_cast<uint32_t>(sizeof(bc1_match_entry) * 256);
    return reinterpret_cast<const uint8_t *>(p);
}

// The `.inc` constant table g_etc1_g_to_bc7_m5a (ETC1->BC7 mode 5 alpha
// conversion). Raw bytes in C++ in-memory layout; *out_len is sizeof the table.
const uint8_t *orc_etc1_g_to_bc7_m5a(uint32_t *out_len) {
    *out_len = static_cast<uint32_t>(sizeof(g_etc1_g_to_bc7_m5a));
    return reinterpret_cast<const uint8_t *>(g_etc1_g_to_bc7_m5a);
}

// Generic accessor for constant tables. The first group is the `.inc`
// "solution" tables (all share the 4-byte {m_lo, m_hi, m_err} layout); the
// second group is the remaining ETC1S grayscale conversion tables plus the
// BC7 mode-5 match table, each with its own padding-free element layout.
// Returns the table's raw bytes by name; nullptr + *out_len = 0 for an
// unknown name.
const uint8_t *orc_const_table(const char *name, uint32_t *out_len) {
#define ORC_T(n, sym)                                                          \
    if (std::strcmp(name, n) == 0) {                                           \
        *out_len = static_cast<uint32_t>(sizeof(sym));                         \
        return reinterpret_cast<const uint8_t *>(sym);                         \
    }
    ORC_T("etc1_to_astc", g_etc1_to_astc)
    ORC_T("etc1_to_astc_0_255", g_etc1_to_astc_0_255)
    ORC_T("etc1_to_bc7_m5_color", g_etc1_to_bc7_m5_color)
    ORC_T("etc1_to_dxt_5", g_etc1_to_dxt_5)
    ORC_T("etc1_to_dxt_6", g_etc1_to_dxt_6)
    ORC_T("etc1s_to_atc_55", g_etc1s_to_atc_55)
    ORC_T("etc1s_to_atc_56", g_etc1s_to_atc_56)
    ORC_T("etc1s_to_pvrtc2_45", g_etc1s_to_pvrtc2_45)
    // g_etc1_g_to_dxt5a: etc1_g_to_dxt5a_conversion {u8 m_lo, u8 m_hi,
    // u16 m_trans}, [32*8][4] flattened.
    ORC_T("etc1_g_to_dxt5a", g_etc1_g_to_dxt5a)
    // s_etc1_g_to_etc2_a8 / s_etc1_g_to_etc2_r11: etc1_g_to_eac_conversion
    // {u8 m_base, u8 m_table_mul, u16 m_trans}, [32*8][4].
    ORC_T("etc1_g_to_etc2_a8", s_etc1_g_to_etc2_a8)
    ORC_T("etc1_g_to_etc2_r11", s_etc1_g_to_etc2_r11)
    // g_bc7_m5_equals_1: bc7_m5_match_entry {u8 m_hi, u8 m_lo}, [256].
    ORC_T("bc7_m5_equals_1", g_bc7_m5_equals_1)
#undef ORC_T
    *out_len = 0;
    return nullptr;
}

// Unpack one UASTC block (16 bytes) to 16 RGBA pixels (out64). Returns 1 on
// success, 0 if the block's mode is invalid. Matches the convenience wrapper
// (no hints, no blue-contract).
uint32_t orc_uastc_unpack_pixels(const uint8_t *src16, uint32_t srgb, uint8_t *out64) {
    basisu_transcoder_init(); // idempotent; ensures uastc_init() ran
    uastc_block blk;
    std::memcpy(blk.m_bytes, src16, 16);
    color32 pixels[16];
    if (!unpack_uastc(blk, pixels, srgb != 0))
        return 0;
    std::memcpy(out64, pixels, 64);
    return 1;
}

// Unpack one physical ASTC block (16 bytes) of the given footprint and decode
// it in one of astc_helpers' decode modes (0=SRGB8, 1=LDR8, 2=HDR16,
// 3=RGB9E5). Returns 0 if the unpack rejects the block, 1 if the decode
// rejects its config, 2 on success. `out` receives bw*bh texels: 4 bytes each
// for SRGB8/LDR8/RGB9E5, 8 bytes (four halves) for HDR16. Note decode_block
// writes error texels into `out` before returning false, so `out` is only
// meaningful when this returns 2.
uint32_t orc_astc_unpack_and_decode(const uint8_t *src16, uint32_t bw, uint32_t bh,
                                    uint32_t mode, uint8_t *out) {
    basisu_transcoder_init(); // idempotent; runs astc_helpers::init_tables()
    astc_helpers::log_astc_block log_blk;
    if (!astc_helpers::unpack_block(src16, log_blk, bw, bh))
        return 0;
    return astc_helpers::decode_block(log_blk, out, bw, bh, (astc_helpers::decode_mode)mode)
               ? 2u
               : 1u;
}

// Decompress an XUASTC LDR stream through xuastc_decoded_image::decode,
// packing every logical block to a physical ASTC block (the transcoder's
// ASTC pass-through arm does exactly this). Writes ceil(w/bw)*ceil(h/bh)
// 16-byte blocks row-major into out_blocks (capacity out_blocks_cap bytes)
// and the header fields into the out params. Returns 1 on success, 0 when
// the reference rejects the stream, a block fails to pack, or the decoded
// blocks would not fit.
struct OrcXuastcState {
    uint8_t *out;
    uint32_t cap;
    uint32_t num_blocks_x, num_blocks_y;
};
uint32_t orc_xuastc_decompress(const uint8_t *comp, uint32_t comp_size,
                               uint8_t *out_blocks, uint32_t out_blocks_cap,
                               uint32_t *out_block_w, uint32_t *out_block_h,
                               uint32_t *out_w, uint32_t *out_h,
                               uint32_t *out_has_alpha, uint32_t *out_srgb) {
    basisu_transcoder_init();
    OrcXuastcState st{out_blocks, out_blocks_cap, 0, 0};

    auto init_fn = [](uint32_t nbx, uint32_t nby, uint32_t bw, uint32_t bh,
                      bool srgb, float dct_q, bool has_alpha,
                      void *user) -> bool {
        (void)bw; (void)bh; (void)srgb; (void)dct_q; (void)has_alpha;
        OrcXuastcState &s = *(OrcXuastcState *)user;
        if ((uint64_t)nbx * nby * 16 > s.cap)
            return false;
        s.num_blocks_x = nbx;
        s.num_blocks_y = nby;
        return true;
    };
    auto block_fn = [](uint32_t bx, uint32_t by,
                       const astc_helpers::log_astc_block &log_blk,
                       void *user) -> bool {
        OrcXuastcState &s = *(OrcXuastcState *)user;
        astc_helpers::astc_block phys;
        if (!astc_helpers::pack_astc_block(phys, log_blk))
            return false;
        memcpy(s.out + (by * s.num_blocks_x + bx) * 16, &phys, 16);
        return true;
    };

    xuastc_decoded_image img;
    if (!img.decode(comp, comp_size, init_fn, &st, block_fn, &st))
        return 0;
    *out_block_w = img.m_actual_block_width;
    *out_block_h = img.m_actual_block_height;
    *out_w = img.m_actual_width;
    *out_h = img.m_actual_height;
    *out_has_alpha = img.m_actual_has_alpha ? 1 : 0;
    *out_srgb = img.m_uses_srgb_astc_decode_mode ? 1 : 0;
    return 1;
}

// The XUASTC weight-grid 2D inverse DCT (astc_ldr_t::idct_2d): rows*cols
// f32 coefficients in, rows*cols f32 spatial values out. Sizes 2..=12 each.
void orc_xuastc_idct_2d(const float *src, uint32_t num_rows, uint32_t num_cols,
                        float *dst) {
    astc_ldr_t::idct_2d(src, dst, num_rows, num_cols);
}

// Decompress a UASTC HDR 6x6 intermediate stream through
// astc_6x6_hdr::decode_6x6_hdr. Writes the decoded physical ASTC blocks
// (ceil(w/6)*ceil(h/6) 16-byte blocks, row-major) into out_blocks (capacity
// out_blocks_cap bytes) and the coded dimensions into out_w/out_h. Returns 1
// on success, 0 when the reference decoder rejects the stream or the decoded
// blocks would not fit.
uint32_t orc_decode_6x6_hdr(const uint8_t *comp, uint32_t comp_size,
                            uint8_t *out_blocks, uint32_t out_blocks_cap,
                            uint32_t *out_w, uint32_t *out_h) {
    basisu_transcoder_init(); // idempotent; runs astc_helpers::init_tables()
    basisu::vector2D<astc_helpers::astc_block> decoded_blocks;
    uint32_t w = 0, h = 0;
    if (!astc_6x6_hdr::decode_6x6_hdr(comp, comp_size, decoded_blocks, w, h))
        return 0;
    *out_w = w;
    *out_h = h;
    const uint32_t total_bytes =
        decoded_blocks.get_width() * decoded_blocks.get_height() * 16;
    if (total_bytes > out_blocks_cap)
        return 0;
    memcpy(out_blocks, decoded_blocks.get_ptr(), total_bytes);
    return 1;
}

// Encode one 4x4 block of RGB half-floats (48 halves = 96 bytes, raster
// order) as BC6H unsigned through astc_6x6_hdr::fast_encode_bc6h. `hq` sets
// m_max_2subset_pats_to_try = 1 (the HIGH_QUALITY transcode path); 0 is the
// default path. All other params keep their defaults.
void orc_fast_encode_bc6h(const uint8_t *halves96, uint32_t hq, uint8_t *out16) {
    basisu_transcoder_init(); // idempotent; runs fast_encode_bc6h_init()
    astc_6x6_hdr::fast_bc6h_params params;
    params.m_max_2subset_pats_to_try = hq ? 1 : 0;
    astc_6x6_hdr::fast_encode_bc6h((const basist::half_float *)halves96,
                                   (basist::bc6h_block *)out16, params);
}

// Pack one 4x4 RGBA pixel block (64 bytes, r,g,b,a order) as BC7 through
// bc7f's auto-RGBA entry -- the only entry the raw-ASTC decode paths reach
// (their has_alpha is always "unknown"). `flags` is the raw cPackBC7Flag
// bitmask (the transcoder passes cPackBC7FlagDefault or
// cPackBC7FlagDefaultPartiallyAnalytical).
void orc_bc7f_pack_rgba(const uint8_t *pixels64, uint32_t flags, uint8_t *out16) {
    basisu_transcoder_init(); // idempotent; runs bc7f::init()
    bc7f::fast_pack_bc7_auto_rgba(out16, (const color_rgba *)pixels64, flags);
}

// Transcode one UASTC block (16 bytes) to one block of the named target
// format. Returns 1 on success, 0 on an invalid source block. The BC7, ETC1
// and ETC2_RGBA hooks below share this contract; only the output size varies.
uint32_t orc_transcode_uastc_to_astc(const uint8_t *src16, uint8_t *out16) {
    basisu_transcoder_init();
    uastc_block blk;
    std::memcpy(blk.m_bytes, src16, 16);
    return transcode_uastc_to_astc(blk, out16) ? 1u : 0u;
}

uint32_t orc_transcode_uastc_to_bc7(const uint8_t *src16, uint8_t *out16) {
    basisu_transcoder_init();
    uastc_block blk;
    std::memcpy(blk.m_bytes, src16, 16);
    return transcode_uastc_to_bc7(blk, out16) ? 1u : 0u;
}

uint32_t orc_transcode_uastc_to_etc1(const uint8_t *src16, uint8_t *out8) {
    basisu_transcoder_init();
    uastc_block blk;
    std::memcpy(blk.m_bytes, src16, 16);
    return transcode_uastc_to_etc1(blk, out8) ? 1u : 0u;
}

uint32_t orc_transcode_uastc_to_etc2_rgba(const uint8_t *src16, uint8_t *out16) {
    basisu_transcoder_init();
    uastc_block blk;
    std::memcpy(blk.m_bytes, src16, 16);
    return transcode_uastc_to_etc2_rgba(blk, out16) ? 1u : 0u;
}

// ASTC BISE unquantization (built tables require orc_init first).
uint32_t orc_astc_get_levels(uint32_t range) { return (uint32_t)astc_get_levels((int)range); }
uint32_t orc_astc_is_valid_endpoint_range(uint32_t range) {
    return astc_is_valid_endpoint_range(range) ? 1u : 0u;
}
uint32_t orc_unquant_astc_endpoint_val(uint32_t packed_val, uint32_t range) {
    return unquant_astc_endpoint_val(packed_val, range);
}
uint32_t orc_unquant_astc_endpoint(uint32_t bits, uint32_t trits, uint32_t quints, uint32_t range) {
    return unquant_astc_endpoint(bits, trits, quints, range);
}
// The init-built g_astc_unquant[21][256] table (astc_quant_bin = 2 bytes).
const uint8_t *orc_astc_unquant_table(uint32_t *out_len) {
    *out_len = static_cast<uint32_t>(sizeof(g_astc_unquant));
    return reinterpret_cast<const uint8_t *>(g_astc_unquant);
}

// The init-built BC7 mode-5/6 optimal endpoint tables (endpoint_err = 4 bytes).
const uint8_t *orc_bc7_mode6_optimal(uint32_t *out_len) {
    *out_len = static_cast<uint32_t>(sizeof(g_bc7_mode_6_optimal_endpoints));
    return reinterpret_cast<const uint8_t *>(g_bc7_mode_6_optimal_endpoints);
}
const uint8_t *orc_bc7_mode5_optimal(uint32_t *out_len) {
    *out_len = static_cast<uint32_t>(sizeof(g_bc7_mode_5_optimal_endpoints));
    return reinterpret_cast<const uint8_t *>(g_bc7_mode_5_optimal_endpoints);
}

// UASTC block bit readers. Each takes the bit cursor by pointer (in/out) and
// returns the field; mirrors the C++ `uint32_t& bit_offset` signature.
uint32_t orc_read_bits1_to_9(const uint8_t *buf, uint32_t *bit_offset, uint32_t codesize) {
    return read_bits1_to_9(buf, *bit_offset, codesize);
}
uint64_t orc_read_bits64(const uint8_t *buf, uint32_t *bit_offset, uint32_t codesize) {
    return read_bits64(buf, *bit_offset, codesize);
}
uint32_t orc_read_bits1_to_9_fst(const uint8_t *buf, uint32_t *bit_offset, uint32_t codesize) {
    return read_bits1_to_9_fst(buf, *bit_offset, codesize);
}
uint32_t orc_read_bit(const uint8_t *buf, uint32_t *bit_offset) { return read_bit(buf, *bit_offset); }

// UASTC per-mode property tables (19 bytes each). Returns a pointer to the
// named table; nullptr for an unknown name.
const uint8_t *orc_uastc_mode_table(const char *name, uint32_t *out_len) {
    *out_len = TOTAL_UASTC_MODES;
#define ORC_M(n, sym)                                                          \
    if (std::strcmp(name, n) == 0)                                             \
        return reinterpret_cast<const uint8_t *>(sym);
    ORC_M("weight_bits", g_uastc_mode_weight_bits)
    ORC_M("weight_ranges", g_uastc_mode_weight_ranges)
    ORC_M("endpoint_ranges", g_uastc_mode_endpoint_ranges)
    ORC_M("subsets", g_uastc_mode_subsets)
    ORC_M("planes", g_uastc_mode_planes)
    ORC_M("comps", g_uastc_mode_comps)
    ORC_M("has_etc1_bias", g_uastc_mode_has_etc1_bias)
    ORC_M("has_bc1_hint0", g_uastc_mode_has_bc1_hint0)
    ORC_M("has_bc1_hint1", g_uastc_mode_has_bc1_hint1)
    ORC_M("has_alpha", g_uastc_mode_has_alpha)
    ORC_M("is_la", g_uastc_mode_is_la)
    ORC_M("cem", g_uastc_mode_cem)
    ORC_M("total_hint_bits", g_uastc_mode_total_hint_bits)
#undef ORC_M
    *out_len = 0;
    return nullptr;
}

// Partition descriptor tables, exposed field-wise (the C++ structs carry
// padding, so raw bytes are not comparable). The caller passes column buffers
// sized to the table's row count.
void orc_partitions2(uint8_t *bc7, uint16_t *astc, uint8_t *invert) {
    for (uint32_t i = 0; i < TOTAL_ASTC_BC7_COMMON_PARTITIONS2; i++) {
        bc7[i] = g_astc_bc7_common_partitions2[i].m_bc7;
        astc[i] = g_astc_bc7_common_partitions2[i].m_astc;
        invert[i] = g_astc_bc7_common_partitions2[i].m_invert ? 1u : 0u;
    }
}
void orc_partitions3(uint8_t *bc7, uint16_t *astc, uint8_t *perm) {
    for (uint32_t i = 0; i < TOTAL_ASTC_BC7_COMMON_PARTITIONS3; i++) {
        bc7[i] = g_astc_bc7_common_partitions3[i].m_bc7;
        astc[i] = g_astc_bc7_common_partitions3[i].m_astc;
        perm[i] = g_astc_bc7_common_partitions3[i].m_astc_to_bc7_perm;
    }
}
void orc_bc7_3_astc2_partitions(uint8_t *bc73, uint16_t *astc2, uint8_t *k) {
    for (uint32_t i = 0; i < TOTAL_BC7_3_ASTC2_COMMON_PARTITIONS; i++) {
        bc73[i] = g_bc7_3_astc2_common_partitions[i].m_bc73;
        astc2[i] = g_bc7_3_astc2_common_partitions[i].m_astc2;
        k[i] = g_bc7_3_astc2_common_partitions[i].k;
    }
}

// Other flat UASTC tables (no padding): raw bytes by name + sizeof.
const uint8_t *orc_uastc_flat_table(const char *name, uint32_t *out_len) {
#define ORC_F(n, sym)                                                          \
    if (std::strcmp(name, n) == 0) {                                           \
        *out_len = static_cast<uint32_t>(sizeof(sym));                         \
        return reinterpret_cast<const uint8_t *>(sym);                         \
    }
    ORC_F("mode_huff_codes", g_uastc_mode_huff_codes)
    ORC_F("astc_to_bc7_perm", g_astc_to_bc7_partition_index_perm_tables)
    ORC_F("bc7_to_astc_perm", g_bc7_to_astc_partition_index_perm_tables)
    ORC_F("astc_bise_range_table", g_astc_bise_range_table)
    ORC_F("patterns2", g_astc_bc7_patterns2)
    ORC_F("patterns3", g_astc_bc7_patterns3)
    ORC_F("bc7_3_astc2_patterns2", g_bc7_3_astc2_patterns2)
    ORC_F("pattern2_anchors", g_astc_bc7_pattern2_anchors)
    ORC_F("pattern3_anchors", g_astc_bc7_pattern3_anchors)
    ORC_F("bc7_3_astc2_patterns2_anchors", g_bc7_3_astc2_patterns2_anchors)
    ORC_F("huff_modes", g_uastc_huff_modes)
    ORC_F("etc1_inten_tables", g_etc1_inten_tables)
    ORC_F("eac_modifier_table", g_eac_modifier_table)
    ORC_F("etc1_pixel_coords", g_etc1_pixel_coords)
    ORC_F("etc1_solid_selectors", s_etc1_solid_selectors)
    ORC_F("etc2_eac_a8_sel4", g_etc2_eac_a8_sel4)
    ORC_F("selector_index_to_etc1", g_selector_index_to_etc1)
    ORC_F("etc_5_to_8", g_etc_5_to_8)
    ORC_F("astc_trit_encode", g_astc_trit_encode)
    ORC_F("astc_quint_encode", g_astc_quint_encode)
    ORC_F("bc7_partition1", g_bc7_partition1)
    ORC_F("bc7_partition2", g_bc7_partition2)
    ORC_F("bc7_partition3", g_bc7_partition3)
    ORC_F("bc7_anchor_second", g_bc7_table_anchor_index_second_subset)
    ORC_F("bc7_anchor_third_1", g_bc7_table_anchor_index_third_subset_1)
    ORC_F("bc7_anchor_third_2", g_bc7_table_anchor_index_third_subset_2)
    ORC_F("bc7_num_subsets", g_bc7_num_subsets)
    ORC_F("bc7_partition_bits", g_bc7_partition_bits)
    ORC_F("bc7_color_index_bitcount", g_bc7_color_index_bitcount)
    ORC_F("bc7_alpha_index_bitcount", g_bc7_alpha_index_bitcount)
    ORC_F("bc7_mode_has_p_bits", g_bc7_mode_has_p_bits)
    ORC_F("bc7_mode_has_shared_p_bits", g_bc7_mode_has_shared_p_bits)
    ORC_F("bc7_color_precision", g_bc7_color_precision_table)
    ORC_F("bc7_alpha_precision", g_bc7_alpha_precision_table)
#undef ORC_F
    *out_len = 0;
    return nullptr;
}

// BasisLZ Huffman table builder. Builds into a persistent table and exposes its
// lookup/tree/code_sizes for differential comparison.
static huffman_decoding_table g_orc_huff;

uint32_t orc_huffman_init(const uint8_t *code_sizes, uint32_t n) {
    g_orc_huff.clear();
    return g_orc_huff.init(n, code_sizes) ? 1u : 0u;
}
const int32_t *orc_huffman_lookup(uint32_t *out_len) {
    const auto &v = g_orc_huff.get_lookup();
    *out_len = static_cast<uint32_t>(v.size());
    return v.data();
}
const int16_t *orc_huffman_tree(uint32_t *out_len) {
    const auto &v = g_orc_huff.get_tree();
    *out_len = static_cast<uint32_t>(v.size());
    return v.data();
}
const uint8_t *orc_huffman_code_sizes(uint32_t *out_len) {
    const auto &v = g_orc_huff.get_code_sizes();
    *out_len = static_cast<uint32_t>(v.size());
    return v.data();
}

// Parse a real ETC1S KTX2's BasisLZ global data and decode its codebooks, so
// the Rust decode_palettes/decode_tables can be diffed on identical raw inputs.
struct OrcEtc1s {
    std::vector<uint8_t> file;
    basisu_lowlevel_etc1s_transcoder t;
    uint32_t num_e, num_s;
    uint32_t e_ofs, e_len, s_ofs, s_len, t_ofs, t_len;
    uint32_t width, height, num_bx, num_by, slice_ofs, slice_len;
    uint32_t alpha_ofs, alpha_len;
    uint32_t ok;
};
// Little-endian scalar reads out of the raw file buffer.
static uint32_t orc_rd32(const std::vector<uint8_t> &b, size_t o) {
    return b[o] | (b[o + 1] << 8) | (b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
}
static uint64_t orc_rd64(const std::vector<uint8_t> &b, size_t o) {
    uint64_t v = 0;
    for (int i = 0; i < 8; i++)
        v |= (uint64_t)b[o + i] << (i * 8);
    return v;
}
void *orc_etc1s_open(const uint8_t *buf, uint32_t n) {
    OrcEtc1s *o = new OrcEtc1s();
    o->file.assign(buf, buf + n);
    o->ok = 0;
    if (n < 80)
        return o;
    uint64_t sgd_ofs = orc_rd64(o->file, 64), sgd_len = orc_rd64(o->file, 72);
    if (sgd_ofs + sgd_len > n || sgd_len < 20)
        return o;
    size_t s = (size_t)sgd_ofs;
    o->num_e = o->file[s] | (o->file[s + 1] << 8);
    o->num_s = o->file[s + 2] | (o->file[s + 3] << 8);
    o->e_len = orc_rd32(o->file, s + 4);
    o->s_len = orc_rd32(o->file, s + 8);
    o->t_len = orc_rd32(o->file, s + 12);
    // SGD layout: [header(20)][image_descs(20 * image_count)][endpoints][selectors][tables].
    uint32_t layer_count = orc_rd32(o->file, 32);
    uint32_t face_count = orc_rd32(o->file, 36);
    uint32_t level_count = orc_rd32(o->file, 40);
    uint32_t image_count = (layer_count ? layer_count : 1) * face_count * level_count;
    o->e_ofs = (uint32_t)s + 20 + 20 * image_count;
    o->s_ofs = o->e_ofs + o->e_len;
    o->t_ofs = o->s_ofs + o->s_len;
    if ((uint64_t)o->t_ofs + o->t_len > n)
        return o;
    // Level-0 image-0 slice (etc1s_image_index 0): slice = level0.byteOffset +
    // image_desc[0].rgb_slice_byte_offset. Level index starts at file offset 80.
    o->width = orc_rd32(o->file, 20);
    o->height = orc_rd32(o->file, 24);
    o->num_bx = (o->width + 3) / 4;
    o->num_by = (o->height + 3) / 4;
    uint64_t level0_ofs = orc_rd64(o->file, 80);
    uint32_t desc0 = (uint32_t)s + 20; // first image desc
    uint32_t rgb_slice_ofs = orc_rd32(o->file, desc0 + 4);
    o->slice_len = orc_rd32(o->file, desc0 + 8);
    o->slice_ofs = (uint32_t)level0_ofs + rgb_slice_ofs;
    if ((uint64_t)o->slice_ofs + o->slice_len > n)
        return o;
    // Alpha slice (image_desc.alpha_slice_byte_offset@+12 / length@+16).
    uint32_t alpha_slice_ofs = orc_rd32(o->file, desc0 + 12);
    o->alpha_len = orc_rd32(o->file, desc0 + 16);
    o->alpha_ofs = o->alpha_len ? (uint32_t)level0_ofs + alpha_slice_ofs : 0;
    if ((uint64_t)o->alpha_ofs + o->alpha_len > n)
        return o;

    basisu_transcoder_init();
    o->ok = o->t.decode_palettes(o->num_e, o->file.data() + o->e_ofs, o->e_len, o->num_s,
                                 o->file.data() + o->s_ofs, o->s_len) &&
            o->t.decode_tables(o->file.data() + o->t_ofs, o->t_len);
    return o;
}
// Level-0 geometry: which 0 = blocks across, 1 = blocks down, 2 = width,
// 3 = height.
uint32_t orc_etc1s_dim(void *h, uint32_t which) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    switch (which) {
    case 0: return o->num_bx;
    case 1: return o->num_by;
    case 2: return o->width;
    default: return o->height;
    }
}
// The raw level-0 image-0 RGB slice bytes (the alpha variant is further down).
const uint8_t *orc_etc1s_slice(void *h, uint32_t *len) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    *len = o->slice_len;
    return o->file.data() + o->slice_ofs;
}
// Run the oracle's lowlevel transcode_slice to ETC1 on level-0 image-0.
uint32_t orc_etc1s_transcode_slice_etc1(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    return o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                o->slice_len, block_format::cETC1, 8, false, false, false, 0,
                                o->width, o->height)
               ? 1u
               : 0u;
}
// Run the oracle's lowlevel transcode_slice to RGBA32 (width*height*4 output).
uint32_t orc_etc1s_transcode_slice_rgba32(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    return o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                o->slice_len, block_format::cRGBA32, 4, false, false, false, 0,
                                o->width, o->height)
               ? 1u
               : 0u;
}
// Run the oracle's lowlevel transcode_slice to ASTC 4x4 (opaque RGB; 16 b/block).
uint32_t orc_etc1s_transcode_slice_astc(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    return o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                o->slice_len, block_format::cASTC_LDR_4x4, 16, false, false, false, 0,
                                o->width, o->height)
               ? 1u
               : 0u;
}
// Run the oracle's lowlevel transcode_slice to BC7 mode 5 color (16 b/block).
uint32_t orc_etc1s_transcode_slice_bc7(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    return o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                o->slice_len, block_format::cBC7_M5_COLOR, 16, false, false, false,
                                0, o->width, o->height)
               ? 1u
               : 0u;
}
// Run the oracle's lowlevel transcode_slice to ETC2 EAC-A8 alpha (8 b/block).
uint32_t orc_etc1s_transcode_slice_eac_a8(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    return o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                o->slice_len, block_format::cETC2_EAC_A8, 8, false, false, false, 0,
                                o->width, o->height)
               ? 1u
               : 0u;
}
uint32_t orc_etc1s_has_alpha(void *h) { return static_cast<OrcEtc1s *>(h)->alpha_len ? 1u : 0u; }
const uint8_t *orc_etc1s_alpha_slice(void *h, uint32_t *len) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    *len = o->alpha_len;
    return o->file.data() + o->alpha_ofs;
}
// Combined ETC1S -> BC7 mode 5 RGBA: color pass then alpha pass (in place).
uint32_t orc_etc1s_transcode_bc7_rgba(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    if (!o->alpha_len)
        return 0;
    bool ok = o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                   o->slice_len, block_format::cBC7_M5_COLOR, 16, false, false,
                                   false, 0, o->width, o->height) &&
              o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->alpha_ofs,
                                   o->alpha_len, block_format::cBC7_M5_ALPHA, 16, false, false,
                                   false, 0, o->width, o->height);
    return ok ? 1u : 0u;
}

// Combined ETC1S -> ASTC 4x4 RGBA: write alpha (endpoint,selector) indices into
// each block via cIndices, then the color pass with astc_transcode_alpha=true.
uint32_t orc_etc1s_transcode_astc_rgba(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    if (!o->alpha_len)
        return 0;
    bool ok = o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->alpha_ofs,
                                   o->alpha_len, block_format::cIndices, 16, false, false, false, 0,
                                   o->width, o->height) &&
              o->t.transcode_slice(out, o->num_bx, o->num_by, o->file.data() + o->slice_ofs,
                                   o->slice_len, block_format::cASTC_LDR_4x4, 16, false, false, false, 0,
                                   o->width, o->height, 0, nullptr, true);
    return ok ? 1u : 0u;
}

// Combined ETC1S -> ETC2_RGBA: EAC-A8 alpha (bytes 0-7) + ETC1 color (8-15).
uint32_t orc_etc1s_transcode_etc2_rgba(void *h, uint8_t *out) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    if (!o->alpha_len)
        return 0;
    uint32_t nb = o->num_bx * o->num_by;
    std::vector<uint8_t> color(nb * 8), alpha(nb * 8);
    bool ok = o->t.transcode_slice(color.data(), o->num_bx, o->num_by,
                                   o->file.data() + o->slice_ofs, o->slice_len, block_format::cETC1,
                                   8, false, false, false, 0, o->width, o->height) &&
              o->t.transcode_slice(alpha.data(), o->num_bx, o->num_by,
                                   o->file.data() + o->alpha_ofs, o->alpha_len,
                                   block_format::cETC2_EAC_A8, 8, false, false, false, 0, o->width,
                                   o->height);
    if (!ok)
        return 0;
    for (uint32_t b = 0; b < nb; b++) {
        std::memcpy(out + b * 16, alpha.data() + b * 8, 8);
        std::memcpy(out + b * 16 + 8, color.data() + b * 8, 8);
    }
    return 1;
}
void orc_etc1s_free(void *h) { delete static_cast<OrcEtc1s *>(h); }
// 1 if orc_etc1s_open fully parsed the file and decoded both codebooks.
uint32_t orc_etc1s_ok(void *h) { return static_cast<OrcEtc1s *>(h)->ok; }
// Codebook sizes: which 0 = endpoint count, otherwise selector count.
uint32_t orc_etc1s_counts(void *h, uint32_t which) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    return which == 0 ? o->num_e : o->num_s;
}
// Raw compressed SGD ranges: which 0 = endpoints, 1 = selectors, else tables.
const uint8_t *orc_etc1s_data(void *h, uint32_t which, uint32_t *len) {
    OrcEtc1s *o = static_cast<OrcEtc1s *>(h);
    if (which == 0) {
        *len = o->e_len;
        return o->file.data() + o->e_ofs;
    }
    if (which == 1) {
        *len = o->s_len;
        return o->file.data() + o->s_ofs;
    }
    *len = o->t_len;
    return o->file.data() + o->t_ofs;
}
// Decoded codebook entries, serialized field-by-field (the C++ structs carry
// bitfields/padding). Endpoint i as {r5, g5, b5, inten}; selector i as its 4
// packed 2-bit-selector rows followed by the 4 ETC1-format selector bytes.
void orc_etc1s_endpoint(void *h, uint32_t i, uint8_t *out4) {
    const auto &e = static_cast<OrcEtc1s *>(h)->t.get_endpoints()[i];
    out4[0] = e.m_color5.r;
    out4[1] = e.m_color5.g;
    out4[2] = e.m_color5.b;
    out4[3] = e.m_inten5;
}
void orc_etc1s_selector(void *h, uint32_t i, uint8_t *out8) {
    const auto &s = static_cast<OrcEtc1s *>(h)->t.get_selectors()[i];
    std::memcpy(out8, s.m_selectors, 4);
    std::memcpy(out8 + 4, s.m_bytes, 4);
}

// ASTC CEM pack helpers: build astc_block_params from endpoints[10]+weights[32],
// pack to a 16-byte ASTC block. which: 0=cem12_r2, 1=cem12_r0, 2=cem4_r2, 3=cem8_r2.
void orc_astc_pack_cem(uint32_t which, const uint8_t *endpoints, const uint8_t *weights,
                       uint8_t *out16) {
    astc_block_params blk;
    std::memcpy(blk.m_endpoints, endpoints, 10);
    std::memcpy(blk.m_weights, weights, 32);
    uint32_t *o = reinterpret_cast<uint32_t *>(out16);
    switch (which) {
    case 0: astc_pack_block_cem_12_weight_range2(o, &blk); break;
    case 1: astc_pack_block_cem_12_weight_range0(o, &blk); break;
    case 2: astc_pack_block_cem_4_weight_range2(o, &blk); break;
    default: astc_pack_block_cem_8_weight_range2(o, &blk); break;
    }
}

// ETC1S->ASTC init-built tables (built by transcoder_init_astc). orc_init first.
const uint8_t *orc_astc_init_table(const char *name, uint32_t *out_len) {
    basisu_transcoder_init();
#define ORC_AT(n, sym)                                                         \
    if (std::strcmp(name, n) == 0) {                                           \
        *out_len = static_cast<uint32_t>(sizeof(sym));                         \
        return reinterpret_cast<const uint8_t *>(sym);                         \
    }
    ORC_AT("ise_to_unquant", g_ise_to_unquant)
    ORC_AT("selector_range_index", g_etc1_to_astc_selector_range_index)
    ORC_AT("best_grayscale_mapping", g_etc1_to_astc_best_grayscale_mapping)
    ORC_AT("best_grayscale_mapping_0_255", g_etc1_to_astc_best_grayscale_mapping_0_255)
    ORC_AT("single_color_encoding_0", g_astc_single_color_encoding_0)
    ORC_AT("single_color_encoding_1", g_astc_single_color_encoding_1)
    ORC_AT("etc1_to_astc_selector_mappings", g_etc1_to_astc_selector_mappings)
#undef ORC_AT
    *out_len = 0;
    return nullptr;
}

// approx_move_to_front, driven in lockstep with the Rust port.
void *orc_amf_new(uint32_t n) { return new approx_move_to_front(n); }
void orc_amf_free(void *h) { delete static_cast<approx_move_to_front *>(h); }
void orc_amf_add(void *h, int32_t v) { static_cast<approx_move_to_front *>(h)->add(v); }
void orc_amf_use(void *h, uint32_t i) { static_cast<approx_move_to_front *>(h)->use(i); }
int32_t orc_amf_get(void *h, uint32_t i) { return (*static_cast<approx_move_to_front *>(h))[i]; }

// A bitwise_decoder over a private copy of the buffer (it borrows the bytes).
struct OrcBd {
    std::vector<uint8_t> data;
    bitwise_decoder bd;
};
void *orc_bd_new(const uint8_t *buf, uint32_t n) {
    OrcBd *o = new OrcBd();
    o->data.assign(buf, buf + n);
    o->bd.init(o->data.data(), n);
    return o;
}
void orc_bd_free(void *h) { delete static_cast<OrcBd *>(h); }
uint32_t orc_bd_get_bits(void *h, uint32_t n) { return static_cast<OrcBd *>(h)->bd.get_bits(n); }
uint32_t orc_bd_decode_vlc(void *h, uint32_t cb) {
    return static_cast<OrcBd *>(h)->bd.decode_vlc(cb);
}
uint32_t orc_bd_decode_rice(void *h, uint32_t m) {
    return static_cast<OrcBd *>(h)->bd.decode_rice(m);
}
uint32_t orc_bd_decode_truncated_binary(void *h, uint32_t n) {
    return static_cast<OrcBd *>(h)->bd.decode_truncated_binary(n);
}
// Decodes using the persistent g_orc_huff table (built via orc_huffman_init).
uint32_t orc_bd_decode_huffman(void *h) {
    return static_cast<OrcBd *>(h)->bd.decode_huffman(g_orc_huff);
}

// Universal KTX2 transcode via the upstream public `ktx2_transcoder` API (the
// canonical Basis Universal entry point). This is the conformance ground
// truth: one hook covers every (level, target format, decode flags)
// combination, so no per-format C++ glue is needed as the matrix fills in.
struct OrcKtx2 {
    std::vector<uint8_t> data;
    ktx2_transcoder t;
};
// Open a KTX2 file and start transcoding, reporting the base header fields
// through the out params. Returns null if the reference rejects the file.
void *orc_ktx2_open(const uint8_t *data, uint32_t size, uint32_t *out_w, uint32_t *out_h,
                    uint32_t *out_levels, uint32_t *out_has_alpha, uint32_t *out_is_uastc) {
    if (!data || !size)
        return nullptr;
    basisu_transcoder_init();
    OrcKtx2 *k = new OrcKtx2();
    k->data.assign(data, data + size);
    if (!k->t.init(k->data.data(), static_cast<uint32_t>(k->data.size())) ||
        !k->t.start_transcoding()) {
        delete k;
        return nullptr;
    }
    *out_w = k->t.get_width();
    *out_h = k->t.get_height();
    *out_levels = k->t.get_levels();
    *out_has_alpha = k->t.get_has_alpha();
    *out_is_uastc = k->t.is_uastc() ? 1u : 0u;
    return k;
}
void orc_ktx2_close(void *h) { delete static_cast<OrcKtx2 *>(h); }
// Remaining header fields not reported by orc_ktx2_open.
uint32_t orc_ktx2_layers(void *h) {
    OrcKtx2 *k = static_cast<OrcKtx2 *>(h);
    return k->t.get_layers();
}
uint32_t orc_ktx2_faces(void *h) {
    OrcKtx2 *k = static_cast<OrcKtx2 *>(h);
    return k->t.get_faces();
}
uint32_t orc_ktx2_is_video(void *h) {
    OrcKtx2 *k = static_cast<OrcKtx2 *>(h);
    return k->t.is_video() ? 1u : 0u;
}
// Bytes that transcoding (level, layer, face) to fmt will produce, plus the
// level's logical dimensions. 0 for an invalid level/layer/face.
uint32_t orc_ktx2_level_size(void *h, uint32_t level, uint32_t layer, uint32_t face, int32_t fmt,
                             uint32_t *out_w, uint32_t *out_h) {
    OrcKtx2 *k = static_cast<OrcKtx2 *>(h);
    ktx2_image_level_info info;
    if (!k->t.get_image_level_info(info, level, layer, face))
        return 0;
    *out_w = info.m_orig_width;
    *out_h = info.m_orig_height;
    auto f = static_cast<transcoder_texture_format>(fmt);
    uint32_t bpb = basis_get_bytes_per_block_or_pixel(f);
    return basis_transcoder_format_is_uncompressed(f)
               ? info.m_orig_width * info.m_orig_height * bpb
               : info.m_total_blocks * bpb;
}
// Transcode one image to fmt with the given decode flags. Returns the byte
// count written into out, or 0 on failure or a too-small buffer.
uint32_t orc_ktx2_transcode(void *h, uint32_t level, uint32_t layer, uint32_t face, int32_t fmt,
                            uint32_t flags, uint8_t *out, uint32_t out_cap) {
    OrcKtx2 *k = static_cast<OrcKtx2 *>(h);
    ktx2_image_level_info info;
    if (!k->t.get_image_level_info(info, level, layer, face))
        return 0;
    auto f = static_cast<transcoder_texture_format>(fmt);
    uint32_t bpb = basis_get_bytes_per_block_or_pixel(f);
    bool uncompressed = basis_transcoder_format_is_uncompressed(f);
    uint32_t units = uncompressed ? info.m_orig_width * info.m_orig_height : info.m_total_blocks;
    uint32_t bytes = units * bpb;
    if (!out || bytes > out_cap)
        return 0;
    if (!k->t.transcode_image_level(level, layer, face, out, units, f, flags))
        return 0;
    return bytes;
}

// Universal .basis transcode via the upstream public `basisu_transcoder` API,
// the second Basis Universal container. Mirrors the orc_ktx2_* hooks: one open
// hook decodes the codebooks (start_transcoding), and one transcode hook covers
// every (image, level, target format, decode flags) combination. The API is
// stateless per call, so the data buffer is passed on every call.
struct OrcBasis {
    std::vector<uint8_t> data;
    basisu_transcoder t;
    // For a global-codebook file: the file that provides the shared codebook.
    // It must outlive `t`, which references its lowlevel decoder after
    // set_global_codebooks. Null for a self-contained file.
    std::unique_ptr<OrcBasis> codebook;
};
// Open a .basis file, decode its codebooks, and report base header info. Returns
// null if the reference rejects the file (bad header, global codebooks, etc.).
// out_tex_format receives the raw basis_tex_format byte so the harness can skip
// HDR/XUASTC/ASTC-LDR; out_tex_type receives the raw basis_texture_type byte
// (3 == video). out_w/out_h/out_levels describe image 0.
void *orc_basis_open(const uint8_t *data, uint32_t size, uint32_t *out_w, uint32_t *out_h,
                     uint32_t *out_levels, uint32_t *out_has_alpha, uint32_t *out_tex_format,
                     uint32_t *out_tex_type, uint32_t *out_total_images) {
    if (!data || !size)
        return nullptr;
    basisu_transcoder_init();
    OrcBasis *k = new OrcBasis();
    k->data.assign(data, data + size);
    const uint8_t *p = k->data.data();
    const uint32_t n = static_cast<uint32_t>(k->data.size());
    if (!k->t.validate_header(p, n) || !k->t.start_transcoding(p, n)) {
        delete k;
        return nullptr;
    }
    basisu_image_level_info info;
    if (!k->t.get_image_level_info(p, n, info, 0, 0)) {
        delete k;
        return nullptr;
    }
    *out_w = info.m_orig_width;
    *out_h = info.m_orig_height;
    *out_levels = k->t.get_total_image_levels(p, n, 0);
    *out_has_alpha = info.m_alpha_flag ? 1u : 0u;
    *out_tex_format = static_cast<uint32_t>(k->t.get_basis_tex_format(p, n));
    *out_tex_type = static_cast<uint32_t>(k->t.get_texture_type(p, n));
    *out_total_images = k->t.get_total_images(p, n);
    return k;
}
void orc_basis_close(void *h) { delete static_cast<OrcBasis *>(h); }
// Bytes that transcoding (image, level) to fmt will produce, plus the level's
// logical dimensions. Mirrors orc_ktx2_level_size.
uint32_t orc_basis_level_size(void *h, uint32_t image, uint32_t level, int32_t fmt, uint32_t *out_w,
                              uint32_t *out_h) {
    OrcBasis *k = static_cast<OrcBasis *>(h);
    const uint8_t *p = k->data.data();
    const uint32_t n = static_cast<uint32_t>(k->data.size());
    basisu_image_level_info info;
    if (!k->t.get_image_level_info(p, n, info, image, level))
        return 0;
    *out_w = info.m_orig_width;
    *out_h = info.m_orig_height;
    auto f = static_cast<transcoder_texture_format>(fmt);
    uint32_t bpb = basis_get_bytes_per_block_or_pixel(f);
    return basis_transcoder_format_is_uncompressed(f)
               ? info.m_orig_width * info.m_orig_height * bpb
               : info.m_total_blocks * bpb;
}
// Transcode one (image, level) to fmt with the given decode flags. Returns the
// byte count written into out, or 0 on failure or a too-small buffer.
uint32_t orc_basis_transcode(void *h, uint32_t image, uint32_t level, int32_t fmt, uint32_t flags,
                             uint8_t *out, uint32_t out_cap) {
    OrcBasis *k = static_cast<OrcBasis *>(h);
    const uint8_t *p = k->data.data();
    const uint32_t n = static_cast<uint32_t>(k->data.size());
    basisu_image_level_info info;
    if (!k->t.get_image_level_info(p, n, info, image, level))
        return 0;
    auto f = static_cast<transcoder_texture_format>(fmt);
    uint32_t bpb = basis_get_bytes_per_block_or_pixel(f);
    bool uncompressed = basis_transcoder_format_is_uncompressed(f);
    uint32_t units = uncompressed ? info.m_orig_width * info.m_orig_height : info.m_total_blocks;
    uint32_t bytes = units * bpb;
    if (!out || bytes > out_cap)
        return 0;
    if (!k->t.transcode_image_level(p, n, image, level, out, units, f, flags))
        return 0;
    return bytes;
}
// Open a global-codebook .basis file, supplying the shared ETC1S codebook from a
// separate self-contained file via set_global_codebooks. The codebook file is
// decoded first and kept alive for the returned handle's lifetime, since its
// lowlevel decoder is referenced during transcode. Returns null if either file
// is rejected. Out params describe image 0 of the global-codebook file, exactly
// like orc_basis_open; the handle transcodes and closes through the same hooks.
void *orc_basis_open_with_global_codebook(const uint8_t *cb_data, uint32_t cb_size,
                                          const uint8_t *file_data, uint32_t file_size,
                                          uint32_t *out_w, uint32_t *out_h, uint32_t *out_levels,
                                          uint32_t *out_has_alpha, uint32_t *out_tex_format,
                                          uint32_t *out_tex_type, uint32_t *out_total_images) {
    if (!cb_data || !cb_size || !file_data || !file_size)
        return nullptr;
    basisu_transcoder_init();

    std::unique_ptr<OrcBasis> cb(new OrcBasis());
    cb->data.assign(cb_data, cb_data + cb_size);
    if (!cb->t.validate_header(cb->data.data(), static_cast<uint32_t>(cb->data.size())) ||
        !cb->t.start_transcoding(cb->data.data(), static_cast<uint32_t>(cb->data.size())))
        return nullptr;

    OrcBasis *k = new OrcBasis();
    k->data.assign(file_data, file_data + file_size);
    k->t.set_global_codebooks(&cb->t.get_lowlevel_etc1s_decoder());
    k->codebook = std::move(cb);

    const uint8_t *p = k->data.data();
    const uint32_t n = static_cast<uint32_t>(k->data.size());
    if (!k->t.validate_header(p, n) || !k->t.start_transcoding(p, n)) {
        delete k;
        return nullptr;
    }
    basisu_image_level_info info;
    if (!k->t.get_image_level_info(p, n, info, 0, 0)) {
        delete k;
        return nullptr;
    }
    *out_w = info.m_orig_width;
    *out_h = info.m_orig_height;
    *out_levels = k->t.get_total_image_levels(p, n, 0);
    *out_has_alpha = info.m_alpha_flag ? 1u : 0u;
    *out_tex_format = static_cast<uint32_t>(k->t.get_basis_tex_format(p, n));
    *out_tex_type = static_cast<uint32_t>(k->t.get_texture_type(p, n));
    *out_total_images = k->t.get_total_images(p, n);
    return k;
}

} // extern "C"
