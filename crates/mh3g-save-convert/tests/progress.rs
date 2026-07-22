use mh3g_save_convert::{
    converter::convert_3ds_to_cemu,
    profile::{JP_3DS_HEADER, PAYLOAD_SIZE, THREE_DS_SIZE},
    progress::{quest_catalog, quest_progress, remap_quest_completion},
    transforms::QUEST_COMPLETION_START,
};

#[test]
fn catalog_identifies_bear_trap_as_the_village_two_star_key_quest() {
    let quest = quest_catalog()
        .iter()
        .find(|quest| quest.quest_id == 1204)
        .unwrap();

    assert_eq!(quest.table_index, 10);
    assert_eq!(quest.source_table_index, 10);
    assert_eq!(quest.target_table_index, 10);
    assert_eq!(quest.title_en.as_deref(), Some("Bear Trap"));
    assert_eq!(quest.objective_en.as_deref(), Some("Capture an Arzuros"));
    assert_eq!(quest.area, "village");
    assert_eq!(quest.star, Some(2));
    assert_eq!(quest.key, Some(true));
    assert!(!quest.urgent);
    assert_eq!(quest.completion_word, 0);
    assert_eq!(quest.completion_bit, 10);
}

#[test]
fn quest_completion_remap_uses_quest_ids_across_word_boundaries() {
    let mut source = vec![0_u8; PAYLOAD_SIZE];
    let mut target = vec![0xA5_u8; PAYLOAD_SIZE];
    for quest_id in [1101_u16, 1404, 1405, 20312] {
        let quest = quest_catalog()
            .iter()
            .find(|quest| quest.quest_id == quest_id)
            .unwrap();
        let offset = QUEST_COMPLETION_START + (quest.source_table_index / 32) * 4;
        let mut word = u32::from_le_bytes(source[offset..offset + 4].try_into().unwrap());
        word |= 1_u32 << (quest.source_table_index % 32);
        source[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    remap_quest_completion(&source, &mut target).unwrap();

    for quest in quest_catalog() {
        let source_offset = QUEST_COMPLETION_START + (quest.source_table_index / 32) * 4;
        let source_word =
            u32::from_le_bytes(source[source_offset..source_offset + 4].try_into().unwrap());
        let target_offset = QUEST_COMPLETION_START + (quest.target_table_index / 32) * 4;
        let target_word =
            u32::from_be_bytes(target[target_offset..target_offset + 4].try_into().unwrap());
        assert_eq!(
            source_word & (1_u32 << (quest.source_table_index % 32)) != 0,
            target_word & (1_u32 << (quest.target_table_index % 32)) != 0,
            "quest {}",
            quest.quest_id
        );
    }
}

#[test]
fn quest_progress_reads_identical_logical_bits_from_3ds_and_converted_cemu_saves() {
    let mut source = vec![0_u8; THREE_DS_SIZE];
    source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
    let logical_word = (1_u32 << 0) | (1_u32 << 10) | (1_u32 << 31);
    let offset = JP_3DS_HEADER.len() + QUEST_COMPLETION_START;
    source[offset..offset + 4].copy_from_slice(&logical_word.to_le_bytes());

    let cemu = convert_3ds_to_cemu(&source).unwrap();
    let source_progress = quest_progress(&source).unwrap();
    let cemu_progress = quest_progress(&cemu).unwrap();

    for quest_id in [1101, 1204] {
        assert!(
            source_progress
                .iter()
                .find(|quest| quest.quest.quest_id == quest_id)
                .unwrap()
                .completed
        );
        assert!(
            cemu_progress
                .iter()
                .find(|quest| quest.quest.quest_id == quest_id)
                .unwrap()
                .completed
        );
    }
    assert_eq!(source.len(), PAYLOAD_SIZE + JP_3DS_HEADER.len());
}
