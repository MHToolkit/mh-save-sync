use crate::{
    ConversionError,
    profile::{
        CEMU_SIZE, CEMU_SYSTEM_SIZE, JP_3DS_HEADER, PAYLOAD_SIZE, SYSTEM_PAYLOAD_SIZE, SaveProfile,
        build_jp_cemu_header, inspect_bytes,
    },
    transforms::{
        GuildCardBodyKind, apply_japanese_wiiu_corrections,
        apply_japanese_wiiu_guild_card_corrections,
    },
};

/// MH3G extra-data components shared between all character slots.
///
/// These live in the 3DS extdata `00000481/user` directory and are separate
/// from `user1`/`user2`/`user3`.  Guild-card data is stored in card1-card3 and
/// cardbox, while the quest files carry downloaded/created quest data.
pub const EXTERNAL_COMPONENT_NAMES: [&str; 8] = [
    "card1", "card2", "card3", "cardbox", "quest1", "quest2", "quest3", "quest4",
];

const CARD_PAYLOAD_SIZE: usize = 0x57_FFC;
const CARDBOX_PAYLOAD_SIZE: usize = 0x2F_FFC;
const QUEST_PAYLOAD_SIZE: usize = 0x28_FFC;

fn external_component_payload_size(filename: &str) -> Option<usize> {
    match filename {
        "card1" | "card2" | "card3" => Some(CARD_PAYLOAD_SIZE),
        "cardbox" => Some(CARDBOX_PAYLOAD_SIZE),
        "quest1" | "quest2" | "quest3" | "quest4" => Some(QUEST_PAYLOAD_SIZE),
        _ => None,
    }
}

fn is_guild_card_component(filename: &str) -> bool {
    matches!(filename, "card1" | "card2" | "card3" | "cardbox")
}

/// Convert one MH3G 3DS extra-data component into its Cemu save container.
///
/// Card bodies have their own platform-specific scalar and bitfield mapping;
/// quest bodies are already compatible and only receive the Cemu wrapper.
pub fn convert_external_component_to_cemu_named(
    source: &[u8],
    filename: &str,
) -> Result<Vec<u8>, ConversionError> {
    let payload_size = external_component_payload_size(filename).ok_or_else(|| {
        ConversionError::InvalidSave(format!("unsupported MH3G extra-data component: {filename}"))
    })?;
    let expected_size = JP_3DS_HEADER.len() + payload_size;
    if source.len() != expected_size || !source.starts_with(&JP_3DS_HEADER) {
        return Err(ConversionError::InvalidSave(format!(
            "invalid Japanese MH3G 3DS extra-data {filename}: expected {expected_size} bytes with 3DS header"
        )));
    }

    let source_payload = &source[JP_3DS_HEADER.len()..];
    let mut payload = source_payload.to_vec();
    match filename {
        "card1" | "card2" | "card3" => apply_japanese_wiiu_guild_card_corrections(
            GuildCardBodyKind::Card,
            source_payload,
            &mut payload,
        )?,
        "cardbox" => apply_japanese_wiiu_guild_card_corrections(
            GuildCardBodyKind::Cardbox,
            source_payload,
            &mut payload,
        )?,
        "quest1" | "quest2" | "quest3" | "quest4" => {}
        _ => unreachable!("external_component_payload_size validated the component"),
    }

    let mut output = Vec::with_capacity(payload_size + 40);
    output.extend_from_slice(&build_jp_cemu_header(filename, payload_size)?);
    output.extend_from_slice(&payload);
    Ok(output)
}

/// Create an empty Cemu guild-card component from a valid 3DS component.
///
/// This is an explicit destructive compatibility fallback, not a semantic
/// conversion: it creates the same empty component shape as a native Cemu
/// save. It never runs implicitly because it discards local and received cards.
pub fn reset_guild_card_component_to_cemu_named(
    source: &[u8],
    filename: &str,
) -> Result<Vec<u8>, ConversionError> {
    if !is_guild_card_component(filename) {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported MH3G guild-card component: {filename}"
        )));
    }
    let payload_size = external_component_payload_size(filename)
        .expect("guild-card component has a declared payload size");
    let expected_size = JP_3DS_HEADER.len() + payload_size;
    if source.len() != expected_size || !source.starts_with(&JP_3DS_HEADER) {
        return Err(ConversionError::InvalidSave(format!(
            "invalid Japanese MH3G 3DS extra-data {filename}: expected {expected_size} bytes with 3DS header"
        )));
    }

    let mut output = build_jp_cemu_header(filename, payload_size)?.to_vec();
    output.resize(payload_size + 40, 0);
    Ok(output)
}

