//! Leaf-level differential tests: every low-level oracle hook in
//! `conformance` paired with its Rust counterpart in `basisu`, asserting
//! byte/value equality. Static tables are compared as the raw native bytes
//! the C++ holds in memory (the oracle returns raw memory, so the Rust side
//! serializes to the same layout); functions are driven over their input
//! domains or over deterministic pseudo-random streams.

use basisu::basislz::amf::ApproxMoveToFront;
use basisu::basislz::astc_pack;
use basisu::basislz::astc_tables;
use basisu::basislz::decoder::BitwiseDecoder;
use basisu::basislz::etc1s::Etc1sTranscoder;
use basisu::basislz::huffman::HuffmanDecodingTable;
use basisu::ktx2::{BasisFormat, Ktx2Header};
use basisu::tables;
use basisu::uastc;

/// View a slice of padding-free plain-old-data as raw bytes. Thin wrapper so
/// the tests have one audited unsafe call site; every type passed here is
/// `#[repr(C)]` (or a primitive / nested array of primitives) with no padding.
fn pod<T: Copy>(s: &[T]) -> &[u8] {
    // SAFETY: callers only pass padding-free element types (asserted by the
    // table definitions in basisu, which pin `#[repr(C)]` layouts for this).
    unsafe { basisu::pod_bytes(s) }
}

/// xorshift64* generator: deterministic pseudo-random values with a fixed
/// seed, so both sides of every differential test see identical inputs.
struct Rng(u64);

impl Rng {
    /// Seeded generator (seed 0 is remapped, xorshift state must be nonzero).
    fn new(seed: u64) -> Self {
        Rng(if seed == 0 { 0x9e3779b97f4a7c15 } else { seed })
    }

    /// Next 64-bit value.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545f4914f6cdd1d)
    }

    /// Next 32-bit value.
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform-ish value in `0..n`.
    fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }

    /// A deterministic pseudo-random byte buffer of length `n`.
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u32() as u8).collect()
    }
}

/// Assert two byte slices are identical, reporting the table name, both
/// lengths, and the first differing byte index on failure.
fn assert_bytes_eq(name: &str, got: &[u8], want: &[u8]) {
    assert_eq!(
        got.len(),
        want.len(),
        "{name}: length mismatch (rust {} vs oracle {})",
        got.len(),
        want.len()
    );
    if let Some(i) = (0..got.len()).find(|&i| got[i] != want[i]) {
        panic!(
            "{name}: first differing byte at {i}: rust {:#04x} vs oracle {:#04x}",
            got[i], want[i]
        );
    }
}

/// The eight `.inc` "solution" constant tables (`{m_lo, m_hi, m_err}` entries)
/// match the oracle's `orc_const_table` bytes exactly.
#[test]
fn const_solution_tables_match_oracle() {
    use basisu::tables::solution::{as_bytes, Etc1ToSolution};
    let pairs: [(&str, &[Etc1ToSolution]); 8] = [
        ("etc1_to_astc", &tables::astc::G_ETC1_TO_ASTC),
        (
            "etc1_to_astc_0_255",
            &tables::astc_0_255::G_ETC1_TO_ASTC_0_255,
        ),
        (
            "etc1_to_bc7_m5_color",
            &tables::bc7_m5_color::G_ETC1_TO_BC7_M5_COLOR,
        ),
        ("etc1_to_dxt_5", &tables::dxt1_5::G_ETC1_TO_DXT_5),
        ("etc1_to_dxt_6", &tables::dxt1_6::G_ETC1_TO_DXT_6),
        ("etc1s_to_atc_55", &tables::atc_55::G_ETC1S_TO_ATC_55),
        ("etc1s_to_atc_56", &tables::atc_56::G_ETC1S_TO_ATC_56),
        (
            "etc1s_to_pvrtc2_45",
            &tables::pvrtc2_45::G_ETC1S_TO_PVRTC2_45,
        ),
    ];
    for (name, table) in pairs {
        assert_bytes_eq(name, as_bytes(table), conformance::const_table(name));
    }
}

/// The ETC1S grayscale conversion tables (`g_etc1_g_to_bc7_m5a` via its
/// dedicated hook, plus the `orc_const_table` hooks for `g_etc1_g_to_dxt5a`,
/// `s_etc1_g_to_etc2_a8`, `s_etc1_g_to_etc2_r11`, `g_bc7_m5_equals_1`) match
/// the Rust tables byte for byte.
#[test]
fn etc1_g_conversion_tables_match_oracle() {
    assert_bytes_eq(
        "g_etc1_g_to_bc7_m5a",
        pod(&tables::bc7_m5_alpha::G_ETC1_G_TO_BC7_M5A[..]),
        conformance::etc1_g_to_bc7_m5a(),
    );
    assert_bytes_eq(
        "g_etc1_g_to_dxt5a",
        pod(&tables::dxt5a::G_ETC1_G_TO_DXT5A[..]),
        conformance::const_table("etc1_g_to_dxt5a"),
    );
    assert_bytes_eq(
        "s_etc1_g_to_etc2_a8",
        pod(&tables::etc2_eac_a8::S_ETC1_G_TO_ETC2_A8[..]),
        conformance::const_table("etc1_g_to_etc2_a8"),
    );
    assert_bytes_eq(
        "s_etc1_g_to_etc2_r11",
        pod(&tables::etc2_eac_r11::S_ETC1_G_TO_ETC2_R11[..]),
        conformance::const_table("etc1_g_to_etc2_r11"),
    );
    assert_bytes_eq(
        "g_bc7_m5_equals_1",
        pod(&tables::bc7_m5_equals_1::G_BC7_M5_EQUALS_1[..]),
        conformance::const_table("bc7_m5_equals_1"),
    );
}

/// The four init-built BC1 single-color match tables
/// (`g_bc1_match{5,6}_equals_{0,1}`) match `tables::bc1::tables()`.
#[test]
fn bc1_match_tables_match_oracle() {
    use conformance::Bc1MatchTable::*;
    use tables::bc1::Bc1MatchTables;
    let t = tables::bc1::tables();
    let pairs = [
        ("g_bc1_match5_equals_1", &t.match5_equals_1, Match5Equals1),
        ("g_bc1_match6_equals_1", &t.match6_equals_1, Match6Equals1),
        ("g_bc1_match5_equals_0", &t.match5_equals_0, Match5Equals0),
        ("g_bc1_match6_equals_0", &t.match6_equals_0, Match6Equals0),
    ];
    for (name, table, which) in pairs {
        assert_bytes_eq(
            name,
            &Bc1MatchTables::table_bytes(table),
            conformance::bc1_match_table(which),
        );
    }
}

/// All 13 per-mode UASTC property tables (`g_uastc_mode_*`, 19 bytes each)
/// match `uastc::tables`.
#[test]
fn uastc_mode_tables_match_oracle() {
    use uastc::tables as t;
    let pairs: [(&str, &[u8]); 13] = [
        ("weight_bits", &t::WEIGHT_BITS),
        ("weight_ranges", &t::WEIGHT_RANGES),
        ("endpoint_ranges", &t::ENDPOINT_RANGES),
        ("subsets", &t::SUBSETS),
        ("planes", &t::PLANES),
        ("comps", &t::COMPS),
        ("has_etc1_bias", &t::HAS_ETC1_BIAS),
        ("has_bc1_hint0", &t::HAS_BC1_HINT0),
        ("has_bc1_hint1", &t::HAS_BC1_HINT1),
        ("has_alpha", &t::HAS_ALPHA),
        ("is_la", &t::IS_LA),
        ("cem", &t::CEM),
        ("total_hint_bits", &t::TOTAL_HINT_BITS),
    ];
    for (name, table) in pairs {
        assert_bytes_eq(name, table, conformance::uastc_mode_table(name));
    }
}

