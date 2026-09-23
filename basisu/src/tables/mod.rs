//! Lookup tables used by the decoders. Two kinds live here: static constant
//! tables holding precomputed transcode data, and tables built in code once at
//! first use.

// The shared entry type the single-color match tables are built from.
pub mod solution;

// The five big ETC1S solution tables: embedded with `embedded-tables`, supplied
// at runtime otherwise. Every reader goes through this module.
pub mod lazy;

// Tables built once at first use.
pub mod bc1;

// Static constant tables holding precomputed transcode data.
#[cfg(feature = "embedded-tables")]
pub mod astc;
#[cfg(feature = "embedded-tables")]
pub mod astc_0_255;
pub mod atc_55;
pub mod atc_56;
pub mod bc7_m5_alpha;
#[cfg(feature = "embedded-tables")]
pub mod bc7_m5_color;
pub mod bc7_m5_equals_1;
#[cfg(feature = "embedded-tables")]
pub mod dxt1_5;
#[cfg(feature = "embedded-tables")]
pub mod dxt1_6;
pub mod dxt5a;
pub mod etc2_eac_a8;
pub mod etc2_eac_r11;
pub mod pvrtc2_45;