/// Convert one Japanese MH3G 3DS slot into the Japanese Cemu slot format.
///
/// The conversion is deliberately pure: the input is never modified and no
/// filesystem or emulator state is accessed.
pub fn convert_3ds_to_cemu(source: &[u8]) -> Result<Vec<u8>, ConversionError> {
    convert_3ds_to_cemu_named(source, "user2")
}

pub fn convert_3ds_to_cemu_named(
    source: &[u8],
    filename: &str,
) -> Result<Vec<u8>, ConversionError> {
    let inspection = inspect_bytes(source)?;
    if inspection.profile != SaveProfile::JpThreeDs {
        return Err(ConversionError::InvalidSave(format!(
            "expected a Japanese MH3G 3DS save with header {:02X?}",
            JP_3DS_HEADER
        )));
    }

    let source_payload = &source[JP_3DS_HEADER.len()..];
    let mut payload = source_payload.to_vec();
    apply_japanese_wiiu_corrections(source_payload, &mut payload)?;

    let mut output = Vec::with_capacity(CEMU_SIZE);
    output.extend_from_slice(&build_jp_cemu_header(filename, PAYLOAD_SIZE)?);
    output.extend_from_slice(&payload);

    inspect_bytes(&output)?;
    Ok(output)
}

/// Convert the Japanese MH3G 3DS shared system data into the Cemu container.
///
/// The `system` payload is already serialized in the same byte order in both
/// versions. Unlike character slots, this conversion only replaces the outer
/// save container header.
pub fn convert_3ds_system_to_cemu(source: &[u8]) -> Result<Vec<u8>, ConversionError> {
    convert_3ds_system_to_cemu_named(source, "system")
}

pub fn convert_3ds_system_to_cemu_named(
    source: &[u8],
    filename: &str,
) -> Result<Vec<u8>, ConversionError> {
    let inspection = inspect_bytes(source)?;
    if inspection.profile != SaveProfile::JpThreeDsSystem {
        return Err(ConversionError::InvalidSave(
            "expected a Japanese MH3G 3DS system save".to_owned(),
        ));
    }

    let mut payload = source[JP_3DS_HEADER.len()..].to_vec();
    for word in payload[48..].chunks_exact_mut(4) {
        word.reverse();
    }

    let mut output = Vec::with_capacity(CEMU_SYSTEM_SIZE);
    output.extend_from_slice(&build_jp_cemu_header(filename, SYSTEM_PAYLOAD_SIZE)?);
    output.extend_from_slice(&payload);

    inspect_bytes(&output)?;
    Ok(output)
}

