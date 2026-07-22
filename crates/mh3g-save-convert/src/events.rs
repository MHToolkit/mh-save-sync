use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{
    ConversionError,
    profile::{JP_3DS_HEADER, JP_CEMU_HEADER, PAYLOAD_SIZE, SaveProfile, inspect_bytes},
};

const EVENT_CATALOG_JSON: &str = include_str!("../data/event_catalog.json");

pub const SIMPLE_EVENT_START: usize = 0x62AE;
pub const SIMPLE_EVENT_WORDS: usize = 58;
pub const SIMPLE_EVENT_COUNT: usize = SIMPLE_EVENT_WORDS * 16;
pub const CATEGORY_EVENT_START: usize = 0x668C;
pub const CATEGORY_EVENT_LEN: usize = 0x7D0;
pub const CATEGORY_EVENT_BASES: [usize; 20] = [
    0x000, 0x000, 0x08C, 0x10E, 0x190, 0x212, 0x2A8, 0x2B2, 0x320, 0x38E, 0x3FC, 0x46A, 0x4A6,
    0x4E2, 0x51E, 0x582, 0x5E6, 0x64A, 0x6AE, 0x712,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EventDefinition {
    event_id: u16,
    domain: Option<String>,
    semantic_hint: Option<String>,
    three_ds_call_sites: Vec<String>,
    wiiu_call_sites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleEventState {
    pub event_id: u16,
    pub set: bool,
    pub domain: Option<String>,
    pub semantic_hint: Option<String>,
    pub three_ds_call_sites: Vec<String>,
    pub wiiu_call_sites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorizedEventState {
    pub category: u8,
    pub offset: u16,
    pub bit: u8,
    pub set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSnapshot {
    pub simple: Vec<SimpleEventState>,
    pub categorized: Vec<CategorizedEventState>,
}

fn event_catalog() -> &'static [EventDefinition] {
    static CATALOG: OnceLock<Vec<EventDefinition>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(EVENT_CATALOG_JSON)
                .expect("embedded MH3G event catalog must be valid JSON")
        })
        .as_slice()
}

pub fn preserve_event_state(
    source_payload: &[u8],
    target_payload: &mut [u8],
) -> Result<(), ConversionError> {
    if source_payload.len() != PAYLOAD_SIZE || target_payload.len() != PAYLOAD_SIZE {
        return Err(ConversionError::InvalidSave(format!(
            "MH3G event conversion requires {PAYLOAD_SIZE}-byte payloads"
        )));
    }

    for word in 0..SIMPLE_EVENT_WORDS {
        let offset = SIMPLE_EVENT_START + word * 2;
        target_payload[offset..offset + 2].copy_from_slice(&source_payload[offset..offset + 2]);
        target_payload[offset..offset + 2].reverse();
    }
    target_payload[CATEGORY_EVENT_START..CATEGORY_EVENT_START + CATEGORY_EVENT_LEN]
        .copy_from_slice(
            &source_payload[CATEGORY_EVENT_START..CATEGORY_EVENT_START + CATEGORY_EVENT_LEN],
        );

    Ok(())
}

pub fn event_snapshot(save: &[u8], include_unset: bool) -> Result<EventSnapshot, ConversionError> {
    let profile = inspect_bytes(save)?.profile;
    let (payload, little_endian) = match profile {
        SaveProfile::JpThreeDs => (&save[JP_3DS_HEADER.len()..], true),
        SaveProfile::JpCemu => (&save[JP_CEMU_HEADER.len()..], false),
        SaveProfile::JpThreeDsSystem | SaveProfile::JpCemuSystem => {
            return Err(ConversionError::InvalidSave(
                "event progress is stored in a character slot, not system data".to_owned(),
            ));
        }
    };

    let mut simple = Vec::new();
    for definition in event_catalog() {
        let event_id = usize::from(definition.event_id);
        let offset = SIMPLE_EVENT_START + event_id / 16 * 2;
        let bytes: [u8; 2] = payload[offset..offset + 2]
            .try_into()
            .expect("validated simple event word");
        let word = if little_endian {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        };
        let set = word & (1_u16 << (event_id % 16)) != 0;
        if include_unset || set {
            simple.push(SimpleEventState {
                event_id: definition.event_id,
                set,
                domain: definition.domain.clone(),
                semantic_hint: definition.semantic_hint.clone(),
                three_ds_call_sites: definition.three_ds_call_sites.clone(),
                wiiu_call_sites: definition.wiiu_call_sites.clone(),
            });
        }
    }

    let mut categorized = Vec::new();
    for (category, start) in CATEGORY_EVENT_BASES.iter().copied().enumerate().skip(1) {
        let end = CATEGORY_EVENT_BASES
            .get(category + 1)
            .copied()
            .unwrap_or(CATEGORY_EVENT_LEN);
        for relative_offset in 0..end - start {
            let byte = payload[CATEGORY_EVENT_START + start + relative_offset];
            for bit in 0_u8..8 {
                let set = byte & (1_u8 << bit) != 0;
                if include_unset || set {
                    categorized.push(CategorizedEventState {
                        category: category as u8,
                        offset: relative_offset as u16,
                        bit,
                        set,
                    });
                }
            }
        }
    }

    Ok(EventSnapshot {
        simple,
        categorized,
    })
}
