#!/usr/bin/env bash
# Re-vendor the Basis Universal transcoder at a given upstream git ref.
#
#   tools/vendor.sh <git-ref>        # e.g. v1.50, or a commit SHA
#
# Copies the transcoder-only file set (no encoder / CLI / bundled zstd) verbatim
# into conformance/csrc/basisu_transcoder/, leaving a reviewable git diff. Then:
#   1) edit conformance/csrc/basisu_transcoder/UPSTREAM_VERSION (pin the new ref)
#   2) cargo xtask verify        # the oracle localizes any output change to
#                                # exactly the (format, file) rows that diverged
#   3) re-port the Rust to match those rows; repeat until green
#   4) cargo xtask bake-goldens  # refresh the committed pure-Rust goldens
set -euo pipefail
ref="${1:?usage: tools/vendor.sh <git-ref>}"
here="$(cd "$(dirname "$0")/.." && pwd)"
dst="$here/conformance/csrc/basisu_transcoder"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Cloning BinomialLLC/basis_universal @ $ref ..."
if ! git clone --depth 1 --branch "$ref" \
       https://github.com/BinomialLLC/basis_universal "$tmp" 2>/dev/null; then
  git clone https://github.com/BinomialLLC/basis_universal "$tmp"
  git -C "$tmp" checkout "$ref"
fi

# Copy every transcoder source file, so this stays correct when upstream adds
# files (e.g. v2's HDR core / astc helpers / idct / *.inl). basisu_transcoder.cpp
# #includes the rest, so we still compile only that one .cpp; we just need every
# source present. Encoder, CLI, and bundled zstd live in other top-level dirs
# and are not copied.
shopt -s nullglob
src="$tmp/transcoder"
[[ -d "$src" ]] || { echo "ERROR: $ref has no transcoder/ dir" >&2; exit 1; }
# Drop our previous source files (keep UPSTREAM_VERSION + any docs we added).
find "$dst" -maxdepth 1 -type f \
  \( -name '*.h' -o -name '*.cpp' -o -name '*.inl' -o -name '*.inc' -o -name 'LICENSE' \) \
  -delete
for f in "$src"/*.h "$src"/*.cpp "$src"/*.inl "$src"/*.inc; do
  cp "$f" "$dst/"
done
cp "$tmp/LICENSE" "$dst/" 2>/dev/null || true
echo "Copied $(ls "$src"/*.h "$src"/*.cpp "$src"/*.inl "$src"/*.inc 2>/dev/null | wc -l | tr -d ' ') transcoder source files."

echo "Done. Now: edit UPSTREAM_VERSION, then 'cargo xtask verify' (review failures),"
echo "re-port until green, then 'cargo xtask bake-goldens'."