pub fn convert_source_to_cemu(source: &[u8], filename: &str) -> Result<Vec<u8>, ConversionError> {
    match inspect_bytes(source)?.profile {
        SaveProfile::JpThreeDs => convert_3ds_to_cemu_named(source, filename),
        SaveProfile::JpThreeDsSystem => convert_3ds_system_to_cemu_named(source, filename),
        SaveProfile::JpCemu | SaveProfile::JpCemuSystem => Err(ConversionError::InvalidSave(
            "expected a Japanese MH3G 3DS save, not an existing Cemu save".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{CARD_PAYLOAD_SIZE, CARDBOX_PAYLOAD_SIZE, QUEST_PAYLOAD_SIZE};
    use crate::{
        converter::{
            convert_3ds_system_to_cemu, convert_3ds_to_cemu, convert_3ds_to_cemu_named,
            convert_external_component_to_cemu_named,
        },
        profile::{
            CEMU_SIZE, JP_3DS_HEADER, JP_CEMU_HEADER, PAYLOAD_SIZE, SaveProfile, THREE_DS_SIZE,
            build_jp_cemu_header, inspect_bytes,
        },
    };
    use sha2::{Digest, Sha256};

    fn synthetic_3ds_source() -> Vec<u8> {
        let mut source = (0..THREE_DS_SIZE)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        source
    }

    #[test]
    fn converts_a_japanese_3ds_system_by_replacing_only_the_container_header() {
        use crate::profile::{
            CEMU_SYSTEM_SIZE, JP_CEMU_SYSTEM_HEADER, SYSTEM_PAYLOAD_SIZE, THREE_DS_SYSTEM_SIZE,
        };

        let mut source = (0..THREE_DS_SYSTEM_SIZE)
            .map(|index| (index as u8).wrapping_mul(17).wrapping_add(3))
            .collect::<Vec<_>>();
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);

        let output = convert_3ds_system_to_cemu(&source).unwrap();

        assert_eq!(output.len(), CEMU_SYSTEM_SIZE);
        assert_eq!(
            &output[..JP_CEMU_SYSTEM_HEADER.len()],
            &build_jp_cemu_header("system", SYSTEM_PAYLOAD_SIZE).unwrap()
        );
        let mut expected_payload =
            source[JP_3DS_HEADER.len()..JP_3DS_HEADER.len() + SYSTEM_PAYLOAD_SIZE].to_vec();
        for word in expected_payload[48..].chunks_exact_mut(4) {
            word.reverse();
        }
        assert_eq!(&output[JP_CEMU_SYSTEM_HEADER.len()..], &expected_payload);
        assert_eq!(
            inspect_bytes(&output).unwrap().profile,
            SaveProfile::JpCemuSystem
        );
    }

    #[test]
    fn converts_a_japanese_3ds_save_without_mutating_the_source() {
        let source = synthetic_3ds_source();
        let source_before = source.clone();

        let output = convert_3ds_to_cemu(&source).unwrap();

        assert_eq!(source, source_before);
        assert_eq!(output.len(), CEMU_SIZE);
        assert_eq!(&output[..JP_CEMU_HEADER.len()], &JP_CEMU_HEADER);
        assert_eq!(inspect_bytes(&output).unwrap().profile, SaveProfile::JpCemu);
    }

    #[test]
    fn applies_meow_static_operations_to_the_payload() {
        let endian_offset = 28;
        let monster_offset = 33_212;
        let arena_offset = 33_704;

        let mut source = synthetic_3ds_source();
        source[JP_3DS_HEADER.len() + endian_offset..JP_3DS_HEADER.len() + endian_offset + 4]
            .copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        source[JP_3DS_HEADER.len() + monster_offset..JP_3DS_HEADER.len() + monster_offset + 2]
            .copy_from_slice(&[0x07, 0xA5]);
        source[JP_3DS_HEADER.len() + arena_offset..JP_3DS_HEADER.len() + arena_offset + 4]
            .copy_from_slice(&[0x55, 0xAA, 0xAA, 0x55]);

        let output = convert_3ds_to_cemu(&source).unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[endian_offset..endian_offset + 4],
            &[0x44, 0x33, 0x22, 0x11]
        );
        assert_eq!(&payload[monster_offset..monster_offset + 2], &[0xE0, 0xA5]);
        assert_eq!(
            &payload[arena_offset..arena_offset + 4],
            &u32::from_le_bytes([0x55, 0xAA, 0xAA, 0x55])
                .rotate_left(17)
                .to_be_bytes()
        );
    }

    #[test]
    fn rejects_non_japanese_3ds_sources_and_existing_cemu_saves() {
        let mut western_source = synthetic_3ds_source();
        western_source[0] = 0x2C;
        assert!(convert_3ds_to_cemu(&western_source).is_err());

        let mut cemu_source = vec![0_u8; CEMU_SIZE];
        cemu_source[..JP_CEMU_HEADER.len()].copy_from_slice(&JP_CEMU_HEADER);
        assert!(convert_3ds_to_cemu(&cemu_source).is_err());

        let truncated_source = &synthetic_3ds_source()[..THREE_DS_SIZE - 1];
        assert!(convert_3ds_to_cemu(truncated_source).is_err());
    }

    #[test]
    fn named_slots_receive_their_own_wiiu_header_checksum() {
        let output = convert_3ds_to_cemu_named(&synthetic_3ds_source(), "user1").unwrap();
        assert_eq!(
            &output[..40],
            &build_jp_cemu_header("user1", PAYLOAD_SIZE).unwrap()
        );
    }

    fn synthetic_external_component(payload_size: usize) -> Vec<u8> {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + payload_size];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        for (index, byte) in source[JP_3DS_HEADER.len()..].iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
        }
        source
    }

    #[test]
    fn transforms_card_payload_with_the_recovered_meow_mapping() {
        let source = synthetic_external_component(CARD_PAYLOAD_SIZE);
        let source_before = source.clone();

        let output = convert_external_component_to_cemu_named(&source, "card2").unwrap();

        assert_eq!(source, source_before);
        assert_eq!(output.len(), 0x58_024);
        assert_eq!(
            &output[..JP_CEMU_HEADER.len()],
            &build_jp_cemu_header("card2", CARD_PAYLOAD_SIZE).unwrap()
        );
        assert_eq!(
            hex::encode(Sha256::digest(&output[JP_CEMU_HEADER.len()..])),
            "915ef8bf5ee22ee22d2260f58dca09bacef85b6f32aaf71bf4088a5feb7f7fd6"
        );
    }

    #[test]
    fn transforms_cardbox_payload_with_the_recovered_meow_mapping() {
        let source = synthetic_external_component(CARDBOX_PAYLOAD_SIZE);

        let output = convert_external_component_to_cemu_named(&source, "cardbox").unwrap();

        assert_eq!(output.len(), 0x30_024);
        assert_eq!(
            hex::encode(Sha256::digest(&output[JP_CEMU_HEADER.len()..])),
            "60d246ee5ff639cd0f67109e82e167a4b0126bc0160da1ab3dc82e440905c877"
        );
    }

    #[test]
    fn remaps_guild_card_journal_dates_and_monster_log_fields() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        // A card's latest Hunter's Journal date is day/month/u16-year.
        body[0x17A..0x17C].copy_from_slice(&[0xEA, 0x07]);
        // Journal counters are u16 fields inside a 0xA0-byte record.
        body[0x6378..0x6380].copy_from_slice(&[0x07, 0x07, 0xEA, 0x07, 0x04, 0x00, 0x91, 0x2C]);
        // Monster-log entries contain four u16 values plus a crown/discovery bitfield.
        body[0x7C0..0x7CA]
            .copy_from_slice(&[0x16, 0x00, 0x02, 0x00, 0x78, 0x00, 0x5C, 0x00, 0x03, 0x00]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(&payload[0x17A..0x17C], &[0x07, 0xEA]);
        assert_eq!(
            &payload[0x6378..0x6380],
            &[0x07, 0x07, 0x07, 0xEA, 0x00, 0x04, 0x2C, 0x91]
        );
        assert_eq!(
            &payload[0x7C0..0x7CA],
            &[0x00, 0x16, 0x00, 0x02, 0x00, 0x78, 0x00, 0x5C, 0xA0, 0x00]
        );
    }

    #[test]
    fn remaps_each_received_card_monster_record_as_four_independent_u16_values() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        // A full card body stores received cards in 0xE00-byte slots.  The
        // 50-entry monster log begins at +0x7C0; each row has four u16 fields
        // followed by its crown/discovery bytes.  A non-first slot catches the
        // historical table-driven cross-field u32 swap.
        let second_card_row = 0xE00 + 0x7C0 + 32 * 10;
        body[second_card_row..second_card_row + 10]
            .copy_from_slice(&[0x0F, 0x00, 0x10, 0x00, 0x64, 0x00, 0x65, 0x00, 0x03, 0x00]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[second_card_row..second_card_row + 8],
            &[0x00, 0x0F, 0x00, 0x10, 0x00, 0x64, 0x00, 0x65]
        );
    }

    #[test]
    fn remaps_every_received_card_friendship_score() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        // Slot 98 contains 0x38-byte received-card summary records. The
        // earlier sparse MEOW table missed this fifth friendship score, leaving
        // the 3DS little-endian 0x00000F10 as a huge Wii U number.
        let fifth_score = 98 * 0xE00 + 4 * 0x38 + 0x34;
        body[fifth_score..fifth_score + 4].copy_from_slice(&[0x10, 0x0F, 0x00, 0x00]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[fifth_score..fifth_score + 4],
            &[0x00, 0x00, 0x0F, 0x10]
        );
    }

    #[test]
    fn preserves_quest_payloads_byte_for_byte() {
        let source = synthetic_external_component(QUEST_PAYLOAD_SIZE);
        let output = convert_external_component_to_cemu_named(&source, "quest1").unwrap();

        assert_eq!(
            &output[..JP_CEMU_HEADER.len()],
            &build_jp_cemu_header("quest1", QUEST_PAYLOAD_SIZE).unwrap()
        );
        assert_eq!(
            &output[JP_CEMU_HEADER.len()..],
            &source[JP_3DS_HEADER.len()..]
        );
    }
}