/// Every flat (padding-free) table `orc_uastc_flat_table` knows matches its
/// Rust counterpart: the UASTC mode/permutation/BISE tables, the shared
/// ASTC/BC7 pattern tables, the ETC1/EAC helper tables, the ASTC trit/quint
/// encoders, and the BC7 format descriptor tables. Multi-dimensional arrays
/// and non-u8 element types are compared as native bytes, exactly how the
/// C++ holds them.
#[test]
fn uastc_flat_tables_match_oracle() {
    use basisu::etc::tables as etct;
    use uastc::{bc7_tables as bt, patterns as pat};
    let pairs: [(&str, &[u8]); 34] = [
        ("mode_huff_codes", pod(&uastc::tables::MODE_HUFF_CODES[..])),
        (
            "astc_to_bc7_perm",
            pod(&uastc::tables::ASTC_TO_BC7_PERM[..]),
        ),
        (
            "bc7_to_astc_perm",
            pod(&uastc::tables::BC7_TO_ASTC_PERM[..]),
        ),
        (
            "astc_bise_range_table",
            pod(&uastc::tables::ASTC_BISE_RANGE_TABLE[..]),
        ),
        ("patterns2", pod(&pat::ASTC_BC7_PATTERNS2[..])),
        ("patterns3", pod(&pat::ASTC_BC7_PATTERNS3[..])),
        (
            "bc7_3_astc2_patterns2",
            pod(&pat::BC7_3_ASTC2_PATTERNS2[..]),
        ),
        ("pattern2_anchors", pod(&pat::ASTC_BC7_PATTERN2_ANCHORS[..])),
        ("pattern3_anchors", pod(&pat::ASTC_BC7_PATTERN3_ANCHORS[..])),
        (
            "bc7_3_astc2_patterns2_anchors",
            pod(&pat::BC7_3_ASTC2_PATTERNS2_ANCHORS[..]),
        ),
        ("huff_modes", &uastc::huff_modes::HUFF_MODES),
        ("etc1_inten_tables", pod(&etct::INTEN_TABLES[..])),
        ("eac_modifier_table", pod(&etct::EAC_MODIFIER_TABLE[..])),
        ("etc1_pixel_coords", pod(&etct::ETC1_PIXEL_COORDS[..])),
        ("etc1_solid_selectors", pod(&etct::ETC1_SOLID_SELECTORS[..])),
        ("etc2_eac_a8_sel4", &etct::ETC2_EAC_A8_SEL4),
        ("selector_index_to_etc1", &etct::SELECTOR_INDEX_TO_ETC1),
        ("etc_5_to_8", &etct::ETC_5_TO_8),
        (
            "astc_trit_encode",
            &uastc::astc_pack_tables::ASTC_TRIT_ENCODE,
        ),
        (
            "astc_quint_encode",
            &uastc::astc_pack_tables::ASTC_QUINT_ENCODE,
        ),
        ("bc7_partition1", &bt::G_BC7_PARTITION1),
        ("bc7_partition2", &bt::G_BC7_PARTITION2),
        ("bc7_partition3", &bt::G_BC7_PARTITION3),
        ("bc7_anchor_second", &bt::G_BC7_ANCHOR_SECOND),
        ("bc7_anchor_third_1", &bt::G_BC7_ANCHOR_THIRD_1),
        ("bc7_anchor_third_2", &bt::G_BC7_ANCHOR_THIRD_2),
        ("bc7_num_subsets", &bt::G_BC7_NUM_SUBSETS),
        ("bc7_partition_bits", &bt::G_BC7_PARTITION_BITS),
        ("bc7_color_index_bitcount", &bt::G_BC7_COLOR_INDEX_BITCOUNT),
        ("bc7_alpha_index_bitcount", &bt::G_BC7_ALPHA_INDEX_BITCOUNT),
        ("bc7_mode_has_p_bits", &bt::G_BC7_MODE_HAS_P_BITS),
        (
            "bc7_mode_has_shared_p_bits",
            &bt::G_BC7_MODE_HAS_SHARED_P_BITS,
        ),
        ("bc7_color_precision", &bt::G_BC7_COLOR_PRECISION),
        ("bc7_alpha_precision", pod(&bt::G_BC7_ALPHA_PRECISION[..])),
    ];
    for (name, table) in pairs {
        assert_bytes_eq(name, table, conformance::uastc_flat_table(name));
    }
}

/// Every ETC1S->ASTC init-built table `orc_astc_init_table` knows matches the
/// lazily built Rust tables in `basislz::astc_tables`.
#[test]
fn astc_init_tables_match_oracle() {
    conformance::init();
    assert_bytes_eq(
        "g_ise_to_unquant",
        pod(&astc_tables::ise_to_unquant()[..]),
        conformance::astc_init_table("ise_to_unquant"),
    );
    assert_bytes_eq(
        "g_etc1_to_astc_selector_range_index",
        pod(&astc_tables::selector_range_index()[..]),
        conformance::astc_init_table("selector_range_index"),
    );
    assert_bytes_eq(
        "g_etc1_to_astc_best_grayscale_mapping",
        pod(&astc_tables::best_grayscale_mapping_47()[..]),
        conformance::astc_init_table("best_grayscale_mapping"),
    );
    assert_bytes_eq(
        "g_etc1_to_astc_best_grayscale_mapping_0_255",
        pod(&astc_tables::best_grayscale_mapping_0_255()[..]),
        conformance::astc_init_table("best_grayscale_mapping_0_255"),
    );
    assert_bytes_eq(
        "g_astc_single_color_encoding_0",
        &astc_tables::single_color_encoding_0()[..],
        conformance::astc_init_table("single_color_encoding_0"),
    );
    assert_bytes_eq(
        "g_astc_single_color_encoding_1",
        pod(&astc_tables::single_color_encoding_1()[..]),
        conformance::astc_init_table("single_color_encoding_1"),
    );
    assert_bytes_eq(
        "g_etc1_to_astc_selector_mappings",
        pod(&astc_tables::SELECTOR_MAPPINGS[..]),
        conformance::astc_init_table("etc1_to_astc_selector_mappings"),
    );
}

/// ASTC BISE endpoint machinery: `astc_get_levels` and
/// `astc_is_valid_endpoint_range` over all 21 ranges, both unquantizers over
/// their full input domains (every packed value of every valid range, and
/// every (bits, trit, quint) decomposition), and the init-built
/// `g_astc_unquant[21][256]` table bytes.
#[test]
fn bise_unquant_matches_oracle() {
    conformance::init();
    for range in 0..21u32 {
        assert_eq!(
            uastc::bise::astc_get_levels(range),
            conformance::astc_get_levels(range),
            "astc_get_levels({range})"
        );
        let valid = uastc::bise::astc_is_valid_endpoint_range(range);
        assert_eq!(
            valid,
            conformance::astc_is_valid_endpoint_range(range),
            "astc_is_valid_endpoint_range({range})"
        );
        if !valid {
            continue;
        }
        let levels = uastc::bise::astc_get_levels(range);
        for v in 0..levels {
            assert_eq!(
                uastc::bise::unquant_astc_endpoint_val(v, range),
                conformance::unquant_astc_endpoint_val(v, range),
                "unquant_astc_endpoint_val({v}, {range})"
            );
        }
        // decomposed form: every in-range (bits, trit, quint) triple. Pure-bit
        // ranges must pass zero trits/quints (the reference asserts this).
        let t = &uastc::tables::ASTC_BISE_RANGE_TABLE[range as usize];
        let (bits, has_trits, has_quints) = (t[0] as u32, t[1] != 0, t[2] != 0);
        let trit_hi = if has_trits { 3 } else { 1 };
        let quint_hi = if has_quints { 5 } else { 1 };
        for b in 0..(1u32 << bits) {
            for trit in 0..trit_hi {
                for quint in 0..quint_hi {
                    assert_eq!(
                        uastc::bise::unquant_astc_endpoint(b, trit, quint, range),
                        conformance::unquant_astc_endpoint(b, trit, quint, range),
                        "unquant_astc_endpoint({b}, {trit}, {quint}, {range})"
                    );
                }
            }
        }
    }
    assert_bytes_eq(
        "g_astc_unquant",
        pod(&uastc::bise::astc_unquant()[..]),
        conformance::astc_unquant_table(),
    );
}

/// The init-built BC7 mode-5 (`[256]`) and mode-6 (`[256][2]`) optimal
/// single-color endpoint tables match `uastc::bc7`.
#[test]
fn bc7_optimal_endpoint_tables_match_oracle() {
    conformance::init();
    assert_bytes_eq(
        "g_bc7_mode_5_optimal_endpoints",
        pod(&uastc::bc7::mode5_optimal()[..]),
        conformance::bc7_mode5_optimal(),
    );
    assert_bytes_eq(
        "g_bc7_mode_6_optimal_endpoints",
        pod(&uastc::bc7::mode6_optimal()[..]),
        conformance::bc7_mode6_optimal(),
    );
}

/// The shared ASTC/BC7 partition descriptor tables
/// (`g_astc_bc7_common_partitions2/3`, `g_bc7_3_astc2_common_partitions`),
/// compared field-wise because the C++ structs carry padding.
#[test]
fn partition_descriptor_tables_match_oracle() {
    let (bc7, astc, invert) = conformance::partitions2();
    for (i, p) in uastc::partitions::ASTC_BC7_COMMON_PARTITIONS2
        .iter()
        .enumerate()
    {
        assert_eq!(p.bc7, bc7[i], "partitions2[{i}].bc7");
        assert_eq!(p.astc, astc[i], "partitions2[{i}].astc");
        assert_eq!(u8::from(p.invert), invert[i], "partitions2[{i}].invert");
    }
    let (bc7, astc, perm) = conformance::partitions3();
    for (i, p) in uastc::partitions::ASTC_BC7_COMMON_PARTITIONS3
        .iter()
        .enumerate()
    {
        assert_eq!(p.bc7, bc7[i], "partitions3[{i}].bc7");
        assert_eq!(p.astc, astc[i], "partitions3[{i}].astc");
        assert_eq!(p.astc_to_bc7_perm, perm[i], "partitions3[{i}].perm");
    }
    let (bc73, astc2, k) = conformance::bc7_3_astc2_partitions();
    for (i, p) in uastc::partitions::BC7_3_ASTC2_COMMON_PARTITIONS
        .iter()
        .enumerate()
    {
        assert_eq!(p.bc73, bc73[i], "bc7_3_astc2[{i}].bc73");
        assert_eq!(p.astc2, astc2[i], "bc7_3_astc2[{i}].astc2");
        assert_eq!(p.k, k[i], "bc7_3_astc2[{i}].k");
    }
}

