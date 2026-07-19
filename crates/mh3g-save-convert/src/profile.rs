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
    fn validate_slot_path_rejects_directories() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("user1");
        std::fs::create_dir(&directory).unwrap();

        assert!(validate_slot_path(&directory).is_err());
    }
}
