use std::{
    io,
    path::{Path, PathBuf},
};

pub mod cec;
pub mod compatibility;
pub mod converter;
pub mod events;
pub mod extras_transaction;
#[allow(clippy::large_const_arrays)]
pub mod meow_transform_table;
pub mod process_probe;
pub mod profile;
pub mod progress;
pub mod revision;
pub mod transaction;
pub mod transform_table;
pub mod transforms;

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("unsupported or invalid save: {0}")]
    InvalidSave(String),
    #[error("unsafe install refused: {0}")]
    UnsafeInstall(String),
    #[error("I/O error while {operation} `{}`: {source}", path.display())]
    IoAtPath {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub fn io_at_path<T>(
    result: io::Result<T>,
    operation: &'static str,
    path: &Path,
) -> Result<T, ConversionError> {
    result.map_err(|source| ConversionError::IoAtPath {
        operation,
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod transform_table_tests {
    use crate::{
        profile::PAYLOAD_SIZE,
        transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS, SWAP_SPANS},
    };

    #[test]
    fn pinned_transform_table_has_expected_counts_and_payload_bounds() {
        assert_eq!(SWAP_SPANS.len(), 8_509);
        assert_eq!(MONSTER_DISCOVERY_OFFSETS.len(), 50);
        assert_eq!(ARENA_RECORD_OFFSETS.len(), 62);
        assert!(
            SWAP_SPANS
                .iter()
                .all(|span| span.start < span.end && span.end <= PAYLOAD_SIZE)
        );
        assert!(
            MONSTER_DISCOVERY_OFFSETS
                .iter()
                .all(|&offset| offset + 2 <= PAYLOAD_SIZE)
        );
        assert!(
            ARENA_RECORD_OFFSETS
                .iter()
                .all(|&offset| offset + 4 <= PAYLOAD_SIZE)
        );
    }
}