/// The UASTC block bit readers (`read_bit`, `read_bits1_to_9`,
/// `read_bits1_to_9_fst`, `read_bits64`) agree with `basisu::bitreader` for
/// every codesize (0..=9, and 0..=64 for the 64-bit reader) at a spread of
/// bit offsets over a pseudo-random buffer, including offsets that cross the
/// reference's 112-bit fast/slow path boundary.
#[test]
fn bit_readers_match_oracle() {
    let buf = Rng::new(0x1eaf_b175).bytes(64);

    // single bit reads across the whole buffer
    for off in 0..500u32 {
        let mut r_off = off;
        let got = basisu::bitreader::read_bit(&buf, &mut r_off);
        let (want, w_off) = conformance::read_bit(&buf, off);
        assert_eq!((got, r_off), (want, w_off), "read_bit at {off}");
    }

    // 0..=9 bit fields at every offset that keeps the two-byte read in bounds
    for codesize in 0..=9u32 {
        for off in 0..=440u32 {
            let mut r_off = off;
            let got = basisu::bitreader::read_bits1_to_9(&buf, &mut r_off, codesize);
            let (want, w_off) = conformance::read_bits1_to_9(&buf, off, codesize);
            assert_eq!(
                (got, r_off),
                (want, w_off),
                "read_bits1_to_9 cs={codesize} at {off}"
            );
        }
        // the fst variant requires bit_offset < 112 (UASTC block cursor)
        for off in 0..112u32 {
            let mut r_off = off;
            let got = basisu::bitreader::read_bits1_to_9_fst(&buf, &mut r_off, codesize);
            let (want, w_off) = conformance::read_bits1_to_9_fst(&buf, off, codesize);
            assert_eq!(
                (got, r_off),
                (want, w_off),
                "read_bits1_to_9_fst cs={codesize} at {off}"
            );
        }
    }

    // 0..=64 bit fields at offsets that keep every byte access in bounds
    for codesize in 0..=64u32 {
        for off in (0..=440u32).step_by(7) {
            let mut r_off = off;
            let got = basisu::bitreader::read_bits64(&buf, &mut r_off, codesize);
            let (want, w_off) = conformance::read_bits64(&buf, off, codesize);
            assert_eq!(
                (got, r_off),
                (want, w_off),
                "read_bits64 cs={codesize} at {off}"
            );
        }
    }

    // a sequential walk mixing all four readers, sharing one advancing cursor
    let mut rng = Rng::new(0x3a1c_57e9);
    let mut off = 0u32;
    while off < 400 {
        let (got, want, w_off) = match rng.below(4) {
            0 => {
                let mut r = off;
                let g = basisu::bitreader::read_bit(&buf, &mut r) as u64;
                let (w, wo) = conformance::read_bit(&buf, off);
                off = r;
                (g, w as u64, wo)
            }
            1 => {
                let cs = rng.below(10);
                let mut r = off;
                let g = basisu::bitreader::read_bits1_to_9(&buf, &mut r, cs) as u64;
                let (w, wo) = conformance::read_bits1_to_9(&buf, off, cs);
                off = r;
                (g, w as u64, wo)
            }
            2 => {
                let cs = rng.below(10);
                if off >= 100 {
                    continue;
                }
                let mut r = off;
                let g = basisu::bitreader::read_bits1_to_9_fst(&buf, &mut r, cs) as u64;
                let (w, wo) = conformance::read_bits1_to_9_fst(&buf, off, cs);
                off = r;
                (g, w as u64, wo)
            }
            _ => {
                let cs = rng.below(65);
                let mut r = off;
                let g = basisu::bitreader::read_bits64(&buf, &mut r, cs);
                let (w, wo) = conformance::read_bits64(&buf, off, cs);
                off = r;
                (g, w, wo)
            }
        };
        assert_eq!((got, off), (want, w_off), "bit reader walk");
    }
}

/// The BasisLZ `bitwise_decoder` primitives (`get_bits`, `decode_vlc`,
/// `decode_rice`, `decode_truncated_binary`) agree with the Rust decoder over
/// a long scripted pseudo-random op sequence on a shared random buffer,
/// including reads past the end (both sides read zeros there).
#[test]
fn bitwise_decoder_matches_oracle() {
    let data = Rng::new(0xdec0de).bytes(512);
    let orc = conformance::OracleBd::new(&data);
    let mut rs = BitwiseDecoder::new(&data);
    let mut ops = Rng::new(0x0b5e55ed);
    for step in 0..4000 {
        match ops.below(4) {
            0 => {
                let n = 1 + ops.below(32);
                assert_eq!(rs.get_bits(n), orc.get_bits(n), "get_bits({n}) step {step}");
            }
            1 => {
                let cb = 1 + ops.below(8);
                assert_eq!(
                    rs.decode_vlc(cb),
                    orc.decode_vlc(cb),
                    "decode_vlc({cb}) step {step}"
                );
            }
            2 => {
                let m = 1 + ops.below(8);
                assert_eq!(
                    rs.decode_rice(m),
                    orc.decode_rice(m),
                    "decode_rice({m}) step {step}"
                );
            }
            _ => {
                let n = 2 + ops.below(1000);
                assert_eq!(
                    rs.decode_truncated_binary(n),
                    orc.decode_truncated_binary(n),
                    "decode_truncated_binary({n}) step {step}"
                );
            }
        }
    }
}

/// Canonical Huffman table construction and decoding: `huffman_init` on a
/// spread of valid code-size sets (short codes, long codes that spill into
/// the overflow tree, sparse sets, the single-symbol special case) matches
/// the Rust builder's lookup/tree/code_sizes exactly, invalid sets are
/// rejected by both, and `decode_huffman` over a shared random stream yields
/// identical symbols.
#[test]
fn huffman_tables_and_decode_match_oracle() {
    // valid canonical sets (each either completes the code space or is the
    // single-used-symbol special case)
    let valid: &[Vec<u8>] = &[
        vec![1, 1],
        vec![1, 2, 2],
        vec![2, 2, 2, 2],
        vec![1, 2, 3, 3],
        vec![3; 8],
        vec![8; 256],
        // staircase reaching past FAST_LOOKUP_BITS so the overflow tree is used
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 15],
        // sparse: zero-length codes interleaved with a complete 2-bit code
        vec![0, 2, 0, 2, 2, 0, 2],
        // single used symbol (the lone incomplete case the format allows)
        vec![1],
        vec![3],
        vec![0, 0, 4, 0],
    ];
    for (ci, sizes) in valid.iter().enumerate() {
        let (ok, lookup, tree, cs) = conformance::huffman_init(sizes);
        assert!(ok, "oracle rejected valid code set {ci}");
        let mut table = HuffmanDecodingTable::new();
        assert!(
            table.init(sizes.len(), sizes),
            "rust rejected valid code set {ci}"
        );
        assert_eq!(table.code_sizes, cs, "code_sizes for set {ci}");
        assert_eq!(table.lookup, lookup, "lookup for set {ci}");
        assert_eq!(table.tree, tree, "tree for set {ci}");

        // decode a shared pseudo-random stream with the freshly built tables
        // (the oracle decodes with its persistent table, just initialized)
        let stream = Rng::new(0x477 + ci as u64).bytes(256);
        let orc = conformance::OracleBd::new(&stream);
        let mut rs = BitwiseDecoder::new(&stream);
        for step in 0..200 {
            assert_eq!(
                rs.decode_huffman(&table),
                orc.decode_huffman(),
                "decode_huffman set {ci} step {step}"
            );
        }
    }

    // invalid sets: over-subscribed, incomplete, and over-long code sizes
    let invalid: &[Vec<u8>] = &[vec![1, 1, 1], vec![2, 2, 2], vec![1, 1, 2], vec![32]];
    for (ci, sizes) in invalid.iter().enumerate() {
        let (ok, _, _, _) = conformance::huffman_init(sizes);
        assert!(!ok, "oracle accepted invalid code set {ci}");
        let mut table = HuffmanDecodingTable::new();
        assert!(
            !table.init(sizes.len(), sizes),
            "rust accepted invalid code set {ci}"
        );
    }
}

/// `approx_move_to_front` driven in lockstep: a scripted mix of `add` and
/// `use` operations over several buffer sizes, comparing every slot after
/// every step.
#[test]
fn amf_matches_oracle() {
    for n in [1u32, 2, 5, 16, 64] {
        let orc = conformance::OracleAmf::new(n);
        let mut rs = ApproxMoveToFront::new(n as usize);
        let mut rng = Rng::new(0xa3f + n as u64);
        for step in 0..300 {
            if rng.below(2) == 0 {
                let v = rng.next_u32() as i32;
                orc.add(v);
                rs.add(v);
            } else {
                let i = rng.below(n);
                orc.use_index(i);
                rs.use_index(i as usize);
            }
            for i in 0..n {
                assert_eq!(
                    rs.get(i as usize),
                    orc.get(i),
                    "amf n={n} step {step} slot {i}"
                );
            }
        }
    }
}

/// Little-endian u16 at `o` (KTX2 stores integers LE).
fn rd16(d: &[u8], o: usize) -> u32 {
    u16::from_le_bytes([d[o], d[o + 1]]) as u32
}
/// Little-endian u32 at `o`.
fn rd32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// One slice transcode comparison row: label, rust output, oracle output.
type SliceCase<'a> = (&'a str, Option<Vec<u8>>, Option<Vec<u8>>);

