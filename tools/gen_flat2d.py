#!/usr/bin/env python3
"""Extract an inline flat 2D array definition (`const T name[OUTER][INNER] =
{...};`) from the vendored `.cpp` and emit it as a Rust `[[ty; INNER]; OUTER]`
const. For the UASTC pattern/anchor tables, which live inline in
basisu_transcoder.cpp rather than in a `.inc`. Comment-stripped; the difftest
gate verifies the bytes.

Usage:
  gen_flat2d.py --cpp <file> --symbol g_astc_bc7_patterns2 --outer 30 \
                --inner 16 --type u8 --const ASTC_BC7_PATTERNS2 --out <f.rs>
"""
import argparse
import re
import sys


def strip_comments(text: str) -> str:
    """Drop /* */ and // comments so the numeric parse below never picks up
    values that only appear in commented-out rows."""
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    text = re.sub(r"//[^\n]*", "", text)
    return text


def merge_existing(out_path, const_name, default_header, static_line, data_lines):
    """Assemble the output, preserving curated documentation on regeneration.

    The committed generated files carry hand-written module and item rustdoc.
    When the output file already exists and contains the target static,
    everything before that static (including its rustdoc) and everything after
    the closing bracket is kept verbatim; only the static declaration line and
    the data rows are replaced. A fresh file gets the minimal default header."""
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
    """Locate the symbol's initializer in the C++ source, parse its rows, and
    write the Rust const. Returns a process exit code (non-zero on any
    shape mismatch, since a silently wrong table would defeat the difftest)."""
    ap = argparse.ArgumentParser()
    ap.add_argument("--cpp", required=True)
    ap.add_argument("--symbol", required=True)
    ap.add_argument("--outer", type=int, required=True)
    ap.add_argument("--inner", type=int, required=True)
    ap.add_argument("--type", default="u8")
    ap.add_argument("--const", required=True, dest="const_name")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    text = open(args.cpp).read()
    # Locate the definition: `<symbol> [dims] = {`. The dims contain no '='.
    m = re.search(re.escape(args.symbol) + r"\s*\[[^=]*=\s*", text)
    if not m:
        print(f"ERROR: definition of {args.symbol} not found", file=sys.stderr)
        return 1
    rest = text[m.end():]
    open_brace = rest.index("{")
    close = rest.index("};", open_brace)
    body = strip_comments(rest[open_brace + 1 : close])

    # Parse per-row inner braces (innermost, no nesting). C++ aggregate
    # initialization zero-fills missing trailing elements, so a row may have
    # fewer than `inner` values; we zero-pad to match the in-memory layout.
    raw_rows = re.findall(r"\{([^{}]*)\}", body)
    if len(raw_rows) != args.outer:
        print(
            f"ERROR: parsed {len(raw_rows)} rows, expected {args.outer}",
            file=sys.stderr,
        )
        return 1
    rows = []
    for r in raw_rows:
        vals = [int(x) for x in re.findall(r"-?\d+", r)]
        if len(vals) > args.inner:
            print(f"ERROR: row has {len(vals)} > inner {args.inner}", file=sys.stderr)
            return 1
        vals += [0] * (args.inner - len(vals))  # zero-fill like C++
        rows.append(vals)

    header = [
        f"// Generated from the upstream basisu_transcoder.cpp ({args.symbol}). Do not edit",
        "// by hand; regenerate from that source instead.",
        "",
    ]
    static_line = (
        f"pub static {args.const_name}: [[{args.type}; {args.inner}]; {args.outer}] = ["
    )
    data = ["    [" + ", ".join(str(v) for v in row) + "]," for row in rows]

    out = merge_existing(args.out, args.const_name, header, static_line, data)
    open(args.out, "w").write("\n".join(out))
    print(f"wrote {args.out}: {args.outer}x{args.inner} {args.type}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
