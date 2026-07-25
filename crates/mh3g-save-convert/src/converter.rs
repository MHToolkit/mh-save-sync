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
            "137c7581bb31cb27b39e468bf181e2c6155c84f7608b9441cba471a98c7ef927"
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
    fn remaps_every_received_card_weapon_usage_counter_as_an_independent_u16() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        // Each 0xE00-byte card slot embeds the same 36-entry (three pages of
        // twelve) u16 weapon-use counter array as user#.  The static table
        // contains a sparse mixture of 2- and 4-byte spans: the latter swaps
        // adjacent counters.  A non-first slot catches that regression.
        let third_card_usage = 2 * 0xE00 + 0x12C;
        body[third_card_usage..third_card_usage + 8]
            .copy_from_slice(&[0x2B, 0x00, 0x1E, 0x00, 0x11, 0x00, 0x37, 0x00]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[third_card_usage..third_card_usage + 8],
            &[0x00, 0x2B, 0x00, 0x1E, 0x00, 0x11, 0x00, 0x37]
        );
    }

    #[test]
    fn remaps_all_offline_hunter_equipment_caches_and_tail_colors() {
        let mut user_source = vec![0_u8; JP_3DS_HEADER.len() + PAYLOAD_SIZE];
        user_source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);

        // Each of the six offline hunters has a 0x30-byte equipment cache
        // immediately before its 0x40-byte header/name block. The five compact
        // equipment headers are followed by two independent RGBA values.
        for roster_record in 0_u8..6 {
            let cache_start = 0x75B0 + usize::from(roster_record) * 0x70;
            for equipment in 0_u8..5 {
                let offset = cache_start + usize::from(equipment) * 8;
                user_source[JP_3DS_HEADER.len() + offset..JP_3DS_HEADER.len() + offset + 8]
                    .copy_from_slice(&[
                        equipment + 1,
                        0x80 + roster_record,
                        0x30 + equipment,
                        0x40 + roster_record,
                        0xA0 + equipment,
                        0xB0 + roster_record,
                        0xC0 + equipment,
                        0xD0 + roster_record,
                    ]);
            }
            user_source[JP_3DS_HEADER.len() + cache_start + 0x28
                ..JP_3DS_HEADER.len() + cache_start + 0x30]
                .copy_from_slice(&[
                    0x10 + roster_record,
                    0x20 + roster_record,
                    0x30 + roster_record,
                    0x40 + roster_record,
                    0x50 + roster_record,
                    0x60 + roster_record,
                    0x70 + roster_record,
                    0x80 + roster_record,
                ]);
        }

        let user_output = convert_3ds_to_cemu_named(&user_source, "user1").unwrap();
        let user_body = &user_output[JP_CEMU_HEADER.len()..];

        for roster_record in 0_u8..6 {
            let cache_start = 0x75B0 + usize::from(roster_record) * 0x70;
            for equipment in 0_u8..5 {
                let offset = cache_start + usize::from(equipment) * 8;
                assert_eq!(
                    &user_body[offset..offset + 8],
                    &[
                        equipment + 1,
                        0x80 + roster_record,
                        0x40 + roster_record,
                        0x30 + equipment,
                        0xD0 + roster_record,
                        0xC0 + equipment,
                        0xB0 + roster_record,
                        0xA0 + equipment,
                    ],
                    "roster {roster_record} equipment {equipment}"
                );
            }
            assert_eq!(
                &user_body[cache_start + 0x28..cache_start + 0x30],
                &[
                    0x40 + roster_record,
                    0x30 + roster_record,
                    0x20 + roster_record,
                    0x10 + roster_record,
                    0x80 + roster_record,
                    0x70 + roster_record,
                    0x60 + roster_record,
                    0x50 + roster_record,
                ],
                "roster {roster_record} tail colors"
            );
        }
    }

    #[test]
    fn preserves_offline_hunter_names_and_card_links() {
        let mut user_source = vec![0_u8; JP_3DS_HEADER.len() + PAYLOAD_SIZE];
        user_source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let mut card_source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        card_source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);

        let mut names = [[0_u8; 16]; 6];
        let mut links = [[0_u8; 8]; 6];
        let card_slots = [12_usize, 3, 13, 10, 9, 4];
        for (record, card_slot) in card_slots.into_iter().enumerate() {
            for (index, byte) in names[record].iter_mut().enumerate() {
                *byte = 0x40 + (record as u8) * 0x10 + index as u8;
            }
            let name_offset = 0x75E0 + record * 0x70 + 0x1C;
            user_source[JP_3DS_HEADER.len() + name_offset
                ..JP_3DS_HEADER.len() + name_offset + names[record].len()]
                .copy_from_slice(&names[record]);

            let link = [
                0x11 + record as u8,
                0x22 + record as u8,
                0x33 + record as u8,
                0x44 + record as u8,
                0x55 + record as u8,
                0x66 + record as u8,
                0x77 + record as u8,
                0x88 + record as u8,
            ];
            links[record] = link;
            let roster_link = 0x75E0 + record * 0x70 + 0x10;
            let card_link = card_slot * 0xE00 + 0x11A;
            user_source
                [JP_3DS_HEADER.len() + roster_link..JP_3DS_HEADER.len() + roster_link + link.len()]
                .copy_from_slice(&link);
            card_source
                [JP_3DS_HEADER.len() + card_link..JP_3DS_HEADER.len() + card_link + link.len()]
                .copy_from_slice(&link);
        }

        // The selected offline-hall candidate follows the six hunter headers.
        // Its card anchor begins at queue +0x0A and must use the same exact
        // eight-byte boundary as card slot +0x11A.
        let queue_link = [0x91, 0x82, 0x73, 0x64, 0x55, 0x46, 0x37, 0x28];
        let queue_link_offset = 0x7850 + 0x0A;
        let queue_card_link = 7 * 0xE00 + 0x11A;
        user_source[JP_3DS_HEADER.len() + queue_link_offset
            ..JP_3DS_HEADER.len() + queue_link_offset + queue_link.len()]
            .copy_from_slice(&queue_link);
        card_source[JP_3DS_HEADER.len() + queue_card_link
            ..JP_3DS_HEADER.len() + queue_card_link + queue_link.len()]
            .copy_from_slice(&queue_link);

        let user_output = convert_3ds_to_cemu_named(&user_source, "user1").unwrap();
        let card_output = convert_external_component_to_cemu_named(&card_source, "card1").unwrap();
        let user_body = &user_output[JP_CEMU_HEADER.len()..];
        let card_body = &card_output[JP_CEMU_HEADER.len()..];

        for (record, card_slot) in card_slots.into_iter().enumerate() {
            let name_offset = 0x75E0 + record * 0x70 + 0x1C;
            assert_eq!(
                &user_body[name_offset..name_offset + names[record].len()],
                &names[record]
            );
            let roster_link = 0x75E0 + record * 0x70 + 0x10;
            let card_link = card_slot * 0xE00 + 0x11A;
            assert_eq!(
                &user_body[roster_link..roster_link + 8],
                &links[record],
                "hunter {record} roster link"
            );
            assert_eq!(
                &card_body[card_link..card_link + 8],
                &links[record],
                "hunter {record} card link"
            );
        }
        assert_eq!(
            &user_body[queue_link_offset..queue_link_offset + queue_link.len()],
            &queue_link
        );
        assert_eq!(
            &card_body[queue_card_link..queue_card_link + queue_link.len()],
            &queue_link
        );
    }

    #[test]
    fn remaps_offline_hunter_candidate_ids_as_independent_u16_values() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        // The offline-hall queue stores three table IDs immediately before
        // its selected/state bytes. The 3DS serializes each ID little-endian;
        // MH3G HD looks them up as independent big-endian u16 values.
        body[0x7848..0x784E].copy_from_slice(&[0x83, 0x00, 0xEA, 0x00, 0xD2, 0x00]);
        body[0x784E..0x7852].copy_from_slice(&[0x01, 0x01, 0x03, 0x02]);

        let output = convert_3ds_to_cemu_named(&source, "user1").unwrap();
        let output_body = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &output_body[0x7848..0x784E],
            &[0x00, 0x83, 0x00, 0xEA, 0x00, 0xD2]
        );
        assert_eq!(&output_body[0x784E..0x7852], &[0x01, 0x01, 0x03, 0x02]);
    }

    #[test]
    fn remaps_all_received_card_compact_equipment_headers() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];
        let slot_start = 3 * 0xE00;

        for equipment in 0_u8..5 {
            let offset = slot_start + 0x4C + usize::from(equipment) * 0x10;
            body[offset..offset + 8].copy_from_slice(&[
                equipment + 1,
                0x70,
                0x20 + equipment,
                0x30,
                0xA0 + equipment,
                0xB0,
                0xC0 + equipment,
                0xD0,
            ]);
        }
        body[slot_start + 0x110..slot_start + 0x118]
            .copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        for equipment in 0_u8..5 {
            let offset = slot_start + 0x4C + usize::from(equipment) * 0x10;
            assert_eq!(
                &payload[offset..offset + 8],
                &[
                    equipment + 1,
                    0x70,
                    0x30,
                    0x20 + equipment,
                    0xD0,
                    0xC0 + equipment,
                    0xB0,
                    0xA0 + equipment,
                ],
                "card equipment {equipment}"
            );
        }
        assert_eq!(
            &payload[slot_start + 0x110..slot_start + 0x118],
            &[0x44, 0x33, 0x22, 0x11, 0x88, 0x77, 0x66, 0x55]
        );
    }

    #[test]
    fn does_not_apply_card_slot_schema_to_the_received_card_metadata_region() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        let last_slot_start = 97 * 0xE00;
        body[last_slot_start + 0x4C..last_slot_start + 0x54]
            .copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        body[last_slot_start + 0x110..last_slot_start + 0x118]
            .copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);

        // The Wii U lookup scans 0x62 (98) 0xe00-byte card slots. The next
        // region, beginning at logical slot 98, contains 0x38-byte summary
        // records rather than another guild card. Applying equipment, color,
        // weapon-use, or monster-log field boundaries there corrupts metadata.
        let metadata_start = 98 * 0xE00;
        let equipment_shaped_offset = metadata_start + 0x4C;
        let color_shaped_offset = metadata_start + 0x110;
        let weapon_shaped_offset = metadata_start + 0x12C;
        let monster_shaped_offset = metadata_start + 0x7C0;
        body[equipment_shaped_offset..equipment_shaped_offset + 8]
            .copy_from_slice(&[21, 22, 23, 24, 25, 26, 27, 28]);
        body[color_shaped_offset..color_shaped_offset + 8]
            .copy_from_slice(&[31, 32, 33, 34, 35, 36, 37, 38]);
        body[weapon_shaped_offset..weapon_shaped_offset + 8]
            .copy_from_slice(&[41, 42, 43, 44, 45, 46, 47, 48]);
        body[monster_shaped_offset..monster_shaped_offset + 8]
            .copy_from_slice(&[51, 52, 53, 54, 55, 56, 57, 58]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[last_slot_start + 0x4C..last_slot_start + 0x54],
            &[1, 2, 4, 3, 8, 7, 6, 5]
        );
        assert_eq!(
            &payload[last_slot_start + 0x110..last_slot_start + 0x118],
            &[12, 11, 10, 9, 16, 15, 14, 13]
        );
        assert_eq!(
            &payload[equipment_shaped_offset..equipment_shaped_offset + 8],
            &[21, 22, 23, 24, 25, 26, 27, 28]
        );
        assert_eq!(
            &payload[color_shaped_offset..color_shaped_offset + 8],
            // The first u32 stays opaque. The second happens to be summary
            // record 4's friendship score and follows trailer semantics.
            &[31, 32, 33, 34, 38, 37, 36, 35]
        );
        assert_eq!(
            &payload[weapon_shaped_offset..weapon_shaped_offset + 8],
            &[41, 42, 43, 44, 45, 46, 47, 48]
        );
        assert_eq!(
            &payload[monster_shaped_offset..monster_shaped_offset + 8],
            &[51, 52, 53, 54, 55, 56, 57, 58]
        );
    }

    #[test]
    fn remaps_exactly_the_33_received_card_friendship_scores() {
        let mut source = vec![0_u8; JP_3DS_HEADER.len() + CARD_PAYLOAD_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let body = &mut source[JP_3DS_HEADER.len()..];

        // The trailer has a 12-byte header followed by exactly 33 0x38-byte
        // summary records. Records 30 and 32 are beyond the sparse static
        // friendship table and must still convert; record 33 is an out-of-table
        // sentinel.
        let summary_start = 98 * 0xE00 + 0x0C;
        let record_30_score = summary_start + 30 * 0x38 + 0x28;
        let record_32_score = summary_start + 32 * 0x38 + 0x28;
        let record_33_score = summary_start + 33 * 0x38 + 0x28;
        body[record_30_score..record_30_score + 4].copy_from_slice(&[0x10, 0x0F, 0x00, 0x00]);
        body[record_32_score..record_32_score + 4].copy_from_slice(&[0x32, 0x10, 0x00, 0x00]);
        body[record_33_score..record_33_score + 4].copy_from_slice(&[0x21, 0x43, 0x65, 0x87]);

        let output = convert_external_component_to_cemu_named(&source, "card1").unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[record_30_score..record_30_score + 4],
            &[0x00, 0x00, 0x0F, 0x10]
        );
        assert_eq!(
            &payload[record_32_score..record_32_score + 4],
            &[0x00, 0x00, 0x10, 0x32]
        );
        assert_eq!(
            &payload[record_33_score..record_33_score + 4],
            &[0x21, 0x43, 0x65, 0x87]
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