/// The BasisLZ supercompression global data ranges of an ETC1S KTX2, parsed
/// the same way the transcoders do: `(num_endpoints, num_selectors,
/// endpoints, selectors, tables, rgb_slice, alpha_slice)` for level-0
/// image-0.
#[allow(clippy::type_complexity)]
fn etc1s_ranges<'a>(
    h: &Ktx2Header,
    data: &'a [u8],
) -> (
    u32,
    u32,
    &'a [u8],
    &'a [u8],
    &'a [u8],
    &'a [u8],
    Option<&'a [u8]>,
) {
    let sgd = h.sgd_byte_offset as usize;
    let num_e = rd16(data, sgd);
    let num_s = rd16(data, sgd + 2);
    let e_len = rd32(data, sgd + 4) as usize;
    let s_len = rd32(data, sgd + 8) as usize;
    let t_len = rd32(data, sgd + 12) as usize;
    let image_count = h.level_count.max(1) as usize
        * h.layer_count.max(1) as usize
        * h.face_count.max(1) as usize;
    let e_ofs = sgd + 20 + 20 * image_count;
    let s_ofs = e_ofs + e_len;
    let t_ofs = s_ofs + s_len;

    // level-0 image-0 slice locations from the first image desc
    let desc0 = sgd + 20;
    let level0 = h.levels[0].byte_offset as usize;
    let rgb_ofs = level0 + rd32(data, desc0 + 4) as usize;
    let rgb_len = rd32(data, desc0 + 8) as usize;
    let alpha_len = rd32(data, desc0 + 16) as usize;
    let alpha = (alpha_len != 0).then(|| {
        let alpha_ofs = level0 + rd32(data, desc0 + 12) as usize;
        &data[alpha_ofs..alpha_ofs + alpha_len]
    });
    (
        num_e,
        num_s,
        &data[e_ofs..e_ofs + e_len],
        &data[s_ofs..s_ofs + s_len],
        &data[t_ofs..t_ofs + t_len],
        &data[rgb_ofs..rgb_ofs + rgb_len],
        alpha,
    )
}

