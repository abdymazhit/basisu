#!/usr/bin/env bash
# Fetch the full conformance corpus into corpus/{cts,gltf,binomial}.
#
# The committed corpus/smoke set is enough for `cargo test` and CI's fast path.
# The full corpus (about 220 MB on disk) feeds the exhaustive byte-equality runs.
#
# License-clean sources, pinned to fixed commits so the corpus is reproducible:
#   cts:      KhronosGroup/KTX-Software-CTS .ktx2 assets (Apache-2.0)
#   gltf:     KhronosGroup/glTF-Sample-Assets KTX/BasisU textures
#   binomial: BinomialLLC/basis_universal test files (Apache-2.0)
#
# Each source is fetched with a blob-filtered, sparse, single-commit clone so
# only the Basis container files are downloaded, then the matching files are
# copied out (directory structure preserved) and the clone is removed. Re-runs
# skip sources whose destination directory already exists; delete a directory
# to re-fetch it.
#
# If you already have a corpus checkout elsewhere, set CORPUS_DIR to its path
# and this links it in as corpus/cts instead of downloading anything.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
existing="${CORPUS_DIR:-}"

if [[ -n "$existing" && -d "$existing" ]]; then
  echo "Linking existing corpus from $existing"
  ln -sfn "$existing" "$here/cts"
  echo "Linked corpus/cts -> $existing"
  exit 0
fi

# Every temp clone lives under one root so a single EXIT trap cleans up even
# when set -e aborts mid-fetch.
tmproot="$(mktemp -d "$here/.fetch.XXXXXX")"
trap 'rm -rf "$tmproot" "$here"/.fetch-*' EXIT

# fetch_source <dest-subdir> <github-repo> <commit> <pattern...>
# Sparse blob-filtered clone of one pinned commit, then copy every file that
# matches one of the gitignore-style patterns into corpus/<dest-subdir>.
fetch_source() {
  local dest="$1" repo="$2" commit="$3"
  shift 3
  # A dangling symlink (a stale CORPUS_DIR link) blocks mkdir but fails -e;
  # drop it and re-fetch.
  if [[ -L "$here/$dest" && ! -e "$here/$dest" ]]; then
    echo "corpus/$dest is a dangling symlink, removing it"
    rm "$here/$dest"
  fi
  if [[ -e "$here/$dest" ]]; then
    echo "corpus/$dest already exists, skipping (delete it to re-fetch)"
    return 0
  fi
  local tmp="$tmproot/$dest"
  echo "Fetching $repo@${commit:0:12} -> corpus/$dest"
  git clone --quiet --no-checkout --filter=blob:none \
    "https://github.com/$repo.git" "$tmp/repo"
  git -C "$tmp/repo" sparse-checkout set --no-cone "$@"
  git -C "$tmp/repo" checkout --quiet "$commit"
  mkdir -p "$here/$dest"
  local n=0 f rel
  while IFS= read -r -d '' f; do
    rel="${f#"$tmp/repo/"}"
    mkdir -p "$here/$dest/$(dirname "$rel")"
    cp "$f" "$here/$dest/$rel"
    n=$((n + 1))
  done < <(find "$tmp/repo" -type f \( -name '*.ktx2' -o -name '*.basis' \) -print0)
  rm -rf "$tmp"
  echo "corpus/$dest: $n files"
}

fetch_source cts KhronosGroup/KTX-Software-CTS \
  030ad7c045581b6221f81f970810d1b0e7165dbc '*.ktx2' '*.basis'

fetch_source gltf KhronosGroup/glTF-Sample-Assets \
  2bac6f8c57bf471df0d2a1e8a8ec023c7801dddf '*.ktx2'

fetch_source binomial BinomialLLC/basis_universal \
  20ed781c4b8d98b36074019a3389d4f71527a4d9 '*.ktx2' '*.basis'

echo "Corpus fetch complete."
