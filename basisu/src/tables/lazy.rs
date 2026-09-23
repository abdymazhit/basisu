//! The ETC1S single-color solution tables behind the ASTC and BC targets: five
//! tables of 15 360 four-byte entries, the bulk of the crate's static data. With
//! the `embedded-tables` feature they are the `static`s of the sibling modules;
//! without it a consumer downloads a family's bundle and installs it once with
//! [`install`], and every reader takes the table from the slot filled here.
//! Until a family is installed its targets are unsupported for ETC1S sources
//! (`Transcoder::supports`), so a caller falls back instead of failing.
//!
//! Bundle layout (little-endian): `b"BSU1"`, family id, table count, two
//! reserved bytes, then the tables in family order, each entry `m_lo`, `m_hi`,
//! `m_err` (u16 LE). The families and their table order are part of the
//! format: ASTC = `[0, 47]` range, `[0, 255]` range; BC = BC7 mode-5 color,
//! DXT 5-bit, DXT 6-bit.
use super::solution::Etc1ToSolution;
use crate::api::{SourceFormat, TargetFormat};
use crate::once::OnceBox;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Entries per solution table.
pub const SOLUTION_LEN: usize = 15360;
/// One solution table.
pub type Solutions = [Etc1ToSolution; SOLUTION_LEN];
const ENTRY_BYTES: usize = 4;
const TABLE_BYTES: usize = SOLUTION_LEN * ENTRY_BYTES;
const HEADER_BYTES: usize = 8;
const MAGIC: [u8; 4] = *b"BSU1";

/// A family of tables, downloaded and installed together: the set one GPU
/// target needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableFamily {
    /// ETC1S to ASTC 4x4: the `[0, 47]` and `[0, 255]` range tables.
    Astc,
    /// ETC1S to BC1/BC3/BC7: BC7 mode-5 color, DXT 5-bit and DXT 6-bit.
    Bc,
}

impl TableFamily {
    pub const ALL: [TableFamily; 2] = [TableFamily::Astc, TableFamily::Bc];

    #[cfg(feature = "embedded-tables")]
    fn id(self) -> u8 {
        match self {
            TableFamily::Astc => 0,
            TableFamily::Bc => 1,
        }
    }

    fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(TableFamily::Astc),
            1 => Some(TableFamily::Bc),
            _ => None,
        }
    }

    /// Tables in the family, in bundle order.
    pub fn table_count(self) -> usize {
        self.slots().len()
    }

    /// Exact byte length of the family's bundle.
    pub fn bundle_len(self) -> usize {
        HEADER_BYTES + self.table_count() * TABLE_BYTES
    }

    /// The family an ETC1S transcode to `target` reads from, if any.
    pub fn for_target(target: TargetFormat) -> Option<Self> {
        match target {
            TargetFormat::Astc4x4Rgba => Some(TableFamily::Astc),
            TargetFormat::Bc1Rgb | TargetFormat::Bc3Rgba | TargetFormat::Bc7Rgba => {
                Some(TableFamily::Bc)
            }
            _ => None,
        }
    }

    fn slots(self) -> &'static [&'static OnceBox<Solutions>] {
        match self {
            TableFamily::Astc => &ASTC_SLOTS,
            TableFamily::Bc => &BC_SLOTS,
        }
    }
}

/// Why a bundle was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TablesError {
    /// Not a table bundle (wrong magic).
    BadMagic,
    /// A family id this build does not know.
    UnknownFamily(u8),
    /// The table count does not match the family.
    BadTableCount { family: TableFamily, count: u8 },
    /// The bundle is not exactly the family's length.
    BadLength { expected: usize, actual: usize },
}

static ASTC: OnceBox<Solutions> = OnceBox::new();
static ASTC_0_255: OnceBox<Solutions> = OnceBox::new();
static BC7_M5_COLOR: OnceBox<Solutions> = OnceBox::new();
static DXT_5: OnceBox<Solutions> = OnceBox::new();
static DXT_6: OnceBox<Solutions> = OnceBox::new();
static ASTC_SLOTS: [&OnceBox<Solutions>; 2] = [&ASTC, &ASTC_0_255];
static BC_SLOTS: [&OnceBox<Solutions>; 3] = [&BC7_M5_COLOR, &DXT_5, &DXT_6];
#[cfg(feature = "embedded-tables")]
static ASTC_EMBEDDED: [&Solutions; 2] = [
    &super::astc::G_ETC1_TO_ASTC,
    &super::astc_0_255::G_ETC1_TO_ASTC_0_255,
];
#[cfg(feature = "embedded-tables")]
static BC_EMBEDDED: [&Solutions; 3] = [
    &super::bc7_m5_color::G_ETC1_TO_BC7_M5_COLOR,
    &super::dxt1_5::G_ETC1_TO_DXT_5,
    &super::dxt1_6::G_ETC1_TO_DXT_6,
];

/// Whether every table of `family` is available: always with
/// `embedded-tables`, after a successful [`install`] otherwise.
pub fn installed(family: TableFamily) -> bool {
    if cfg!(feature = "embedded-tables") {
        return true;
    }
    family.slots().iter().all(|slot| slot.get().is_some())
}

/// Whether an ETC1S transcode to `target` has its tables; targets that read
/// none, and every other source, are unaffected.
pub fn ready_for(target: TargetFormat, source: SourceFormat) -> bool {
    match (source, TableFamily::for_target(target)) {
        (SourceFormat::Etc1s, Some(family)) => installed(family),
        _ => true,
    }
}