/// ETC1S container leaves: for every smoke file the ETC1S oracle accepts,
/// the raw SGD ranges, decoded endpoint/selector codebooks, level-0
/// dimensions, raw slices, every `transcode_slice_*` target, and the
/// combined alpha paths (`transcode_*_rgba`) all match the Rust
/// `basislz::etc1s` implementation.
#[test]
fn etc1s_container_leaves_match_oracle() {
    conformance::init();
    let mut tested = 0;
    for path in conformance::smoke_files() {
        let data = std::fs::read(&path).unwrap();
        let orc = conformance::OracleEtc1s::open(&data);
        if !orc.ok {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        tested += 1;

        // the oracle only accepts ETC1S KTX2 files, so the Rust header parse
        // must agree
        let h = Ktx2Header::parse(&data).unwrap_or_else(|| panic!("{name}: rust header parse"));
        assert_eq!(h.format, BasisFormat::Etc1s, "{name}: format");
        let (num_e, num_s, e_data, s_data, t_data, rgb, alpha) = etc1s_ranges(&h, &data);

        // raw SGD ranges
        assert_eq!(num_e, orc.num_endpoints, "{name}: endpoint count");
        assert_eq!(num_s, orc.num_selectors, "{name}: selector count");
        assert_bytes_eq(&format!("{name}: endpoint data"), e_data, orc.data(0));
        assert_bytes_eq(&format!("{name}: selector data"), s_data, orc.data(1));
        assert_bytes_eq(&format!("{name}: table data"), t_data, orc.data(2));

        // dimensions
        let (bx, by) = (h.width.div_ceil(4), h.height.div_ceil(4));
        assert_eq!((bx, by), orc.dims(), "{name}: block dims");
        assert_eq!((h.width, h.height), orc.wh(), "{name}: pixel dims");

        // raw slices
        assert_bytes_eq(&format!("{name}: rgb slice"), rgb, orc.slice());
        assert_eq!(alpha.is_some(), orc.has_alpha(), "{name}: has_alpha");
        if let Some(a) = alpha {
            assert_bytes_eq(&format!("{name}: alpha slice"), a, orc.alpha_slice());
        }

        // codebook decode (decode_palettes + decode_tables)
        let mut t = Etc1sTranscoder::new();
        assert!(
            t.decode_palettes(num_e, e_data, num_s, s_data),
            "{name}: rust decode_palettes"
        );
        assert!(t.decode_tables(t_data), "{name}: rust decode_tables");
        assert_eq!(t.endpoints.len(), num_e as usize, "{name}: endpoints len");
        assert_eq!(t.selectors.len(), num_s as usize, "{name}: selectors len");
        for i in 0..num_e {
            let e = &t.endpoints[i as usize];
            let got = [e.color5.c[0], e.color5.c[1], e.color5.c[2], e.inten5];
            assert_eq!(got, orc.endpoint(i), "{name}: endpoint {i}");
        }
        for i in 0..num_s {
            let s = &t.selectors[i as usize];
            let mut got = [0u8; 8];
            got[0..4].copy_from_slice(&s.selectors);
            got[4..8].copy_from_slice(&s.bytes);
            assert_eq!(got, orc.selector(i), "{name}: selector {i}");
        }

        // slice-level transcodes (level-0 image-0, color slice). The Rust
        // converters write into a caller-sized buffer; wrap them back into
        // the oracle's owned-Vec shape for the comparison.
        let n = (bx * by) as usize;
        let into_vec = |len: usize, f: &dyn Fn(&mut [u8]) -> Option<()>| -> Option<Vec<u8>> {
            let mut out = vec![0u8; len];
            f(&mut out).map(|()| out)
        };
        let cases: [SliceCase; 5] = [
            (
                "slice etc1",
                into_vec(n * 8, &|o| t.transcode_slice_etc1(rgb, bx, by, None, o)),
                orc.transcode_slice_etc1(),
            ),
            (
                "slice rgba32",
                into_vec((h.width * h.height * 4) as usize, &|o| {
                    t.transcode_slice_rgba32(rgb, bx, by, h.width, h.height, None, o)
                }),
                orc.transcode_slice_rgba32(h.width, h.height),
            ),
            (
                "slice astc",
                into_vec(n * 16, &|o| t.transcode_slice_astc(rgb, bx, by, None, o)),
                orc.transcode_slice_astc(),
            ),
            // decode_flags = 0 in the oracle means chroma filtering is on
            (
                "slice bc7",
                into_vec(n * 16, &|o| {
                    t.transcode_slice_bc7(rgb, bx, by, true, None, o)
                }),
                orc.transcode_slice_bc7(),
            ),
            (
                "slice eac_a8",
                into_vec(n * 8, &|o| t.transcode_slice_eac_a8(rgb, bx, by, None, o)),
                orc.transcode_slice_eac_a8(),
            ),
        ];
        for (what, got, want) in cases {
            let want = want.unwrap_or_else(|| panic!("{name}: oracle {what} failed"));
            let got = got.unwrap_or_else(|| panic!("{name}: rust {what} failed"));
            assert_bytes_eq(&format!("{name}: {what}"), &got, &want);
        }

        // combined color+alpha paths
        if let Some(a) = alpha {
            let cases: [SliceCase; 3] = [
                (
                    "etc2_rgba",
                    into_vec(n * 16, &|o| {
                        t.transcode_image_etc2_rgba(rgb, a, bx, by, None, o)
                    }),
                    orc.transcode_etc2_rgba(),
                ),
                (
                    "bc7_rgba",
                    into_vec(n * 16, &|o| {
                        t.transcode_image_bc7_rgba(rgb, a, bx, by, true, None, o)
                    }),
                    orc.transcode_bc7_rgba(),
                ),
                (
                    "astc_rgba",
                    into_vec(n * 16, &|o| {
                        t.transcode_image_astc_rgba(rgb, a, bx, by, None, o)
                    }),
                    orc.transcode_astc_rgba(),
                ),
            ];
            for (what, got, want) in cases {
                let want = want.unwrap_or_else(|| panic!("{name}: oracle {what} failed"));
                let got = got.unwrap_or_else(|| panic!("{name}: rust {what} failed"));
                assert_bytes_eq(&format!("{name}: {what}"), &got, &want);
            }
        }
    }
    // the committed smoke set has three ETC1S KTX2 files (etc1s.ktx2,
    // etc1s_alpha.ktx2, kodim23.ktx2); fail loudly if the corpus shrinks
    assert!(tested >= 3, "only {tested} ETC1S smoke files tested");
}

/// All 16-byte UASTC blocks of every level of the smoke corpus UASTC KTX2
/// files (none are supercompressed, so the level data is raw blocks).
fn uastc_corpus_blocks() -> Vec<[u8; 16]> {
    let mut blocks: Vec<[u8; 16]> = Vec::new();
    for path in conformance::smoke_files() {
        let data = std::fs::read(&path).unwrap();
        let Some(h) = Ktx2Header::parse(&data) else {
            continue;
        };
        if h.format != BasisFormat::Uastc {
            continue;
        }
        assert_eq!(h.supercompression, 0, "smoke UASTC files are ss=0");
        for l in &h.levels {
            let lvl = &data[l.byte_offset as usize..(l.byte_offset + l.byte_length) as usize];
            for chunk in lvl.chunks_exact(16) {
                blocks.push(chunk.try_into().unwrap());
            }
        }
    }
    assert!(blocks.len() >= 1000, "expected a real UASTC corpus");
    blocks
}

/// `unpack_uastc` parity for one block, both sRGB flags. Returns whether the
/// block's mode is valid (both sides agreed either way).
fn check_unpack(bi: usize, blk: &[u8; 16]) -> bool {
    let mut valid = false;
    for srgb in [false, true] {
        let got = uastc::unpack::unpack_uastc(blk, srgb).map(|px| {
            let mut out = [0u8; 64];
            for (i, p) in px.iter().enumerate() {
                out[i * 4..i * 4 + 4].copy_from_slice(&p.c);
            }
            out
        });
        let want = conformance::uastc_unpack_pixels(blk, srgb);
        assert_eq!(got, want, "unpack_uastc block {bi} srgb {srgb}");
        valid = got.is_some();
    }
    valid
}

/// Per-block transcode parity for one block: ETC1, ETC2_RGBA, ASTC, BC7.
fn check_transcodes(bi: usize, blk: &[u8; 16]) {
    assert_eq!(
        basisu::etc::uastc_to_etc1::transcode_uastc_to_etc1(blk),
        conformance::transcode_uastc_to_etc1(blk),
        "uastc->etc1 block {bi}"
    );
    assert_eq!(
        basisu::etc::uastc_to_etc2::transcode_uastc_to_etc2_rgba(blk),
        conformance::transcode_uastc_to_etc2_rgba(blk),
        "uastc->etc2_rgba block {bi}"
    );
    assert_eq!(
        uastc::astc_pack::transcode_uastc_to_astc(blk),
        conformance::transcode_uastc_to_astc(blk),
        "uastc->astc block {bi}"
    );
    assert_eq!(
        uastc::bc7::transcode_uastc_to_bc7(blk),
        conformance::transcode_uastc_to_bc7(blk),
        "uastc->bc7 block {bi}"
    );
}

/// UASTC per-block leaves: `unpack_uastc` (both sRGB flags) and the
/// per-block transcodes to ETC1, ETC2_RGBA, ASTC, and BC7 match the Rust
/// per-block functions on every corpus block, on degenerate all-0/all-1
/// blocks, and on corpus blocks with pseudo-randomized weight bits.
/// Fully random blocks are checked for unpack parity and (when the mode is
/// invalid) rejection parity only: the oracle is compiled with asserts on,
/// and random hint bits legitimately trip reference asserts (for example
/// `multiplier >= 1` in the ETC2 EAC path) on blocks no encoder would emit.
#[test]
fn uastc_block_leaves_match_oracle() {
    conformance::init();
    let corpus = uastc_corpus_blocks();
    let mut rng = Rng::new(0x0a57c);

    // encoder-plausible set: real blocks, degenerate blocks, and real blocks
    // with the trailing weight bytes randomized (mode and hint bits kept, so
    // the reference's hint-validity asserts hold)
    let mut blocks = corpus.clone();
    blocks.push([0u8; 16]);
    blocks.push([0xff; 16]);
    for _ in 0..512 {
        let mut b = corpus[rng.below(corpus.len() as u32) as usize];
        for byte in &mut b[10..16] {
            *byte = rng.next_u32() as u8;
        }
        blocks.push(b);
    }
    for (bi, blk) in blocks.iter().enumerate() {
        check_unpack(bi, blk);
        check_transcodes(bi, blk);
    }

    // fully random blocks: unpack parity always; transcode parity only for
    // invalid modes (both sides must reject those through every hook)
    let mut invalid_seen = 0;
    for bi in 0..256 {
        let blk: [u8; 16] = rng.bytes(16).try_into().unwrap();
        if !check_unpack(usize::MAX - bi, &blk) {
            check_transcodes(usize::MAX - bi, &blk);
            invalid_seen += 1;
        }
    }
    assert!(invalid_seen > 0, "no invalid-mode random blocks generated");
}

/// The four fixed-layout ETC1S->ASTC CEM packers
/// (`astc_pack_block_cem_{12_weight_range2, 12_weight_range0,
/// 4_weight_range2, 8_weight_range2}`) produce identical blocks for a spread
/// of in-domain endpoint/weight inputs.
#[test]
fn astc_pack_cem_matches_oracle() {
    conformance::init();
    let mut rng = Rng::new(0xce4);
    for case in 0..64 {
        for which in 0..4u32 {
            let mut endpoints = [0u8; 10];
            for e in endpoints.iter_mut() {
                // cem_12_weight_range2 BISE-encodes endpoints in [0, 47];
                // the other packers take full 8-bit endpoints
                *e = if which == 0 {
                    rng.below(48) as u8
                } else {
                    rng.next_u32() as u8
                };
            }
            let mut weights = [0u8; 32];
            for w in weights.iter_mut() {
                // range0 weights are 1 bit, range2 weights are 2 bits
                *w = if which == 1 {
                    rng.below(2) as u8
                } else {
                    rng.below(4) as u8
                };
            }
            let blk = astc_pack::AstcBlockParams { endpoints, weights };
            let got = match which {
                0 => astc_pack::pack_cem_12_weight_range2(&blk),
                1 => astc_pack::pack_cem_12_weight_range0(&blk),
                2 => astc_pack::pack_cem_4_weight_range2(&blk),
                _ => astc_pack::pack_cem_8_weight_range2(&blk),
            };
            let want = conformance::astc_pack_cem(which, &endpoints, &weights);
            assert_eq!(got, want, "astc_pack_cem which {which} case {case}");
        }
    }
}

/// Rust-side mirror of the oracle's `orc_astc_unpack_and_decode` contract:
/// unpack one physical ASTC block of footprint `bw` x `bh` and decode it in
/// `mode` (0 SRGB8, 1 LDR8, 2 HDR16, 3 RGB9E5), serializing decoded texels
/// to the oracle's little-endian layout.
fn rust_astc_unpack_and_decode(
    blk: &[u8; 16],
    bw: u32,
    bh: u32,
    mode: u32,
) -> conformance::AstcDecodeResult {
    use basisu::astc::{decode, unpack};
    use conformance::AstcDecodeResult::*;
    let Some(log) = unpack::unpack_block(blk, bw, bh) else {
        return UnpackRejected;
    };
    let n = (bw * bh) as usize;
    let out = match mode {
        0 | 1 => {
            let mut texels = vec![[0u8; 4]; n];
            if decode::decode_block_ldr8(&log, bw, bh, mode == 0, &mut texels).is_none() {
                return DecodeRejected;
            }
            texels.into_iter().flatten().collect()
        }
        2 => {
            let mut texels = vec![[0u16; 4]; n];
            if decode::decode_block_hdr16(&log, bw, bh, &mut texels).is_none() {
                return DecodeRejected;
            }
            texels
                .into_iter()
                .flatten()
                .flat_map(u16::to_le_bytes)
                .collect()
        }
        _ => {
            let mut texels = vec![0u32; n];
            if decode::decode_block_9e5(&log, bw, bh, &mut texels).is_none() {
                return DecodeRejected;
            }
            texels.into_iter().flat_map(u32::to_le_bytes).collect()
        }
    };
    Decoded(out)
}

/// Assert unpack+decode parity for one block at one footprint across all four
/// decode modes. Returns how many modes fully decoded.
fn check_astc_decode(ctx: &str, blk: &[u8; 16], bw: u32, bh: u32) -> u32 {
    let mut decoded = 0;
    for mode in 0..4u32 {
        let want = conformance::astc_unpack_and_decode(blk, bw, bh, mode);
        let got = rust_astc_unpack_and_decode(blk, bw, bh, mode);
        assert_eq!(got, want, "{ctx} {bw}x{bh} mode {mode} block {blk:02x?}");
        if matches!(want, conformance::AstcDecodeResult::Decoded(_)) {
            decoded += 1;
        }
    }
    decoded
}

/// The generic ASTC decode (`astc_helpers::unpack_block` + `decode_block`)
/// matches the oracle byte-for-byte in all four decode modes, at every one of
/// the 14 block footprints. Real ASTC 4x4 LDR blocks (the UASTC->ASTC
/// transcode of the smoke corpus) and real UASTC HDR blocks exercise every
/// footprint (a valid 4x4 block is a valid config at any larger footprint,
/// which drives the generic weight upsampler and the large-block partition
/// path); raw-ASTC files of every footprint join in when the full corpus is
/// fetched; random blocks pin rejection parity.
#[test]
fn astc_generic_decode_matches_oracle() {
    use basisu::{AstcBlock, DecodeFlags, SourceFormat, TargetFormat, Transcoder};
    conformance::init();

    // Real 4x4 ASTC blocks from the smoke corpus: LDR via UASTC->ASTC,
    // HDR via the UASTC HDR pass-through.
    let mut pool = Vec::<[u8; 16]>::new();
    for path in conformance::smoke_files() {
        let data = std::fs::read(&path).unwrap();
        let Ok(tex) = Transcoder::new(&data) else {
            continue;
        };
        let target = match tex.source_format() {
            SourceFormat::UastcLdr => TargetFormat::Astc4x4Rgba,
            SourceFormat::UastcHdr4x4 => TargetFormat::AstcHdr4x4Rgba,
            _ => continue,
        };
        if tex.is_video() {
            continue;
        }
        let Ok(astc) = tex.transcode(0, target, DecodeFlags::NONE) else {
            continue;
        };
        pool.extend(
            astc.chunks_exact(16)
                .map(|c| <[u8; 16]>::try_from(c).unwrap()),
        );
    }
    assert!(pool.len() > 500, "expected real ASTC blocks from smoke");
    // Thin the pool deterministically to keep 14 footprints x 4 modes fast.
    let mut rng = Rng::new(0xa57cdec0de);
    let mut decoded_any = 0u64;
    for _ in 0..384 {
        let blk = pool[rng.below(pool.len() as u32) as usize];
        for b in AstcBlock::ALL {
            let (bw, bh) = b.dims();
            decoded_any += u64::from(check_astc_decode("smoke-pool", &blk, bw, bh));
        }
    }
    assert!(decoded_any > 1000, "too few decoded cases: {decoded_any}");

    // Raw-ASTC corpus files at their native footprint (full corpus only).
    let mut corpus_files_used = 0u32;
    for path in conformance::corpus_files() {
        if corpus_files_used >= 128 {
            break;
        }
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let Ok(tex) = Transcoder::new(&data) else {
            continue;
        };
        let (target, bw, bh) = match tex.source_format() {
            SourceFormat::AstcLdr(b) => {
                let (bw, bh) = b.dims();
                (b.passthrough_target(), bw, bh)
            }
            SourceFormat::AstcHdr6x6 => (TargetFormat::AstcHdr6x6Rgba, 6, 6),
            _ => continue,
        };
        let Ok(raw) = tex.transcode(0, target, DecodeFlags::NONE) else {
            continue;
        };
        corpus_files_used += 1;
        for blk in raw.chunks_exact(16).take(64) {
            check_astc_decode("corpus", blk.try_into().unwrap(), bw, bh);
        }
    }

    // Random blocks: rejection parity at every footprint.
    for b in AstcBlock::ALL {
        let (bw, bh) = b.dims();
        for _ in 0..256 {
            let blk: [u8; 16] = rng.bytes(16).try_into().unwrap();
            check_astc_decode("random", &blk, bw, bh);
        }
    }
}

/// The bc7f BC7 packer matches the oracle byte-for-byte through its auto-RGBA
/// entry (the only entry the raw-ASTC paths use) at both transcoder flag
/// sets, over decoded real corpus blocks (RGB pipelines), alpha-modified
/// variants (RGBA pipelines), solid blocks, and random pixels.
#[test]
fn bc7f_pack_matches_oracle() {
    use basisu::fastenc::bc7f;
    conformance::init();

    const FLAG_SETS: [u32; 2] = [
        bc7f::flags::DEFAULT,
        bc7f::flags::DEFAULT_PARTIALLY_ANALYTICAL,
    ];

    let mut checked = 0u64;
    let mut check = |ctx: &str, pixels: &[u8; 64]| {
        let px: [[u8; 4]; 16] = core::array::from_fn(|i| {
            [
                pixels[i * 4],
                pixels[i * 4 + 1],
                pixels[i * 4 + 2],
                pixels[i * 4 + 3],
            ]
        });
        for flags in FLAG_SETS {
            let want = conformance::bc7f_pack_rgba(pixels, flags);
            let mut got = [0u8; 16];
            bc7f::fast_pack_bc7_auto_rgba(&mut got, &px, flags);
            assert_eq!(got, want, "{ctx} flags={flags} pixels={pixels:02x?}");
            checked += 1;
        }
    };

    // Real pixel blocks: decode smoke-corpus ASTC blocks (from the UASTC
    // pass-through, all opaque -> RGB pipelines), then alpha-modified
    // variants (RGBA pipelines).
    let mut rng = Rng::new(0xbc7fbc7f);
    {
        use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};
        let mut pool = Vec::<[u8; 64]>::new();
        for path in conformance::smoke_files() {
            let data = std::fs::read(&path).unwrap();
            let Ok(tex) = Transcoder::new(&data) else {
                continue;
            };
            if tex.source_format() != SourceFormat::UastcLdr || tex.is_video() {
                continue;
            }
            let Ok(rgba) = tex.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE) else {
                continue;
            };
            let (w, _h) = tex.base_dimensions();
            // Gather 4x4 pixel blocks from the raster.
            let bw = (w / 4).max(1) as usize;
            for b in 0..(rgba.len() / (16 * 4)).min(4096) {
                let bx = b % bw;
                let by = b / bw;
                let mut blk = [0u8; 64];
                let mut ok = true;
                for y in 0..4 {
                    for x in 0..4 {
                        let px_i = (bx * 4 + x) + (by * 4 + y) * w as usize;
                        let o = px_i * 4;
                        if o + 4 > rgba.len() {
                            ok = false;
                            break;
                        }
                        blk[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].copy_from_slice(&rgba[o..o + 4]);
                    }
                }
                if ok {
                    pool.push(blk);
                }
            }
        }
        assert!(pool.len() > 500, "expected real pixel blocks from smoke");
        for _ in 0..768 {
            let mut blk = pool[rng.below(pool.len() as u32) as usize];
            check("smoke-opaque", &blk);
            // Alpha-modified variant: exercises the RGBA pipelines.
            for i in 0..16 {
                blk[i * 4 + 3] = rng.next_u32() as u8;
            }
            check("smoke-alpha", &blk);
        }
    }

    // Solid blocks (both opaque and translucent).
    for _ in 0..64 {
        let c = [
            rng.next_u32() as u8,
            rng.next_u32() as u8,
            rng.next_u32() as u8,
            rng.next_u32() as u8,
        ];
        let mut blk = [0u8; 64];
        for i in 0..16 {
            blk[i * 4..i * 4 + 4].copy_from_slice(&c);
        }
        check("solid", &blk);
        for i in 0..16 {
            blk[i * 4 + 3] = 255;
        }
        check("solid-opaque", &blk);
    }

    // Random pixels: half fully random (RGBA), half opaque (RGB).
    for case in 0..1024 {
        let mut blk = [0u8; 64];
        for b in blk.iter_mut() {
            *b = rng.next_u32() as u8;
        }
        if case & 1 == 0 {
            for i in 0..16 {
                blk[i * 4 + 3] = 255;
            }
        }
        check("random", &blk);
    }

    assert!(checked > 4000, "too few cases: {checked}");
}

