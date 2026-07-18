#!/usr/bin/env python3
"""Mechanically transcribe the UASTC HDR 6x6 intermediate-codec tables from the
vendored basisu_transcoder.cpp (namespace astc_6x6_hdr) into a committed Rust
module, so the bulk data is never hand-typed:

  - g_block_mode_descs[75]           (block_mode_desc struct rows)
  - g_reuse_xy_deltas[32]            (int8 x/y pairs)
  - g_part2_unique_index_to_seed[521]
  - g_part3_unique_index_to_seed[333]

BISE_*_LEVELS symbols are resolved to their numeric ISE range indices, and the
BASIST_HDR_6X6_LEVEL* flag ORs to their numeric values (the flags field is
encoder-side detail, kept for fidelity). Preserves curated docs on
regeneration the same way gen_table.py does: everything before the first
generated `pub static` is kept if the output file already exists.

Usage: gen_hdr6x6_tables.py [--cpp <basisu_transcoder.cpp>] [--out <tables.rs>]
Throwaway dev tooling; the generated Rust is what ships.
"""

import argparse
import re
import sys
from pathlib import Path

BISE = {f"BISE_{n}_LEVELS": i for i, n in enumerate(
    [2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64, 80, 96, 128, 160, 192, 256])}
FLAGS = {"BASIST_HDR_6X6_LEVEL0": 1, "BASIST_HDR_6X6_LEVEL1": 2, "BASIST_HDR_6X6_LEVEL2": 4}


def extract_braces(text, name):
    """Return the text between the outermost braces of `name = { ... };`."""
    m = re.search(re.escape(name) + r"[^=]*=\s*\{", text)
    if not m:
        sys.exit(f"table {name} not found")
    depth, start = 1, m.end()
    i = start
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


def parse_rows(body):
    """Split a brace-initialized struct table into rows of fields."""
    rows = []
    depth = 0
    cur = None
    for c in body:
        if c == "{":
            depth += 1
            if depth == 1:
                cur = ""
                continue
        if c == "}":
            depth -= 1
            if depth == 0:
                rows.append([f.strip() for f in cur.split(",")])
                cur = None
                continue
        if cur is not None:
            cur += c
    return rows


def resolve(tok):
    tok = tok.replace("astc_helpers::", "").strip()
    if tok in ("true", "false"):
        return tok
    if tok in BISE:
        return str(BISE[tok])
    if "|" in tok:
        return str(sum(FLAGS[t.strip()] for t in tok.split("|")))
    if tok in FLAGS:
        return str(FLAGS[tok])
    return str(int(tok, 0))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cpp", default="conformance/csrc/basisu_transcoder/basisu_transcoder.cpp")
    ap.add_argument("--out", default="basisu/src/uastc_hdr_6x6/tables.rs")
    args = ap.parse_args()

    text = Path(args.cpp).read_text(errors="replace")

    descs = parse_rows(strip_comments(extract_braces(text, "g_block_mode_descs")))
    assert len(descs) == 75, len(descs)
    deltas = parse_rows(strip_comments(extract_braces(text, "g_reuse_xy_deltas")))
    assert len(deltas) == 32, len(deltas)
    part2 = [int(v) for v in strip_comments(extract_braces(text, "g_part2_unique_index_to_seed")).replace("\n", " ").split(",") if v.strip()]
    assert len(part2) == 521, len(part2)
    part3 = [int(v) for v in strip_comments(extract_braces(text, "g_part3_unique_index_to_seed")).replace("\n", " ").split(",") if v.strip()]
    assert len(part3) == 333, len(part3)

    lines = []
    lines.append("pub static BLOCK_MODE_DESCS: [BlockModeDesc; 75] = [")
    for r in descs:
        dp, cem, parts, gx, gy, eir, wir, teir, twir, flags, dpch = [resolve(f) for f in r]
        lines.append(
            f"    BlockModeDesc {{ dp: {dp}, cem: {cem}, num_partitions: {parts}, grid_x: {gx}, grid_y: {gy}, "
            f"endpoint_ise_range: {eir}, weight_ise_range: {wir}, transcode_endpoint_ise_range: {teir}, "
            f"transcode_weight_ise_range: {twir}, flags: {flags}, dp_channel: {dpch} }},"
        )
    lines.append("];")
    lines.append("")
    lines.append("pub static REUSE_XY_DELTAS: [(i8, i8); 32] = [")
    for r in deltas:
        lines.append(f"    ({int(r[0])}, {int(r[1])}),")
    lines.append("];")
    lines.append("")

    def emit_flat(name, vals, ty, per_row=16):
        lines.append(f"pub static {name}: [{ty}; {len(vals)}] = [")
        for i in range(0, len(vals), per_row):
            lines.append("    " + " ".join(f"{v}," for v in vals[i : i + per_row]))
        lines.append("];")
        lines.append("")

    emit_flat("PART2_UNIQUE_INDEX_TO_SEED", part2, "u16")
    emit_flat("PART3_UNIQUE_INDEX_TO_SEED", part3, "u16")

    out = Path(args.out)
    generated = "\n".join(lines).rstrip() + "\n"
    if out.exists():
        existing = out.read_text()
        idx = existing.find("pub static BLOCK_MODE_DESCS")
        if idx >= 0:
            out.write_text(existing[:idx] + generated)
            print(f"regenerated {out} (docs preserved)")
            return
    header = (
        "// Generated by tools/gen_hdr6x6_tables.py from the vendored\n"
        "// basisu_transcoder.cpp (namespace astc_6x6_hdr). Do not edit the tables by\n"
        "// hand; regenerate from that source instead.\n\n"
        "/// `block_mode_desc`: one row per coded block mode. ISE ranges are the\n"
        "/// standard 0..=20 indices; `flags` is the encoder-side level mask (kept\n"
        "/// for fidelity, unused by the decoder); `dp_channel` is the dual-plane\n"
        "/// color component selector.\n"
        "#[derive(Clone, Copy)]\n"
        "pub struct BlockModeDesc {\n"
        "    pub dp: bool,\n"
        "    pub cem: u32,\n"
        "    pub num_partitions: u32,\n"
        "    pub grid_x: u32,\n"
        "    pub grid_y: u32,\n"
        "    pub endpoint_ise_range: u32,\n"
        "    pub weight_ise_range: u32,\n"
        "    pub transcode_endpoint_ise_range: u32,\n"
        "    pub transcode_weight_ise_range: u32,\n"
        "    #[allow(dead_code)]\n"
        "    pub flags: u32,\n"
        "    pub dp_channel: u32,\n"
        "}\n\n"
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(header + generated)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
