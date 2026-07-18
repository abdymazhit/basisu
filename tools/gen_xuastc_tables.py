#!/usr/bin/env python3
"""Mechanically transcribe the XUASTC LDR tables from the vendored sources
into committed Rust modules, so the bulk data is never hand-typed:

From basisu_astc_cfgs.inl:
  - BU_TOTAL_ASTC_CFGS (10311) packed 24-bit trial-mode configs.
From basisu_transcoder.cpp (namespace astc_ldr_t):
  - g_total_unique_patterns[14][2]
  - the 28 g_unique_to_seed_<bs>_p{2,3} u16 arrays (+ dispatch order)
  - g_baseline_jpeg_y[8][8]
  - g_scale_quant_steps[12]
From basisu_idct.h:
  - the 11 baked f32 IDCT coefficient matrices (sizes 2..12), extracted
    verbatim per (k, x) slot (the literals carry per-cell float rounding
    noise and must NOT be recomputed or deduplicated). Structural zeros
    are emitted as 0.0 (adding a zero product is exact, so a generic
    ascending-(k, x) accumulation loop over the full matrix reproduces
    the reference's sum order bit-for-bit).

Preserves curated docs on regeneration like the other generators: content
before the first generated item is kept when the output exists.

Usage: gen_xuastc_tables.py [--src <vendored dir>] [--out-dir basisu/src/xuastc]
Throwaway dev tooling; the generated Rust is what ships.
"""

import argparse
import re
import sys
from pathlib import Path

BLOCK_SIZES = ["4x4", "5x4", "5x5", "6x5", "6x6", "8x5", "8x6", "10x5",
               "10x6", "8x8", "10x8", "10x10", "12x10", "12x12"]