/// Leaf: the `astc_6x6_hdr::fast_encode_bc6h` real-time BC6H encoder must
/// match the oracle bit-for-bit, at both HIGH_QUALITY settings the 6x6
/// transcode path uses (2-subset search off and on), exercised in isolation
/// from the ASTC HDR 6x6 -> BC6H decode target that uses it.
#[test]
fn bc6h_fast_encode_matches_oracle() {
    use basisu::fastenc::bc6h_enc::{fast_encode_bc6h, FastBc6hParams};
    conformance::init();

    let mut checked = 0u64;
    let mut check = |ctx: &str, halves: &[u16; 48]| {
        let mut bytes = [0u8; 96];
        for (i, h) in halves.iter().enumerate() {
            bytes[i * 2..i * 2 + 2].copy_from_slice(&h.to_le_bytes());
        }
        for hq in [false, true] {
            let mut want = [0u8; 16];
            conformance::fast_encode_bc6h(&bytes, hq, &mut want);
            let params = FastBc6hParams {
                max_2subset_pats_to_try: hq as u32,
                ..FastBc6hParams::default()
            };
            let mut got = [0u8; 16];
            fast_encode_bc6h(halves, &mut got, &params);
            assert_eq!(got, want, "{ctx} hq={hq} halves={halves:04x?}");
            checked += 1;
        }
    };

    // The encoder's contract: positive, finite halves <= 0x7BFF (65504.0).
    let clamp_half = |h: u16| -> u16 { (h & 0x7FFF).min(0x7BFF) };

    // Real half-float blocks: transcode the UASTC HDR smoke files to RGB
    // half-float (the byte-proven transcode path) and slice out 4x4 blocks,
    // exactly the pixel source the 6x6 BC6H reblocker feeds the encoder.
    {
        use basisu::{DecodeFlags, SourceFormat, TargetFormat, Transcoder};
        for path in conformance::smoke_files() {
            let data = std::fs::read(&path).unwrap();
            let Ok(tex) = Transcoder::new(&data) else {
                continue;
            };
            if tex.source_format() != SourceFormat::UastcHdr4x4 {
                continue;
            }
            let (w, h) = tex.base_dimensions();
            let out = tex
                .transcode(0, TargetFormat::RgbHalf, DecodeFlags::NONE)
                .unwrap();
            let halves: Vec<u16> = out
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            for by in 0..(h as usize / 4).min(24) {
                for bx in 0..(w as usize / 4).min(24) {
                    let mut blk = [0u16; 48];
                    for y in 0..4 {
                        for x in 0..4 {
                            let p = (by * 4 + y) * w as usize + bx * 4 + x;
                            for c in 0..3 {
                                blk[(y * 4 + x) * 3 + c] = clamp_half(halves[p * 3 + c]);
                            }
                        }
                    }
                    check("smoke-hdr", &blk);
                }
            }
        }
    }

    let mut rng = Rng::new(0xbc6bc6bc6);

    // Solid blocks (mode-13 blog16 path).
    for _ in 0..64 {
        let c = [
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
        ];
        let mut blk = [0u16; 48];
        for i in 0..16 {
            blk[i * 3..i * 3 + 3].copy_from_slice(&c);
        }
        check("solid", &blk);
    }

    // Near-solid / low-variance blocks (the "simple" path with its
    // half-domain projection weight assignment).
    for _ in 0..512 {
        let base = [
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
        ];
        let spread = 1 + rng.below(255);
        let mut blk = [0u16; 48];
        for i in 0..48 {
            let b = base[i % 3] as i32 + rng.below(spread * 2 + 1) as i32 - spread as i32;
            blk[i] = clamp_half(b.clamp(0, 0x7BFF) as u16);
        }
        check("near-solid", &blk);
    }

    // Two-cluster blocks (drives the complex path into the 2-subset search).
    for _ in 0..512 {
        let a = [
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
        ];
        let b = [
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
            clamp_half(rng.next_u32() as u16),
        ];
        let mut blk = [0u16; 48];
        for i in 0..16 {
            let src = if rng.next_u32() & 1 == 0 { &a } else { &b };
            for c in 0..3 {
                let jitter = rng.below(64) as i32 - 32;
                blk[i * 3 + c] = clamp_half((src[c] as i32 + jitter).clamp(0, 0x7BFF) as u16);
            }
        }
        check("two-cluster", &blk);
    }

    // Axis gradients (monotonic dot products in the weight search).
    for case in 0..64 {
        let lo = clamp_half(rng.next_u32() as u16) as i32;
        let hi = clamp_half(rng.next_u32() as u16) as i32;
        let mut blk = [0u16; 48];
        for y in 0..4 {
            for x in 0..4 {
                let t = if case & 1 == 0 { x } else { y } as i32;
                for c in 0..3 {
                    blk[(y * 4 + x) * 3 + c] = (lo + (hi - lo) * t / 3).clamp(0, 0x7BFF) as u16;
                }
            }
        }
        check("gradient", &blk);
    }

    // Fully random halves (extreme variance, exercises the very-complex
    // +/-1 weight refinement and every 2-subset mode fallback).
    for _ in 0..2048 {
        let mut blk = [0u16; 48];
        for h in blk.iter_mut() {
            *h = clamp_half(rng.next_u32() as u16);
        }
        check("random", &blk);
    }

    // Edge magnitudes: denormals, zeros, and near-MAX_HALF_FLOAT values.
    for _ in 0..512 {
        let mut blk = [0u16; 48];
        for h in blk.iter_mut() {
            *h = match rng.below(4) {
                0 => rng.below(64) as u16,          // denormals / tiny
                1 => 0,                             // zero
                2 => 0x7BFF - rng.below(64) as u16, // near max
                _ => clamp_half(rng.next_u32() as u16),
            };
        }
        check("edge-magnitude", &blk);
    }

    assert!(checked > 8000, "too few cases: {checked}");
}