/// Parse the header of a bundle without installing it.
pub fn bundle_family(bytes: &[u8]) -> Result<TableFamily, TablesError> {
    if bytes.len() < HEADER_BYTES || bytes[..4] != MAGIC {
        return Err(TablesError::BadMagic);
    }
    let family = TableFamily::from_id(bytes[4]).ok_or(TablesError::UnknownFamily(bytes[4]))?;
    if usize::from(bytes[5]) != family.table_count() {
        return Err(TablesError::BadTableCount {
            family,
            count: bytes[5],
        });
    }
    if bytes.len() != family.bundle_len() {
        return Err(TablesError::BadLength {
            expected: family.bundle_len(),
            actual: bytes.len(),
        });
    }
    Ok(family)
}

/// Install a family's bundle. Idempotent: a family installed twice keeps the
/// first tables. With `embedded-tables` the bundle is only validated — the
/// tables are already in the binary.
pub fn install(bytes: &[u8]) -> Result<TableFamily, TablesError> {
    let family = bundle_family(bytes)?;
    if cfg!(feature = "embedded-tables") {
        return Ok(family);
    }
    for (index, slot) in family.slots().iter().enumerate() {
        let start = HEADER_BYTES + index * TABLE_BYTES;
        slot.get_or_init(|| parse_table(&bytes[start..start + TABLE_BYTES]));
    }
    Ok(family)
}

/// Decode a bundle into its tables without installing anything: the parity
/// check a consumer runs against the embedded tables.
pub fn decode_bundle(bytes: &[u8]) -> Result<(TableFamily, Vec<Box<Solutions>>), TablesError> {
    let family = bundle_family(bytes)?;
    let tables = (0..family.table_count())
        .map(|index| {
            let start = HEADER_BYTES + index * TABLE_BYTES;
            parse_table(&bytes[start..start + TABLE_BYTES])
        })
        .collect();
    Ok((family, tables))
}

fn parse_table(bytes: &[u8]) -> Box<Solutions> {
    let entries: Vec<Etc1ToSolution> = bytes
        .chunks_exact(ENTRY_BYTES)
        .map(|entry| Etc1ToSolution {
            m_lo: entry[0],
            m_hi: entry[1],
            m_err: u16::from_le_bytes([entry[2], entry[3]]),
        })
        .collect();
    // `chunks_exact` over a length-checked slice yields exactly SOLUTION_LEN.
    match entries.into_boxed_slice().try_into() {
        Ok(table) => table,
        Err(_) => Box::new([Etc1ToSolution::default(); SOLUTION_LEN]),
    }
}

/// Serialize a family's embedded tables as the bundle [`install`] accepts —
/// how a consumer produces the files it ships next to its wasm.
#[cfg(feature = "embedded-tables")]
pub fn embedded_bundle(family: TableFamily) -> Vec<u8> {
    let mut out = Vec::with_capacity(family.bundle_len());
    out.extend_from_slice(&MAGIC);
    out.push(family.id());
    out.push(family.table_count() as u8);
    out.extend_from_slice(&[0, 0]);
    for table in embedded(family) {
        for entry in table.iter() {
            out.push(entry.m_lo);
            out.push(entry.m_hi);
            out.extend_from_slice(&entry.m_err.to_le_bytes());
        }
    }
    out
}

#[cfg(feature = "embedded-tables")]
fn embedded(family: TableFamily) -> &'static [&'static Solutions] {
    match family {
        TableFamily::Astc => &ASTC_EMBEDDED,
        TableFamily::Bc => &BC_EMBEDDED,
    }
}

fn table(
    slot: &'static OnceBox<Solutions>,
    embedded: Option<&'static Solutions>,
) -> &'static Solutions {
    // A reader only runs behind `ready_for`, so an empty slot is a dispatch bug.
    embedded
        .or_else(|| slot.get())
        .expect("ETC1S solution tables read before install; Transcoder::supports gates this path")
}

/// The ASTC family: `([0, 47] table, [0, 255] table)`.
pub fn astc_tables() -> (&'static Solutions, &'static Solutions) {
    #[cfg(feature = "embedded-tables")]
    let (a, b) = (
        Some(&super::astc::G_ETC1_TO_ASTC),
        Some(&super::astc_0_255::G_ETC1_TO_ASTC_0_255),
    );
    #[cfg(not(feature = "embedded-tables"))]
    let (a, b) = (None, None);
    (table(&ASTC, a), table(&ASTC_0_255, b))
}

/// The BC7 mode-5 color table.
pub fn bc7_m5_color() -> &'static Solutions {
    #[cfg(feature = "embedded-tables")]
    let e = Some(&super::bc7_m5_color::G_ETC1_TO_BC7_M5_COLOR);
    #[cfg(not(feature = "embedded-tables"))]
    let e = None;
    table(&BC7_M5_COLOR, e)
}

/// The DXT family: `(5-bit table, 6-bit table)`.
pub fn dxt_tables() -> (&'static Solutions, &'static Solutions) {
    #[cfg(feature = "embedded-tables")]
    let (a, b) = (
        Some(&super::dxt1_5::G_ETC1_TO_DXT_5),
        Some(&super::dxt1_6::G_ETC1_TO_DXT_6),
    );
    #[cfg(not(feature = "embedded-tables"))]
    let (a, b) = (None, None);
    (table(&DXT_5, a), table(&DXT_6, b))
}