def extract_braces(text, name):
    m = re.search(re.escape(name) + r"[^=]*=\s*\{", text)
    if not m:
        sys.exit(f"table {name} not found")
    depth, i = 1, m.end()
    start = m.end()
    while depth:
        c = text[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        i += 1
    return text[start : i - 1]


def strip_comments(s):
    s = re.sub(r"/\*.*?\*/", "", s, flags=re.DOTALL)
    return re.sub(r"//[^\n]*", "", s)


def ints(body):
    flat = strip_comments(body).replace("{", " ").replace("}", " ").replace("\n", " ")
    return [int(v) for v in flat.split(",") if v.strip()]


def emit_flat(lines, name, vals, ty, per_row=16):
    lines.append(f"pub static {name}: [{ty}; {len(vals)}] = [")
    for i in range(0, len(vals), per_row):
        lines.append("    " + " ".join(f"{v}," for v in vals[i : i + per_row]))
    lines.append("];")
    lines.append("")


def write_preserving(out, generated, header, anchor):
    if out.exists():
        existing = out.read_text()
        idx = existing.find(anchor)
        if idx >= 0:
            out.write_text(existing[:idx] + generated)
            print(f"regenerated {out} (docs preserved)")
            return
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(header + generated)
    print(f"wrote {out}")


def gen_tables(src_dir: Path, out_dir: Path):
    cfgs = (src_dir / "basisu_astc_cfgs.inl").read_text(errors="replace")
    total = int(re.search(r"BU_TOTAL_ASTC_CFGS = (\d+);", cfgs).group(1))
    cfg_bytes = ints(extract_braces(cfgs, "s_astc_cfg_table"))
    assert len(cfg_bytes) == total * 3, len(cfg_bytes)
    packed = [cfg_bytes[i * 3] | (cfg_bytes[i * 3 + 1] << 8) | (cfg_bytes[i * 3 + 2] << 16) for i in range(total)]

    tc = (src_dir / "basisu_transcoder.cpp").read_text(errors="replace")

    totals = ints(extract_braces(tc, "g_total_unique_patterns"))
    assert len(totals) == 28

    lines = []
    emit_flat(lines, "ASTC_CFG_TABLE", packed, "u32", per_row=12)
    lines.append("pub static TOTAL_UNIQUE_PATTERNS: [[u16; 2]; 14] = [")
    for i in range(14):
        lines.append(f"    [{totals[i * 2]}, {totals[i * 2 + 1]}],")
    lines.append("];")
    lines.append("")

    for p in (2, 3):
        for i, bs in enumerate(BLOCK_SIZES):
            vals = ints(extract_braces(tc, f"g_unique_to_seed_{bs}_p{p}"))
            want = totals[i * 2 + (p - 2)]
            assert len(vals) == want, f"{bs} p{p}: {len(vals)} != {want}"
            emit_flat(lines, f"UNIQUE_TO_SEED_{bs.upper()}_P{p}", vals, "u16")

    names2 = ", ".join(f"&UNIQUE_TO_SEED_{bs.upper()}_P2" for bs in BLOCK_SIZES)
    names3 = ", ".join(f"&UNIQUE_TO_SEED_{bs.upper()}_P3" for bs in BLOCK_SIZES)
    lines.append("pub static UNIQUE_INDEX_TO_PART_SEED: [[&[u16]; 14]; 2] = [")
    lines.append(f"    [{names2}],")
    lines.append(f"    [{names3}],")
    lines.append("];")
    lines.append("")

    jpeg = ints(extract_braces(tc, "g_baseline_jpeg_y"))
    assert len(jpeg) == 64
    lines.append("pub static BASELINE_JPEG_Y: [[i32; 8]; 8] = [")
    for r in range(8):
        lines.append("    [" + ", ".join(str(v) for v in jpeg[r * 8 : r * 8 + 8]) + "],")
    lines.append("];")
    lines.append("")

    steps = [v.strip().rstrip("f") for v in strip_comments(extract_braces(tc, "g_scale_quant_steps")).split(",") if v.strip()]
    assert len(steps) == 12
    lines.append("// Verbatim reference literals (results of scale_quant_steps()).")
    lines.append("#[allow(clippy::excessive_precision)]")
    lines.append("pub static SCALE_QUANT_STEPS: [f32; 12] = [")
    lines.append("    " + " ".join(f"{v}," for v in steps))
    lines.append("];")
    lines.append("")

    header = (
        "// Generated by tools/gen_xuastc_tables.py from the vendored\n"
        "// basisu_astc_cfgs.inl and basisu_transcoder.cpp (namespace astc_ldr_t).\n"
        "// Do not edit the tables by hand; regenerate from those sources instead.\n\n"
        "/// The 10311 packed 24-bit trial-mode configs (`s_astc_cfg_table`),\n"
        "/// pre-combined from the 3-byte little-endian rows. Field order\n"
        "/// LSB-first: endpoint_ise_range(5) weight_ise_range(4) ccs_index(3)\n"
        "/// num_subsets(2) unique_cem_index(3) grid_wh(7).\n"
    )
    write_preserving(out_dir / "tables.rs", "\n".join(lines).rstrip() + "\n", header, "pub static ASTC_CFG_TABLE")


def gen_idct(src_dir: Path, out_dir: Path):
    text = (src_dir / "basisu_idct.h").read_text(errors="replace")
    lines = []
    for n in range(2, 13):
        m = re.search(rf"idct_1d_{n}\(.*?\)\s*\{{", text, re.DOTALL)
        assert m, n
        # function body: brace matching from m.end()-1
        depth, i = 1, m.end()
        start = m.end()
        while depth:
            c = text[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
            i += 1
        body = text[start : i - 1]
        # next function must not bleed in
        coeffs = [[None] * n for _ in range(n)]
        k = None
        for line in body.splitlines():
            mv = re.search(r"float v = src\[(\d+)?\s*\*?\s*src_stride\]", line.replace("0 * src_stride", "0*src_stride"))
            if "float v = src[" in line:
                mk = re.search(r"src\[(?:(\d+)\s*\*\s*)?src_stride", line)
                if "src[src_stride]" in line.replace(" ", ""):
                    k = 1
                elif mk and mk.group(1) is not None:
                    k = int(mk.group(1))
                elif re.search(r"src\[0\]", line):
                    k = 0
                else:
                    sys.exit(f"idct_1d_{n}: unparsed v line: {line!r}")
                continue
            ma = re.search(r"s(\d+)\s*\+=\s*(-?\d\.\d+e[-+]\d+)f\s*\*\s*v", line)
            if ma:
                assert k is not None
                x = int(ma.group(1))
                assert coeffs[k][x] is None, (n, k, x)
                coeffs[k][x] = ma.group(2)
        flat = []
        for kk in range(n):
            for x in range(n):
                flat.append(coeffs[kk][x] if coeffs[kk][x] is not None else "0.0")
        lines.append(f"pub static IDCT_COEFFS_{n}: [f32; {n * n}] = [")
        for i2 in range(0, len(flat), 6):
            lines.append("    " + " ".join(f"{v}," for v in flat[i2 : i2 + 6]))
        lines.append("];")
        lines.append("")
    refs = ", ".join(f"&IDCT_COEFFS_{n}" for n in range(2, 13))
    lines.append("/// Coefficient matrix per DCT size N (index N - 2), row-major [k][x].")
    lines.append(f"pub static IDCT_COEFFS: [&[f32]; 11] = [{refs}];")
    lines.append("")
    header = (
        "// Generated by tools/gen_xuastc_tables.py from the vendored\n"
        "// basisu_idct.h. Do not edit by hand; regenerate instead.\n"
        "//\n"
        "// The literals keep the reference's full printed precision (clippy's\n"
        "// excessive_precision lint would round-trip them identically, but the\n"
        "// verbatim text is the point of the extraction).\n"
        "#![allow(clippy::excessive_precision)]\n\n"
        "// Each matrix holds the baked single-precision DCT-III coefficients\n"
        "// C[k][x] exactly as the reference's inline literals (they carry\n"
        "// per-cell rounding noise and are NOT recomputable); structural zeros\n"
        "// (terms the reference omits) are stored as 0.0, which is exact under\n"
        "// f32 accumulation.\n\n"
    )
    write_preserving(out_dir / "idct_tables.rs", "\n".join(lines).rstrip() + "\n", header, "pub static IDCT_COEFFS_2")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", default="conformance/csrc/basisu_transcoder")
    ap.add_argument("--out-dir", default="basisu/src/xuastc")
    args = ap.parse_args()
    src, out = Path(args.src), Path(args.out_dir)
    gen_tables(src, out)
    gen_idct(src, out)


if __name__ == "__main__":
    main()