/// Leaf: the UASTC HDR 6x6 intermediate decompressor (`decode_6x6_hdr`) must
/// produce byte-identical physical ASTC blocks and dimensions for every
/// intermediate stream in the corpus, independent of the container paths that
/// use it. Streams are extracted with a minimal KTX2 parse (scheme 4, std
/// 12-byte slice offset/length descs, model-168 DFD), plus `.basis`
/// tex_format-4 slices, plus rejection probes.
#[test]
fn uastc_hdr_6x6_intermediate_matches_oracle() {
    conformance::init();

    let mut streams: Vec<(String, Vec<u8>)> = Vec::new();

    let u32le = |d: &[u8], o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let u64le = |d: &[u8], o: usize| u64::from_le_bytes(d[o..o + 8].try_into().unwrap());

    let mut files = conformance::corpus_files();
    files.extend(conformance::smoke_files());
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(d) = std::fs::read(&path) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "ktx2") {
            if d.len() < 80 || u32le(&d, 44) != 4 {
                continue; // not scheme 4 (KTX2_SS_UASTC_HDR_6x6I)
            }
            let dfd_ofs = u32le(&d, 48) as usize;
            if dfd_ofs + 13 > d.len() || d[dfd_ofs + 12] != 168 {
                continue;
            }
            let layer_count = u32le(&d, 32).max(1);
            let face_count = u32le(&d, 36);
            let level_count = u32le(&d, 40);
            let sgd_ofs = u64le(&d, 64) as usize;
            let sgd_len = u64le(&d, 72) as usize;
            let image_count = (layer_count * face_count * level_count) as usize;
            if sgd_len != image_count * 12 {
                continue; // not the std 12-byte descs this probe understands
            }
            for level in 0..level_count as usize {
                // Level index rows (24 bytes each) start right after the header.
                let lofs = u64le(&d, 80 + level * 24) as usize;
                for img in 0..(layer_count * face_count) as usize {
                    let desc = sgd_ofs + (level * (layer_count * face_count) as usize + img) * 12;
                    let so = u32le(&d, desc) as usize;
                    let sl = u32le(&d, desc + 4) as usize;
                    if lofs + so + sl <= d.len() {
                        streams.push((
                            format!("{name}:L{level}I{img}"),
                            d[lofs + so..lofs + so + sl].to_vec(),
                        ));
                    }
                }
            }
        } else if path.extension().is_some_and(|e| e == "basis") {
            if d.len() < 21 || d[20] != 4 {
                continue; // not tex_format cUASTC_HDR_6x6_INTERMEDIATE
            }
            let total_slices = u32::from_le_bytes([d[14], d[15], d[16], 0]) as usize;
            let slice_desc_ofs = u32le(&d, 65) as usize; // m_slice_desc_file_ofs
            for s in 0..total_slices {
                let o = slice_desc_ofs + s * 23; // sizeof(basis_slice_desc)
                if o + 23 > d.len() {
                    break;
                }
                let file_ofs = u32le(&d, o + 13) as usize;
                let file_size = u32le(&d, o + 17) as usize;
                if file_ofs + file_size <= d.len() {
                    streams.push((
                        format!("{name}:S{s}"),
                        d[file_ofs..file_ofs + file_size].to_vec(),
                    ));
                }
            }
        }
    }

    let mut checked = 0u64;
    let mut total_blocks = 0u64;
    for (ctx, stream) in &streams {
        let want = conformance::decode_6x6_hdr(stream);
        let got = basisu::uastc_hdr_6x6::decode_6x6_hdr(stream);
        match (&want, &got) {
            (Some((wb, ww, wh)), Some((gb, gw, gh))) => {
                assert_eq!((ww, wh), (gw, gh), "{ctx}: dims differ");
                assert_eq!(wb, gb, "{ctx}: decoded blocks differ");
                total_blocks += (wb.len() / 16) as u64;
            }
            (None, None) => {}
            _ => panic!(
                "{ctx}: accept/reject divergence: oracle={} rust={}",
                want.is_some(),
                got.is_some()
            ),
        }
        checked += 1;
    }

    // Rejection probes: truncations and bit flips of a real stream must agree.
    if let Some((_, stream)) = streams.first() {
        let mut rng = Rng::new(0x6666_1177);
        for _ in 0..200 {
            let mut s = stream.clone();
            match rng.below(3) {
                0 => s.truncate((rng.below(s.len() as u32)) as usize),
                1 => {
                    let i = rng.below(s.len() as u32) as usize;
                    s[i] ^= 1 << rng.below(8);
                }
                _ => {
                    let i = rng.below(s.len() as u32) as usize;
                    s.truncate(i.max(8));
                    let j = rng.below(s.len() as u32) as usize;
                    s[j] ^= 0xFF;
                }
            }
            let want = conformance::decode_6x6_hdr(&s);
            let got = basisu::uastc_hdr_6x6::decode_6x6_hdr(&s);
            assert_eq!(
                want.is_some(),
                got.is_some(),
                "mutation accept/reject divergence"
            );
            if let (Some(w), Some(g)) = (&want, &got) {
                assert_eq!(w, g, "mutation decode divergence");
            }
            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "no intermediate streams found (fetch the corpus with `cargo xtask corpus`)"
    );
    eprintln!("uastc_hdr_6x6 leaf: {checked} streams, {total_blocks} blocks byte-identical");
}

