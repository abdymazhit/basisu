#!/usr/bin/env python3
"""Mechanically transcribe a vendored `.inc` lookup table into a committed Rust
const, so the bulk data tables are never hand-typed. The brace-initialized
`.inc` is just comma-separated integer literals; we strip any comment header,
flatten the integers, group into struct rows, and emit a Rust const array whose
in-memory layout matches the C++ struct (verified byte-for-byte by the difftest
gate afterwards).

Two struct modes:
  --def           emit the struct definition from --fields (standalone table)
  --use <path>    emit `use <path>;` and reference the shared struct (no def)

Usage:
  gen_table.py --inc <f.inc> --const NAME --struct StructName \
               --fields m_lo:u8,m_hi:u8,m_err:u16 --count 15360 \
               --use super::solution::Etc1ToSolution --out <f.rs>

Throwaway dev tooling; the generated Rust is what ships.
"""
import argparse
import re
import sys


def strip_comments(text: str) -> str:
    """Remove // line comments and /* */ block comments. Some .inc files carry
    an Apache license header whose digits (years, '2.0') would otherwise be
    parsed as table data."""
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    text = re.sub(r"//[^\n]*", "", text)
    return text


def merge_existing(out_path, const_name, default_header, static_line, data_lines):
    """Assemble the output, preserving curated documentation on regeneration.

    The committed generated files carry hand-written module docs, struct and
    field docs, and rustdoc on the static itself. When the output file already
    exists and contains the target static, everything before that static
    (including its rustdoc) and everything after the closing bracket is kept
    verbatim; only the static declaration line and the data rows are replaced.
    A fresh file gets the minimal default header instead."""
    try:
        with open(out_path, "r") as f:
            old = f.read().splitlines()
    except FileNotFoundError:
        old = None
    if old is not None:
        idx = next(
            (i for i, l in enumerate(old) if l.startswith(f"pub static {const_name}")),
            None,
        )
        if idx is not None:
            end = next(
                (i for i in range(idx, len(old)) if old[i].strip() == "];"), None
            )
            if end is not None:
                tail = old[end + 1 :] or [""]
                return old[:idx] + [static_line] + data_lines + ["];"] + tail
    return default_header + [static_line] + data_lines + ["];", ""]


def main() -> int:
    """Transcribe one table per the CLI flags. Returns the process exit code:
    1 if the .inc did not yield exactly count * nfields integers (a wrong
    --count or a malformed input), 0 once the .rs file is written."""
    ap = argparse.ArgumentParser()
    ap.add_argument("--inc", required=True)
    ap.add_argument("--const", required=True, dest="const_name")
    ap.add_argument("--struct", required=True)
    ap.add_argument("--fields", required=True, help="name:type,name:type,...")
    ap.add_argument("--count", type=int, required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--def", action="store_true", dest="emit_def",
                    help="emit the struct definition")
    ap.add_argument("--use", dest="use_path", default=None,
                    help="emit `use <path>;` instead of a definition")
    args = ap.parse_args()

    fields = [f.split(":") for f in args.fields.split(",")]
    nfields = len(fields)

    with open(args.inc, "r") as f:
        text = strip_comments(f.read())
    nums = [int(x) for x in re.findall(r"-?\d+", text)]

    expected = args.count * nfields
    if len(nums) != expected:
        print(
            f"ERROR: parsed {len(nums)} ints, expected {expected} "
            f"({args.count} rows x {nfields} fields)",
            file=sys.stderr,
        )
        return 1

    header = []
    header.append(f"// Generated from the upstream {args.inc.split('/')[-1]}. Do not edit by")
    header.append("// hand; regenerate from that source instead.")
    header.append("")
    if args.emit_def:
        header.append("#[repr(C)]")
        header.append("#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]")
        header.append(f"pub struct {args.struct} {{")
        for name, ty in fields:
            header.append(f"    pub {name}: {ty},")
        header.append("}")
        header.append("")
    elif args.use_path:
        header.append(f"use {args.use_path};")
        header.append("")

    static_line = f"pub static {args.const_name}: [{args.struct}; {args.count}] = ["
    data = []
    for row in range(args.count):
        vals = nums[row * nfields : (row + 1) * nfields]
        parts = ", ".join(f"{name}: {v}" for (name, _ty), v in zip(fields, vals))
        data.append(f"    {args.struct} {{ {parts} }},")

    out = merge_existing(args.out, args.const_name, header, static_line, data)
    with open(args.out, "w") as f:
        f.write("\n".join(out))
    print(f"wrote {args.out}: {args.count} rows x {nfields} fields")
    return 0


if __name__ == "__main__":
    sys.exit(main())
