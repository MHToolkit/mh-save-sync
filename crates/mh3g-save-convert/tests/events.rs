use mh3g_save_convert::{
    converter::convert_3ds_to_cemu,
    events::{CATEGORY_EVENT_BASES, CATEGORY_EVENT_START, SIMPLE_EVENT_START, event_snapshot},
    profile::{JP_3DS_HEADER, THREE_DS_SIZE},
};

#[test]
fn conversion_preserves_simple_and_categorized_event_state_semantically() {
    let mut source = vec![0_u8; THREE_DS_SIZE];
    source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
    let payload = &mut source[JP_3DS_HEADER.len()..];
    for event_id in [0_usize, 15, 16, 468, 927] {
        let offset = SIMPLE_EVENT_START + event_id / 16 * 2;
        let mut word = u16::from_le_bytes(payload[offset..offset + 2].try_into().unwrap());
        word |= 1_u16 << (event_id % 16);
        payload[offset..offset + 2].copy_from_slice(&word.to_le_bytes());
    }
    for (category, offset, bit) in [(1_usize, 0_usize, 0_u8), (19, 0xBD, 7)] {
        payload[CATEGORY_EVENT_START + CATEGORY_EVENT_BASES[category] + offset] |= 1 << bit;
    }

    let target = convert_3ds_to_cemu(&source).unwrap();
    let source_events = event_snapshot(&source, false).unwrap();
    let target_events = event_snapshot(&target, false).unwrap();

    assert_eq!(source_events.simple, target_events.simple);
    assert_eq!(source_events.categorized, target_events.categorized);
}

#[test]
fn event_catalog_attaches_static_farm_evidence_to_event_468() {
    let mut source = vec![0_u8; THREE_DS_SIZE];
    source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
    let snapshot = event_snapshot(&source, true).unwrap();
    let event = snapshot
        .simple
        .iter()
        .find(|event| event.event_id == 468)
        .unwrap();

    assert_eq!(event.domain.as_deref(), Some("farm"));
    assert_eq!(event.semantic_hint.as_deref(), Some("farm UI state branch"));
    assert!(event.wiiu_call_sites.contains(&"0x02693618".to_owned()));
}