/// Leaf: the XUASTC weight-grid inverse DCT must be bit-exact against the
/// oracle for every (rows, cols) size pair and for dense, sparse, and
/// zero-heavy coefficient blocks (the baked coefficient matrices carry
/// per-cell rounding noise, so any recomputation or reordering shows up
/// immediately as f32 bit differences).
#[test]
fn xuastc_idct_matches_oracle() {
    conformance::init();
    let mut rng = Rng::new(0x1dc71dc7);
    let mut checked = 0u64;
    for rows in 2..=12u32 {
        for cols in 2..=12u32 {
            let n = (rows * cols) as usize;
            for case in 0..40 {
                let mut src = vec![0f32; n];
                match case % 4 {
                    // Dense random coefficients in the codec's real range
                    // (dequantized deadzone values, roughly +/- thousands).
                    0 => {
                        for v in src.iter_mut() {
                            *v = (rng.next_u32() as i32 % 4096) as f32 * 0.5;
                        }
                    }
                    // Sparse: a few nonzero entries (typical DCT blocks).
                    1 => {
                        for _ in 0..1 + rng.below(4) {
                            src[rng.below(n as u32) as usize] =
                                (rng.next_u32() as i32 % 2048) as f32;
                        }
                    }
                    // DC only.
                    2 => src[0] = (rng.next_u32() as i32 % 65536) as f32 * 0.25,
                    // All zero.
                    _ => {}
                }
                let mut want = vec![0f32; n];
                conformance::xuastc_idct_2d(&src, rows, cols, &mut want);
                let mut got = vec![0f32; n];
                basisu::xuastc::idct::idct_2d(&src, &mut got, rows as usize, cols as usize);
                for i in 0..n {
                    assert_eq!(
                        want[i].to_bits(),
                        got[i].to_bits(),
                        "{rows}x{cols} case {case} texel {i}: {} vs {}",
                        want[i],
                        got[i]
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 121 * 40);
}

/// Leaf: the XUASTC LDR decompressor must produce byte-identical physical
/// ASTC blocks (each logical block packed through the generic packer, like
/// the reference's pass-through arm) and identical header fields for every
/// stream in the corpus, independent of the container paths. Streams come
/// from a minimal KTX2 parse (scheme 5, model 169) and `.basis` tex_format
/// 5..=18 slices; truncation/bit-flip probes must agree on accept vs reject.
#[test]
fn xuastc_decompress_matches_oracle() {
    conformance::init();

    let mut streams: Vec<(String, Vec<u8>)> = Vec::new();
    let u32le = |d: &[u8], o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let u64le = |d: &[u8], o: usize| u64::from_le_bytes(d[o..o + 8].try_into().unwrap());

    let mut files = conformance::corpus_files();
    files.extend(conformance::smoke_files());
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(d) = std::fs::read(&path) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "ktx2") {
            if d.len() < 100 {
                continue;
            }
            let scheme = u32le(&d, 44);
            let dfd_ofs = u32le(&d, 48) as usize;
            if dfd_ofs + 13 > d.len() || d[dfd_ofs + 12] != 169 {
                continue;
            }
            let layer_count = u32le(&d, 32).max(1);
            let face_count = u32le(&d, 36);
            let level_count = u32le(&d, 40);
            let sgd_ofs = u64le(&d, 64) as usize;
            let sgd_len = u64le(&d, 72) as usize;
            let image_count = (layer_count * face_count * level_count) as usize;
            let desc_size = if scheme == 5 { 12 } else { 8 };
            if sgd_len != image_count * desc_size {
                continue;
            }
            for level in 0..level_count as usize {
                let lofs = u64le(&d, 80 + level * 24) as usize;
                for img in 0..(layer_count * face_count) as usize {
                    let desc =
                        sgd_ofs + (level * (layer_count * face_count) as usize + img) * desc_size;
                    let so = u32le(&d, desc) as usize;
                    let sl = u32le(&d, desc + 4) as usize;
                    if lofs + so + sl <= d.len() {
                        streams.push((
                            format!("{name}:L{level}I{img}"),
                            d[lofs + so..lofs + so + sl].to_vec(),
                        ));
                    }
                }
            }
        } else if path.extension().is_some_and(|e| e == "basis") {
            if d.len() < 21 || !(5..=18).contains(&d[20]) {
                continue;
            }
            let total_slices = u32::from_le_bytes([d[14], d[15], d[16], 0]) as usize;
            let slice_desc_ofs = u32le(&d, 65) as usize;
            for s in 0..total_slices {
                let o = slice_desc_ofs + s * 23;
                if o + 23 > d.len() {
                    break;
                }
                let file_ofs = u32le(&d, o + 13) as usize;
                let file_size = u32le(&d, o + 17) as usize;
                if file_ofs + file_size <= d.len() {
                    streams.push((
                        format!("{name}:S{s}"),
                        d[file_ofs..file_ofs + file_size].to_vec(),
                    ));
                }
            }
        }
    }

    // Both sides pack logical blocks the same way; the Rust init callback
    // applies the oracle hook's capacity policy so accept/reject stays
    // symmetric on corrupt-dimension probes.
    let rust_decompress = |stream: &[u8]| -> Option<conformance::XuastcDecoded> {
        use core::cell::{Cell, RefCell};
        let blocks: RefCell<Vec<u8>> = RefCell::new(Vec::new());
        let nbx = Cell::new(0u32);
        let fields = Cell::new((0u32, 0u32, 0u32, 0u32, false, false));
        let mut init_cb = |info: &basisu::xuastc::decode::XuastcInfo| -> bool {
            let cx = info.width.div_ceil(info.block_width);
            let cy = info.height.div_ceil(info.block_height);
            if (cx as u64) * (cy as u64) * 16 > conformance::XUASTC_LEAF_CAP as u64 {
                return false;
            }
            nbx.set(cx);
            *blocks.borrow_mut() = vec![0u8; (cx * cy) as usize * 16];
            fields.set((
                info.block_width,
                info.block_height,
                info.width,
                info.height,
                info.has_alpha,
                info.srgb,
            ));
            true
        };
        let mut block_cb = |bx: u32, by: u32, log: &basisu::astc::unpack::LogAstcBlock| -> bool {
            match basisu::astc::pack::pack_astc_block(log) {
                Some(phys) => {
                    let o = ((by * nbx.get() + bx) as usize) * 16;
                    blocks.borrow_mut()[o..o + 16].copy_from_slice(&phys);
                    true
                }
                None => false,
            }
        };
        basisu::xuastc::decode::decompress_image(stream, &mut init_cb, &mut block_cb)?;
        Some((blocks.into_inner(), fields.get()))
    };

    let mut checked = 0u64;
    let mut total_blocks = 0u64;
    for (ctx, stream) in &streams {
        let want = conformance::xuastc_decompress(stream);
        let got = rust_decompress(stream);
        match (&want, &got) {
            (Some((wb, wf)), Some((gb, gf))) => {
                assert_eq!(wf, gf, "{ctx}: header fields differ");
                assert_eq!(wb.len(), gb.len(), "{ctx}: block counts differ");
                if wb != gb {
                    let bad = wb
                        .chunks_exact(16)
                        .zip(gb.chunks_exact(16))
                        .position(|(a, b)| a != b)
                        .unwrap();
                    panic!(
                        "{ctx}: first differing block {bad}:\n oracle {:02x?}\n rust   {:02x?}",
                        &wb[bad * 16..bad * 16 + 16],
                        &gb[bad * 16..bad * 16 + 16]
                    );
                }
                total_blocks += (wb.len() / 16) as u64;
            }
            (None, None) => {}
            _ => panic!(
                "{ctx}: accept/reject divergence: oracle={} rust={}",
                want.is_some(),
                got.is_some()
            ),
        }
        checked += 1;
    }

    // Mutation probes on one arith and one zstd stream when available.
    let mut rng = Rng::new(0x8a578a57);
    let mut ruzstd_stricter = 0u32;
    for wanted_syntax in [0u8, 2u8] {
        let Some((_, stream)) = streams
            .iter()
            .find(|(_, s)| s.first() == Some(&wanted_syntax))
        else {
            continue;
        };
        for _ in 0..150 {
            let mut s = stream.clone();
            match rng.below(3) {
                0 => s.truncate(rng.below(s.len() as u32) as usize),
                1 => {
                    let i = rng.below(s.len() as u32) as usize;
                    s[i] ^= 1 << rng.below(8);
                }
                _ => {
                    let i = rng.below(s.len() as u32) as usize;
                    s.truncate(i.max(8));
                    let j = rng.below(s.len() as u32) as usize;
                    s[j] ^= 0xFF;
                }
            }
            let want = conformance::xuastc_decompress(&s);
            let got = rust_decompress(&s);
            // Corrupted zstd payloads are the one sanctioned asymmetry:
            // ruzstd is stricter than libzstd on damaged frames (e.g. a
            // Huffman literals stream one literal short), so on the zstd
            // syntaxes the Rust side may reject streams the oracle still
            // accepts. The reverse (Rust accepting what the oracle
            // rejects) is never allowed, and both-accept must stay
            // byte-identical.
            if wanted_syntax != 0 && want.is_some() && got.is_none() {
                ruzstd_stricter += 1;
                continue;
            }
            assert_eq!(
                want.is_some(),
                got.is_some(),
                "mutation accept/reject divergence (syntax {wanted_syntax})"
            );
            if let (Some(w), Some(g)) = (&want, &got) {
                assert_eq!(w, g, "mutation decode divergence");
            }
            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "no XUASTC streams found (fetch the corpus with `cargo xtask corpus`)"
    );
    eprintln!(
        "xuastc leaf: {checked} cases, {total_blocks} blocks byte-identical, \
         {ruzstd_stricter} corrupt-zstd probes where ruzstd is stricter"
    );
}
