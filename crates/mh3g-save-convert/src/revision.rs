use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Historical converter semantics that can be reproduced for a compatibility
/// merge. Keep this list closed: adding a revision requires a reviewed field
/// map and synthetic parity tests.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
pub enum ConverterRevision {
    #[value(name = "0.0.3")]
    #[serde(rename = "0.0.3")]
    V0_0_3,
    #[value(name = "0.0.4")]
    #[serde(rename = "0.0.4")]
    V0_0_4,
    #[value(name = "0.0.5")]
    #[serde(rename = "0.0.5")]
    V0_0_5,
    #[value(name = "0.0.6")]
    #[serde(rename = "0.0.6")]
    V0_0_6,
}

impl ConverterRevision {
    pub const ALL: [Self; 4] = [Self::V0_0_3, Self::V0_0_4, Self::V0_0_5, Self::V0_0_6];
    /// Last released historical algorithm that compatibility repair can replay.
    ///
    /// This is intentionally not named `LATEST`: current conversion may layer
    /// newer corrections on top while keeping 0.0.3-0.0.6 byte-reproducible.
    pub const LAST_HISTORICAL: Self = Self::V0_0_6;

    pub const fn label(self) -> &'static str {
        match self {
            Self::V0_0_3 => "0.0.3",
            Self::V0_0_4 => "0.0.4",
            Self::V0_0_5 => "0.0.5",
            Self::V0_0_6 => "0.0.6",
        }
    }
}
