use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ConversionError;

pub const THREE_DS_SIZE: usize = 0x8A00;
pub const CEMU_SIZE: usize = 0x8A24;
pub const PAYLOAD_SIZE: usize = 0x89FC;
pub const JP_3DS_HEADER: [u8; 4] = [0x2B, 0, 0, 0];
pub const JP_CEMU_HEADER: [u8; 40] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0x14, 0xEA, 0xA7, 0x2B, 0x10, 0, 0, 0,
    0x0C, 0, 0, 0x8A, 0, 0, 0, 0, 0, 0, 0, 0, 0x2B,
];
pub const THREE_DS_SYSTEM_SIZE: usize = 0x3000;
pub const CEMU_SYSTEM_SIZE: usize = 0x3024;
pub const SYSTEM_PAYLOAD_SIZE: usize = 0x2FFC;
pub const JP_CEMU_SYSTEM_HEADER: [u8; 40] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0x14, 0, 0, 0, 0, 0, 0, 0, 0x0C, 0, 0,
    0x30, 0, 0, 0, 0, 0, 0, 0, 0, 0x2B,
];

pub fn build_jp_cemu_header(
    filename: &str,
    payload_size: usize,
) -> Result<[u8; 40], ConversionError> {
    if !filename.is_ascii() {
        return Err(ConversionError::InvalidSave(format!(
            "Wii U save filename must be ASCII: {filename}"
        )));
    }
    let total_size = payload_size
        .checked_add(JP_3DS_HEADER.len())
        .ok_or_else(|| {
            ConversionError::InvalidSave("Wii U save length overflows u32".to_owned())
        })?;
    let total_size = u32::try_from(total_size)
        .map_err(|_| ConversionError::InvalidSave("Wii U save length exceeds u32".to_owned()))?;

    let mut header = [0_u8; 40];
    for (index, word) in [
        0_u32,
        0,
        4,
        0,
        20,
        !crc32(filename.as_bytes()),
        12,
        total_size,
        0,
        u32::from(JP_3DS_HEADER[0]),
    ]
    .into_iter()
    .enumerate()
    {
        header[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    Ok(header)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xEDB8_8320
            };
        }
    }
    !crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveProfile {
    JpThreeDs,
    JpCemu,
    JpThreeDsSystem,
    JpCemuSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inspection {
    pub profile: SaveProfile,
    pub size: usize,
    pub sha256: String,
}

pub fn inspect_bytes(bytes: &[u8]) -> Result<Inspection, ConversionError> {
    let profile = match bytes.len() {
        THREE_DS_SIZE if bytes.starts_with(&JP_3DS_HEADER) => SaveProfile::JpThreeDs,
        CEMU_SIZE if is_jp_cemu_slot(bytes) => SaveProfile::JpCemu,
        THREE_DS_SYSTEM_SIZE if bytes.starts_with(&JP_3DS_HEADER) => SaveProfile::JpThreeDsSystem,
        CEMU_SYSTEM_SIZE if is_jp_cemu_system(bytes) => SaveProfile::JpCemuSystem,
        size => {
            return Err(ConversionError::InvalidSave(format!(
                "unrecognized Japanese MH3G save size/header combination ({size} bytes)"
            )));
        }
    };

    Ok(Inspection {
        profile,
        size: bytes.len(),
        sha256: hex::encode(Sha256::digest(bytes)),
    })
}

fn is_jp_cemu_slot(bytes: &[u8]) -> bool {
    bytes.len() == CEMU_SIZE
        && bytes[..20] == JP_CEMU_HEADER[..20]
        && bytes[24..JP_CEMU_HEADER.len()] == JP_CEMU_HEADER[24..]
}

fn is_jp_cemu_system(bytes: &[u8]) -> bool {
    bytes.len() == CEMU_SYSTEM_SIZE
        && bytes[..20] == JP_CEMU_SYSTEM_HEADER[..20]
        && bytes[24..JP_CEMU_SYSTEM_HEADER.len()] == JP_CEMU_SYSTEM_HEADER[24..]
}

pub fn validate_slot_path(path: &Path) -> Result<(), ConversionError> {
    validate_save_component_path(path)?;
    match path.file_name().and_then(|name| name.to_str()) {
        Some("user1" | "user2" | "user3") => Ok(()),
        _ => Err(ConversionError::InvalidSave(format!(
            "save slot basename must be user1, user2, or user3: {}",
            path.display()
        ))),
    }
}

pub fn validate_system_path(path: &Path) -> Result<(), ConversionError> {
    validate_save_component_path(path)?;
    match path.file_name().and_then(|name| name.to_str()) {
        Some("system") => Ok(()),
        _ => Err(ConversionError::InvalidSave(format!(
            "shared system basename must be system: {}",
            path.display()
        ))),
    }
}

pub fn validate_save_component_path(path: &Path) -> Result<(), ConversionError> {
    if path.is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "save component path is a directory: {}",
            path.display()
        )));
    }

    match path.file_name().and_then(|name| name.to_str()) {
        Some("user1" | "user2" | "user3" | "system") => Ok(()),
        _ => Err(ConversionError::InvalidSave(format!(
            "save component basename must be user1, user2, user3, or system: {}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn jp_3ds_fixture() -> Vec<u8> {
        let mut bytes = vec![0_u8; THREE_DS_SIZE];
        bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        bytes
    }

    fn jp_cemu_fixture() -> Vec<u8> {
        let mut bytes = vec![0_u8; CEMU_SIZE];
        bytes[..JP_CEMU_HEADER.len()].copy_from_slice(&JP_CEMU_HEADER);
        bytes
    }

    #[test]
    fn inspect_bytes_accepts_a_cemu_slot_after_it_rewrites_its_checksum_field() {
        let mut bytes = jp_cemu_fixture();
        bytes[20..24].copy_from_slice(&[0xEA, 0xA7, 0x2B, 0x10]);

        assert_eq!(inspect_bytes(&bytes).unwrap().profile, SaveProfile::JpCemu);
    }

    #[test]
    fn inspect_bytes_recognizes_japanese_mh3g_system_profiles() {
        let mut three_ds = vec![0_u8; THREE_DS_SYSTEM_SIZE];
        three_ds[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let mut cemu = vec![0_u8; CEMU_SYSTEM_SIZE];
        cemu[..JP_CEMU_SYSTEM_HEADER.len()].copy_from_slice(&JP_CEMU_SYSTEM_HEADER);
        cemu[20..24].copy_from_slice(&[0x36, 0xB2, 0xEE, 0x74]);

        assert_eq!(
            inspect_bytes(&three_ds).unwrap().profile,
            SaveProfile::JpThreeDsSystem
        );
        assert_eq!(
            inspect_bytes(&cemu).unwrap().profile,
            SaveProfile::JpCemuSystem
        );
    }

    #[test]
    fn inspect_bytes_recognizes_japanese_mh3g_profiles() {
        assert_eq!(THREE_DS_SIZE, 0x8A00);
        assert_eq!(CEMU_SIZE, 0x8A24);
        assert_eq!(PAYLOAD_SIZE, 0x89FC);
        assert_eq!(
            inspect_bytes(&jp_3ds_fixture()).unwrap().profile,
            SaveProfile::JpThreeDs
        );
        assert_eq!(
            inspect_bytes(&jp_cemu_fixture()).unwrap().profile,
            SaveProfile::JpCemu
        );
    }

    #[test]
    fn inspection_includes_stable_serializable_metadata() {
        let bytes = jp_3ds_fixture();
        let inspection = inspect_bytes(&bytes).unwrap();

        assert_eq!(inspection.size, THREE_DS_SIZE);
        assert_eq!(inspection.sha256, hex::encode(Sha256::digest(&bytes)));
        assert_eq!(
            serde_json::from_str::<Inspection>(&serde_json::to_string(&inspection).unwrap())
                .unwrap(),
            inspection
        );
    }

    #[test]
    fn inspect_bytes_rejects_invalid_japanese_mh3g_save_data() {
        assert!(inspect_bytes(&vec![0; THREE_DS_SIZE]).is_err());
        assert!(inspect_bytes(&vec![0; THREE_DS_SIZE - 1]).is_err());
    }

    #[test]
    fn validate_slot_path_accepts_only_user_save_slots() {
        for slot in ["user1", "user2", "user3"] {
            assert!(validate_slot_path(Path::new(slot)).is_ok(), "{slot}");
        }

        for invalid in ["user0", "user4", "user1.sav", "system", ""] {
            assert!(validate_slot_path(Path::new(invalid)).is_err(), "{invalid}");
        }
    }

    #[test]
    fn validate_system_path_accepts_only_the_shared_system_file() {
        assert!(validate_system_path(Path::new("system")).is_ok());
        assert!(validate_system_path(Path::new("user2")).is_err());
        assert!(validate_save_component_path(Path::new("system")).is_ok());
    }

    #[test]
    fn validate_slot_path_rejects_directories() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("user1");
        std::fs::create_dir(&directory).unwrap();

        assert!(validate_slot_path(&directory).is_err());
    }
}
