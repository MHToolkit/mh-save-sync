use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{
    ConversionError,
    profile::{JP_3DS_HEADER, JP_CEMU_HEADER, PAYLOAD_SIZE, SaveProfile, inspect_bytes},
};

const QUEST_CATALOG_JSON: &str = include_str!("../data/quest_catalog.json");
pub const QUEST_COMPLETION_START: usize = 0x6E5C;
const QUEST_COMPLETION_WORDS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestDefinition {
    pub table_index: usize,
    pub source_table_index: usize,
    pub target_table_index: usize,
    pub quest_id: u16,
    pub file: String,
    pub title_en: Option<String>,
    pub objective_en: Option<String>,
    pub area: String,
    pub star: Option<u8>,
    pub urgent: bool,
    pub key: Option<bool>,
    pub kind: String,
    pub completion_word: usize,
    pub completion_bit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestProgress {
    #[serde(flatten)]
    pub quest: QuestDefinition,
    pub completed: bool,
}

pub fn quest_catalog() -> &'static [QuestDefinition] {
    static CATALOG: OnceLock<Vec<QuestDefinition>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(QUEST_CATALOG_JSON)
                .expect("embedded MH3G quest catalog must be valid JSON")
        })
        .as_slice()
}

pub fn remap_quest_completion(
    source_payload: &[u8],
    target_payload: &mut [u8],
) -> Result<(), ConversionError> {
    if source_payload.len() != PAYLOAD_SIZE || target_payload.len() != PAYLOAD_SIZE {
        return Err(ConversionError::InvalidSave(format!(
            "MH3G quest remap requires {PAYLOAD_SIZE}-byte payloads"
        )));
    }

    let mut target_words = [0_u32; QUEST_COMPLETION_WORDS];
    for quest in quest_catalog() {
        let source_word_index = quest.source_table_index / 32;
        let source_bit = quest.source_table_index % 32;
        let source_offset = QUEST_COMPLETION_START + source_word_index * 4;
        let source_word = u32::from_le_bytes(
            source_payload[source_offset..source_offset + 4]
                .try_into()
                .expect("validated quest completion word"),
        );
        if source_word & (1_u32 << source_bit) != 0 {
            target_words[quest.target_table_index / 32] |= 1_u32 << (quest.target_table_index % 32);
        }
    }

    for (index, word) in target_words.into_iter().enumerate() {
        let offset = QUEST_COMPLETION_START + index * 4;
        target_payload[offset..offset + 4].copy_from_slice(&word.to_be_bytes());
    }

    Ok(())
}

pub fn quest_progress(save: &[u8]) -> Result<Vec<QuestProgress>, ConversionError> {
    let profile = inspect_bytes(save)?.profile;
    let (payload, little_endian) = match profile {
        SaveProfile::JpThreeDs => (&save[JP_3DS_HEADER.len()..], true),
        SaveProfile::JpCemu => (&save[JP_CEMU_HEADER.len()..], false),
        SaveProfile::JpThreeDsSystem | SaveProfile::JpCemuSystem => {
            return Err(ConversionError::InvalidSave(
                "quest progress is stored in a character slot, not system data".to_owned(),
            ));
        }
    };

    quest_catalog()
        .iter()
        .map(|quest| {
            let table_index = if little_endian {
                quest.source_table_index
            } else {
                quest.target_table_index
            };
            let completion_word = table_index / 32;
            let completion_bit = table_index % 32;
            let offset = QUEST_COMPLETION_START + completion_word * 4;
            let bytes: [u8; 4] = payload
                .get(offset..offset + 4)
                .ok_or_else(|| {
                    ConversionError::InvalidSave(format!(
                        "quest completion word {} is outside the save payload",
                        completion_word
                    ))
                })?
                .try_into()
                .expect("validated four-byte quest completion word");
            let word = if little_endian {
                u32::from_le_bytes(bytes)
            } else {
                u32::from_be_bytes(bytes)
            };

            Ok(QuestProgress {
                quest: quest.clone(),
                completed: word & (1_u32 << completion_bit) != 0,
            })
        })
        .collect()
}
