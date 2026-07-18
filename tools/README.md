# Table generators

Mechanically transcribe vendored lookup tables (`.inc` and `.inl` files, and
the brace-initialized tables inside the C++ sources) into committed Rust
`const` arrays whose in-memory layout matches the C++ struct, so bulk data is
never hand-typed and re-vendoring a changed table upstream is a regen step,
not manual transcription.

The generic transcribers, pointed at one table at a time:

- `gen_table.py`: a 1-D `.inc` of comma-separated ints to a Rust struct array.
- `gen_flat2d.py`: a 2-D brace-nested table (per-row braces, zero-padded) to a
  Rust `[[T; INNER]; OUTER]`.

The codec-specific transcribers, which pull a whole related set at once:

- `gen_hdr6x6_tables.py`: the UASTC HDR 6x6 intermediate-codec tables from
  `basisu_transcoder.cpp` (namespace `astc_6x6_hdr`): the block-mode
  descriptors, the reuse x/y deltas, and the two partition seed tables.
  `BISE_*_LEVELS` symbols resolve to their numeric ISE range indices and the
  `BASIST_HDR_6X6_LEVEL*` flags to their numeric values.
- `gen_xuastc_tables.py`: the XUASTC LDR tables, from three vendored files: the
  packed 24-bit trial-mode configs in `basisu_astc_cfgs.inl`, the pattern,
  seed, JPEG-Y and scale-quant tables in `basisu_transcoder.cpp` (namespace
  `astc_ldr_t`), and the baked f32 IDCT matrices in `basisu_idct.h`. The IDCT
  literals are copied slot by slot rather than recomputed: each cell carries
  its own float rounding noise, so recomputing or deduplicating them would
  break bit-exactness.

Every generator preserves curated documentation on regeneration: when the
output file already exists, everything before the first generated item
(module docs, struct and field docs, the item's own rustdoc) is kept
verbatim and only the data rows are rewritten. Run `cargo fmt` after
regenerating; the committed files are rustfmt-formatted, so a regeneration
followed by `cargo fmt` of an unchanged table is a byte-for-byte no-op.

Every generated table is verified byte-for-byte against the C++ oracle's
in-memory tables by the conformance suite, so a generation mistake fails the
gate.

- `vendor.sh`: re-vendors the upstream transcoder source at a given git ref
  into `conformance/csrc/basisu_transcoder/` (usually run as
  `cargo xtask vendor <ref>`).
