use serde::{Deserialize, Serialize};

use crate::{
    ConversionError,
    converter::{
        convert_3ds_to_cemu_named, convert_3ds_to_cemu_named_for_revision,
        convert_external_component_to_cemu_named,
        convert_external_component_to_cemu_named_for_revision,
        validate_cemu_external_component_named,
    },
    profile::{JP_CEMU_HEADER, SaveProfile, inspect_bytes, validate_slot_path},
    revision::ConverterRevision,
    transaction::sha256_hex,
};

const USER_ARENA_RECORD_START: usize = 0x83A8;
const USER_ARENA_RECORD_COUNT: usize = 110;
const ARENA_RECORD_STRIDE: usize = 4;
const SHAKALAKA_RECORD_START: usize = 0x6F44;
const SHAKALAKA_RECORD_COUNT: usize = 2;
const SHAKALAKA_RECORD_STRIDE: usize = 0x148;
const SHAKALAKA_U32_PREFIX_SIZE: usize = 0x04;
const SHAKALAKA_MASK_STATE_START: usize = 0xDE;
const SHAKALAKA_MASK_STATE_END: usize = 0x140;
#[cfg(test)]
const HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET: usize = 0xE4;
const USER_MONSTER_LOG_START: usize = 0x81B4;
const USER_MONSTER_LOG_COUNT: usize = 50;
const USER_MONSTER_LOG_STRIDE: usize = 10;
const GUILD_CARD_SLOT_SIZE: usize = 0xE00;
const GUILD_CARD_SLOT_COUNT: usize = 0x62;
const GUILD_CARD_ARENA_RECORD_START: usize = 0x9B4;
const GUILD_CARD_ARENA_RECORD_COUNT: usize = 110;
const GUILD_CARD_MONSTER_LOG_START: usize = 0x7C0;
const GUILD_CARD_MONSTER_LOG_COUNT: usize = 50;
const GUILD_CARD_MONSTER_LOG_STRIDE: usize = 10;
const USER_MONSTER_GUIDE_RECORD_START: usize = 0x65C4;
const USER_MONSTER_GUIDE_RECORD_COUNT: usize = 48;
const USER_MONSTER_GUIDE_RECORD_STRIDE: usize = 4;
const USER_APPEARANCE_SCALAR_OFFSETS: [usize; 3] = [0x73B8, 0x73BC, 0x73C8];
const USER_APPEARANCE_PACKED_STYLE_OFFSET: usize = 0x73D0;
const USER_APPEARANCE_RGBA_OFFSET: usize = 0x73D8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetectionConfidence {
    Exact,
    CompatibleRange,
    Ambiguous,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionScore {
    pub revision: ConverterRevision,
    pub matching_fields: usize,
    pub historical_matches: usize,
    pub already_current: usize,
    pub conflicting_fields: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionDetection {
    pub confidence: DetectionConfidence,
    pub candidates: Vec<ConverterRevision>,
    pub scores: Vec<RevisionScore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeFieldStatus {
    Repaired,
    AlreadyCurrent,
    PreservedConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeField {
    pub name: String,
    pub offset: usize,
    pub width: usize,
    pub status: MergeFieldStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityMerge {
    pub component: String,
    pub assumed_revision: ConverterRevision,
    pub source_sha256: String,
    pub current_sha256: String,
    pub merged_sha256: String,
    pub repaired_fields: usize,
    pub already_current_fields: usize,
    pub preserved_conflicts: usize,
    pub fields: Vec<MergeField>,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct FieldSpec {
    name: String,
    offset: usize,
    width: usize,
}

pub fn detect_component_revision(
    source: &[u8],
    current: &[u8],
    filename: &str,
) -> Result<RevisionDetection, ConversionError> {
    let expected = historical_outputs(source, current, filename)?;
    let latest = convert_component_current(source, filename)?;
    let fields = repair_fields(filename)?;
    if fields.is_empty() {
        return Ok(RevisionDetection {
            confidence: DetectionConfidence::CompatibleRange,
            candidates: ConverterRevision::ALL.to_vec(),
            scores: ConverterRevision::ALL
                .into_iter()
                .map(|revision| RevisionScore {
                    revision,
                    matching_fields: 0,
                    historical_matches: 0,
                    already_current: 0,
                    conflicting_fields: 0,
                })
                .collect(),
        });
    }

    let scores = ConverterRevision::ALL
        .into_iter()
        .zip(expected.iter())
        .map(|(revision, candidate)| {
            let mut score = RevisionScore {
                revision,
                matching_fields: 0,
                historical_matches: 0,
                already_current: 0,
                conflicting_fields: 0,
            };
            for field in &fields {
                let current_field = field_bytes(current, field);
                let candidate_field = field_bytes(candidate, field);
                let latest_field = field_bytes(&latest, field);
                if current_field == candidate_field {
                    score.matching_fields += 1;
                    if candidate_field != latest_field {
                        score.historical_matches += 1;
                    } else {
                        score.already_current += 1;
                    }
                } else if current_field == latest_field {
                    score.already_current += 1;
                } else {
                    score.conflicting_fields += 1;
                }
            }
            score
        })
        .collect::<Vec<_>>();

    let best_match = scores
        .iter()
        .map(|score| score.matching_fields)
        .max()
        .unwrap_or(0);
    let best = scores
        .iter()
        .filter(|score| score.matching_fields == best_match)
        .map(|score| score.revision)
        .collect::<Vec<_>>();
    let any_supported_field = scores
        .iter()
        .any(|score| score.matching_fields > 0 || score.already_current > 0);
    let confidence = if !any_supported_field {
        DetectionConfidence::Unknown
    } else if best.len() == 1 {
        DetectionConfidence::Exact
    } else {
        let tied_merges_identical = best.windows(2).all(|pair| {
            let left = &expected[revision_index(pair[0])];
            let right = &expected[revision_index(pair[1])];
            merged_candidate_bytes(current, left, &latest, &fields)
                == merged_candidate_bytes(current, right, &latest, &fields)
        });
        if tied_merges_identical {
            DetectionConfidence::CompatibleRange
        } else {
            DetectionConfidence::Ambiguous
        }
    };

    Ok(RevisionDetection {
        confidence,
        candidates: best,
        scores,
    })
}

pub fn combine_revision_detections(detections: &[RevisionDetection]) -> RevisionDetection {
    let scores = ConverterRevision::ALL
        .into_iter()
        .map(|revision| {
            detections
                .iter()
                .filter_map(|detection| {
                    detection
                        .scores
                        .iter()
                        .find(|score| score.revision == revision)
                })
                .fold(
                    RevisionScore {
                        revision,
                        matching_fields: 0,
                        historical_matches: 0,
                        already_current: 0,
                        conflicting_fields: 0,
                    },
                    |mut total, score| {
                        total.matching_fields += score.matching_fields;
                        total.historical_matches += score.historical_matches;
                        total.already_current += score.already_current;
                        total.conflicting_fields += score.conflicting_fields;
                        total
                    },
                )
        })
        .collect::<Vec<_>>();
    let informative = detections
        .iter()
        .filter(|detection| detection.confidence != DetectionConfidence::Unknown)
        .collect::<Vec<_>>();
    if informative.is_empty() {
        return RevisionDetection {
            confidence: DetectionConfidence::Unknown,
            candidates: ConverterRevision::ALL.to_vec(),
            scores,
        };
    }

    let common = ConverterRevision::ALL
        .into_iter()
        .filter(|revision| {
            informative
                .iter()
                .all(|detection| detection.candidates.contains(revision))
        })
        .collect::<Vec<_>>();
    let (confidence, candidates) = match common.as_slice() {
        [revision] => (DetectionConfidence::Exact, vec![*revision]),
        [] => {
            let candidates = ConverterRevision::ALL
                .into_iter()
                .filter(|revision| {
                    informative
                        .iter()
                        .any(|detection| detection.candidates.contains(revision))
                })
                .collect();
            (DetectionConfidence::Ambiguous, candidates)
        }
        _ => {
            let compatible = informative.iter().all(|detection| {
                detection.confidence == DetectionConfidence::CompatibleRange
                    && common
                        .iter()
                        .all(|revision| detection.candidates.contains(revision))
            });
            (
                if compatible {
                    DetectionConfidence::CompatibleRange
                } else {
                    DetectionConfidence::Ambiguous
                },
                common,
            )
        }
    };

    RevisionDetection {
        confidence,
        candidates,
        scores,
    }
}

pub fn merge_component(
    source: &[u8],
    current: &[u8],
    filename: &str,
    assumed_revision: ConverterRevision,
) -> Result<CompatibilityMerge, ConversionError> {
    validate_component_pair(source, current, filename)?;
    let historical = convert_component_for_revision(source, filename, assumed_revision)?;
    let latest = convert_component_current(source, filename)?;
    let fields = repair_fields(filename)?;
    let mut bytes = current.to_vec();
    let mut changes = Vec::new();

    for field in fields {
        let old = field_bytes(&historical, &field);
        let fixed = field_bytes(&latest, &field);
        if old == fixed {
            continue;
        }
        let observed = field_bytes(current, &field);
        let status = if observed == old {
            bytes[field.offset..field.offset + field.width].copy_from_slice(fixed);
            MergeFieldStatus::Repaired
        } else if observed == fixed {
            MergeFieldStatus::AlreadyCurrent
        } else {
            MergeFieldStatus::PreservedConflict
        };
        changes.push(MergeField {
            name: field.name,
            offset: field.offset,
            width: field.width,
            status,
        });
    }

    validate_current_component(&bytes, filename)?;
    Ok(CompatibilityMerge {
        component: filename.to_owned(),
        assumed_revision,
        source_sha256: sha256_hex(source),
        current_sha256: sha256_hex(current),
        merged_sha256: sha256_hex(&bytes),
        repaired_fields: changes
            .iter()
            .filter(|field| field.status == MergeFieldStatus::Repaired)
            .count(),
        already_current_fields: changes
            .iter()
            .filter(|field| field.status == MergeFieldStatus::AlreadyCurrent)
            .count(),
        preserved_conflicts: changes
            .iter()
            .filter(|field| field.status == MergeFieldStatus::PreservedConflict)
            .count(),
        fields: changes,
        bytes,
    })
}

fn historical_outputs(
    source: &[u8],
    current: &[u8],
    filename: &str,
) -> Result<Vec<Vec<u8>>, ConversionError> {
    validate_component_pair(source, current, filename)?;
    ConverterRevision::ALL
        .into_iter()
        .map(|revision| convert_component_for_revision(source, filename, revision))
        .collect()
}

fn convert_component_for_revision(
    source: &[u8],
    filename: &str,
    revision: ConverterRevision,
) -> Result<Vec<u8>, ConversionError> {
    match filename {
        "user1" | "user2" | "user3" => {
            convert_3ds_to_cemu_named_for_revision(source, filename, revision)
        }
        "card1" | "card2" | "card3" | "cardbox" | "quest1" | "quest2" | "quest3" | "quest4" => {
            convert_external_component_to_cemu_named_for_revision(source, filename, revision)
        }
        _ => Err(ConversionError::InvalidSave(format!(
            "unsupported compatibility component: {filename}"
        ))),
    }
}

fn convert_component_current(source: &[u8], filename: &str) -> Result<Vec<u8>, ConversionError> {
    match filename {
        "user1" | "user2" | "user3" => convert_3ds_to_cemu_named(source, filename),
        "card1" | "card2" | "card3" | "cardbox" | "quest1" | "quest2" | "quest3" | "quest4" => {
            convert_external_component_to_cemu_named(source, filename)
        }
        _ => Err(ConversionError::InvalidSave(format!(
            "unsupported compatibility component: {filename}"
        ))),
    }
}

fn validate_component_pair(
    source: &[u8],
    current: &[u8],
    filename: &str,
) -> Result<(), ConversionError> {
    match filename {
        "user1" | "user2" | "user3" => {
            validate_slot_path(std::path::Path::new(filename))?;
            let source_profile = inspect_bytes(source)?.profile;
            let current_profile = inspect_bytes(current)?.profile;
            if source_profile != SaveProfile::JpThreeDs || current_profile != SaveProfile::JpCemu {
                return Err(ConversionError::InvalidSave(format!(
                    "compatibility merge requires Japanese 3DS and Cemu slot profiles for {filename}"
                )));
            }
        }
        "card1" | "card2" | "card3" | "cardbox" | "quest1" | "quest2" | "quest3" | "quest4" => {
            // Conversion validates the 3DS side, including its component size.
            convert_component_current(source, filename)?;
            validate_cemu_external_component_named(current, filename)?;
        }
        _ => {
            return Err(ConversionError::InvalidSave(format!(
                "unsupported compatibility component: {filename}"
            )));
        }
    }
    Ok(())
}

fn validate_current_component(bytes: &[u8], filename: &str) -> Result<(), ConversionError> {
    match filename {
        "user1" | "user2" | "user3" => {
            if inspect_bytes(bytes)?.profile != SaveProfile::JpCemu {
                return Err(ConversionError::InvalidSave(format!(
                    "merged {filename} is not a Japanese Cemu slot"
                )));
            }
            Ok(())
        }
        _ => validate_cemu_external_component_named(bytes, filename),
    }
}

fn repair_fields(filename: &str) -> Result<Vec<FieldSpec>, ConversionError> {
    let header = JP_CEMU_HEADER.len();
    let mut fields = Vec::new();
    match filename {
        "user1" | "user2" | "user3" => {
            for record in 0..USER_MONSTER_GUIDE_RECORD_COUNT {
                fields.push(FieldSpec {
                    name: format!("monster-guide-record-{record}"),
                    offset: header
                        + USER_MONSTER_GUIDE_RECORD_START
                        + record * USER_MONSTER_GUIDE_RECORD_STRIDE,
                    width: USER_MONSTER_GUIDE_RECORD_STRIDE,
                });
            }
            for (index, offset) in USER_APPEARANCE_SCALAR_OFFSETS.into_iter().enumerate() {
                fields.push(FieldSpec {
                    name: format!("player-appearance-scalar-{index}"),
                    offset: header + offset,
                    width: 4,
                });
            }
            fields.push(FieldSpec {
                name: "player-appearance-packed-style".to_owned(),
                offset: header + USER_APPEARANCE_PACKED_STYLE_OFFSET,
                width: 4,
            });
            fields.push(FieldSpec {
                name: "player-appearance-rgba".to_owned(),
                offset: header + USER_APPEARANCE_RGBA_OFFSET,
                width: 4,
            });
            for record in 0..USER_ARENA_RECORD_COUNT {
                fields.push(FieldSpec {
                    name: format!("personal-arena-{record}"),
                    offset: header + USER_ARENA_RECORD_START + record * ARENA_RECORD_STRIDE,
                    width: 4,
                });
            }
            for record in 0..USER_MONSTER_LOG_COUNT {
                fields.push(FieldSpec {
                    name: format!("personal-monster-state-{record}"),
                    offset: header + USER_MONSTER_LOG_START + record * USER_MONSTER_LOG_STRIDE + 8,
                    width: 1,
                });
            }
            for companion in 0..SHAKALAKA_RECORD_COUNT {
                let start = header + SHAKALAKA_RECORD_START + companion * SHAKALAKA_RECORD_STRIDE;
                fields.push(FieldSpec {
                    name: format!("shakalaka-{companion}-u32-prefix"),
                    offset: start,
                    width: SHAKALAKA_U32_PREFIX_SIZE,
                });
                for relative in (SHAKALAKA_U32_PREFIX_SIZE..SHAKALAKA_MASK_STATE_START).step_by(2) {
                    fields.push(FieldSpec {
                        name: format!("shakalaka-{companion}-scalar-{relative:03x}"),
                        offset: start + relative,
                        width: 2,
                    });
                }
                for relative in (SHAKALAKA_MASK_STATE_START..SHAKALAKA_MASK_STATE_END).step_by(2) {
                    fields.push(FieldSpec {
                        name: format!("shakalaka-{companion}-packed-mask-pair-{relative:03x}"),
                        offset: start + relative,
                        width: 2,
                    });
                }
            }
        }
        "card1" | "card2" | "card3" => {
            for slot in 0..GUILD_CARD_SLOT_COUNT {
                let start = header + slot * GUILD_CARD_SLOT_SIZE;
                for record in 0..GUILD_CARD_ARENA_RECORD_COUNT {
                    fields.push(FieldSpec {
                        name: format!("received-card-{slot}-arena-{record}"),
                        offset: start
                            + GUILD_CARD_ARENA_RECORD_START
                            + record * ARENA_RECORD_STRIDE,
                        width: 4,
                    });
                }
                for record in 0..GUILD_CARD_MONSTER_LOG_COUNT {
                    fields.push(FieldSpec {
                        name: format!("received-card-{slot}-monster-state-{record}"),
                        offset: start
                            + GUILD_CARD_MONSTER_LOG_START
                            + record * GUILD_CARD_MONSTER_LOG_STRIDE
                            + 8,
                        width: 1,
                    });
                }
            }
        }
        "cardbox" | "quest1" | "quest2" | "quest3" | "quest4" => {}
        _ => {
            return Err(ConversionError::InvalidSave(format!(
                "unsupported compatibility component: {filename}"
            )));
        }
    }
    Ok(fields)
}

fn field_bytes<'a>(bytes: &'a [u8], field: &FieldSpec) -> &'a [u8] {
    &bytes[field.offset..field.offset + field.width]
}

fn merged_candidate_bytes(
    current: &[u8],
    candidate: &[u8],
    latest: &[u8],
    fields: &[FieldSpec],
) -> Vec<u8> {
    let mut merged = current.to_vec();
    for field in fields {
        let old = field_bytes(candidate, field);
        let fixed = field_bytes(latest, field);
        if old != fixed && field_bytes(current, field) == old {
            merged[field.offset..field.offset + field.width].copy_from_slice(fixed);
        }
    }
    merged
}

const fn revision_index(revision: ConverterRevision) -> usize {
    match revision {
        ConverterRevision::V0_0_3 => 0,
        ConverterRevision::V0_0_4 => 1,
        ConverterRevision::V0_0_5 => 2,
        ConverterRevision::V0_0_6 => 3,
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::profile::{JP_3DS_HEADER, THREE_DS_SIZE};

    fn source() -> Vec<u8> {
        let mut bytes = (0..THREE_DS_SIZE)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        bytes
    }

    fn card_source() -> Vec<u8> {
        let mut bytes = vec![0_u8; JP_3DS_HEADER.len() + 0x57_FFC];
        bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        for (index, byte) in bytes[JP_3DS_HEADER.len()..].iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
        }
        bytes
    }

    #[test]
    fn replays_the_released_0_0_3_and_0_0_4_card_algorithms() {
        let source = card_source();
        let v003 = convert_external_component_to_cemu_named_for_revision(
            &source,
            "card2",
            ConverterRevision::V0_0_3,
        )
        .unwrap();
        let v004 = convert_external_component_to_cemu_named_for_revision(
            &source,
            "card2",
            ConverterRevision::V0_0_4,
        )
        .unwrap();

        assert_eq!(
            hex::encode(Sha256::digest(&v003[JP_CEMU_HEADER.len()..])),
            "857e91f9f7ec6adf1399480fe4409c1d29ec7b376be6d8f6b28dda3032d965f1"
        );
        assert_eq!(
            hex::encode(Sha256::digest(&v004[JP_CEMU_HEADER.len()..])),
            "9fce84bec2ff99d998747078228941e5dec2f18198ed4931b82b8a30efc00efb"
        );
    }

    #[test]
    fn repairs_the_historical_packed_mask_byte_swap_without_reverting_wiiu_progress() {
        let mut source = source();
        let source_record = JP_3DS_HEADER.len() + SHAKALAKA_RECORD_START;
        source[source_record..source_record + 12].copy_from_slice(&[
            0x19, 0xC2, 0x0A, 0x00, 0x2F, 0x13, 0x2F, 0x01, 0x2C, 0x01, 0x3E, 0x01,
        ]);
        let payload_offset =
            JP_3DS_HEADER.len() + SHAKALAKA_RECORD_START + HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET;
        source[payload_offset..payload_offset + 2].copy_from_slice(&[0x1E, 0x00]);
        let mut current =
            convert_3ds_to_cemu_named_for_revision(&source, "user2", ConverterRevision::V0_0_6)
                .unwrap();
        let unrelated = JP_CEMU_HEADER.len() + 0x240;
        current[unrelated] ^= 0x5A;

        let merged =
            merge_component(&source, &current, "user2", ConverterRevision::V0_0_6).unwrap();
        let lamp =
            JP_CEMU_HEADER.len() + SHAKALAKA_RECORD_START + HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET;
        let record = JP_CEMU_HEADER.len() + SHAKALAKA_RECORD_START;

        assert_eq!(
            &current[record + 4..record + 12],
            &[0x01, 0x2F, 0x13, 0x2F, 0x01, 0x3E, 0x01, 0x2C]
        );
        assert_eq!(
            &merged.bytes[record + 4..record + 12],
            &[0x13, 0x2F, 0x01, 0x2F, 0x01, 0x2C, 0x01, 0x3E]
        );
        assert_eq!(&current[lamp..lamp + 2], &[0x00, 0x1E]);
        assert_eq!(&merged.bytes[lamp..lamp + 2], &[0x1E, 0x00]);
        assert_eq!(merged.bytes[unrelated], current[unrelated]);
        assert!(merged.repaired_fields >= 1);
    }

    #[test]
    fn preserves_a_whole_multibyte_field_when_wiiu_changed_only_one_byte() {
        let mut source = source();
        let source_lamp =
            JP_3DS_HEADER.len() + SHAKALAKA_RECORD_START + HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET;
        source[source_lamp..source_lamp + 2].copy_from_slice(&[0x1E, 0x00]);
        let mut current =
            convert_3ds_to_cemu_named_for_revision(&source, "user2", ConverterRevision::V0_0_6)
                .unwrap();
        let lamp =
            JP_CEMU_HEADER.len() + SHAKALAKA_RECORD_START + HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET;
        current[lamp] = 0xAA;
        let observed = current[lamp..lamp + 2].to_vec();

        let merged =
            merge_component(&source, &current, "user2", ConverterRevision::V0_0_6).unwrap();

        assert_eq!(&merged.bytes[lamp..lamp + 2], observed.as_slice());
        assert!(merged.fields.iter().any(|field| {
            field.name == "shakalaka-0-packed-mask-pair-0e4"
                && field.status == MergeFieldStatus::PreservedConflict
        }));
    }

    #[test]
    fn compatibility_merge_is_idempotent() {
        let source = source();
        let current =
            convert_3ds_to_cemu_named_for_revision(&source, "user2", ConverterRevision::V0_0_3)
                .unwrap();
        let first = merge_component(&source, &current, "user2", ConverterRevision::V0_0_3).unwrap();
        let second =
            merge_component(&source, &first.bytes, "user2", ConverterRevision::V0_0_3).unwrap();

        assert_eq!(first.bytes, second.bytes);
        assert_eq!(second.repaired_fields, 0);
    }

    #[test]
    fn repairs_new_official_parity_fields_without_reverting_wiiu_progress() {
        let mut source = source();
        let source_guide = JP_3DS_HEADER.len() + USER_MONSTER_GUIDE_RECORD_START;
        source[source_guide..source_guide + 4].copy_from_slice(&0x1234_5678_u32.to_le_bytes());
        let source_appearance = JP_3DS_HEADER.len() + USER_APPEARANCE_RGBA_OFFSET;
        source[source_appearance..source_appearance + 4].copy_from_slice(&[0xFF, 0xE6, 0xEF, 0xFA]);
        let mut current =
            convert_3ds_to_cemu_named_for_revision(&source, "user2", ConverterRevision::V0_0_6)
                .unwrap();
        let unrelated = JP_CEMU_HEADER.len() + 0x240;
        current[unrelated] ^= 0x5A;

        let merged =
            merge_component(&source, &current, "user2", ConverterRevision::V0_0_6).unwrap();
        let guide = JP_CEMU_HEADER.len() + USER_MONSTER_GUIDE_RECORD_START;
        let appearance = JP_CEMU_HEADER.len() + USER_APPEARANCE_RGBA_OFFSET;

        assert_eq!(
            &merged.bytes[guide..guide + 4],
            &0x1234_5678_u32.to_be_bytes()
        );
        assert_eq!(
            &merged.bytes[appearance..appearance + 4],
            &[0xFA, 0xEF, 0xE6, 0xFF]
        );
        assert_eq!(merged.bytes[unrelated], current[unrelated]);
        assert!(merged.fields.iter().any(|field| {
            field.name == "monster-guide-record-0" && field.status == MergeFieldStatus::Repaired
        }));
        assert!(merged.fields.iter().any(|field| {
            field.name == "player-appearance-rgba" && field.status == MergeFieldStatus::Repaired
        }));
    }

    #[test]
    fn detects_each_unmodified_historical_core_output() {
        let source = source();
        for revision in ConverterRevision::ALL {
            let current =
                convert_3ds_to_cemu_named_for_revision(&source, "user2", revision).unwrap();
            let detection = detect_component_revision(&source, &current, "user2").unwrap();
            assert!(
                detection.candidates.contains(&revision),
                "{revision:?}: {detection:?}"
            );
        }
    }

    #[test]
    fn reports_a_compatible_range_when_the_component_has_no_revision_fields() {
        let mut source = vec![0; 0x29000];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        let current = convert_external_component_to_cemu_named_for_revision(
            &source,
            "quest1",
            ConverterRevision::V0_0_3,
        )
        .unwrap();

        let detection = detect_component_revision(&source, &current, "quest1").unwrap();

        assert_eq!(detection.confidence, DetectionConfidence::CompatibleRange);
        assert_eq!(detection.candidates, ConverterRevision::ALL);
    }

    #[test]
    fn reports_unknown_when_every_revision_field_contradicts_all_outputs() {
        let source = source();
        let latest = convert_component_current(&source, "user2").unwrap();
        let outputs = historical_outputs(&source, &latest, "user2").unwrap();
        let fields = repair_fields("user2").unwrap();
        let mut current = latest.clone();
        for field in &fields {
            let replacement = (0u8..=u8::MAX)
                .map(|byte| vec![byte; field.width])
                .find(|candidate| {
                    outputs
                        .iter()
                        .all(|output| field_bytes(output, field) != candidate)
                        && field_bytes(&latest, field) != candidate
                })
                .expect("historical and current values cannot exhaust all byte patterns");
            current[field.offset..field.offset + field.width].copy_from_slice(&replacement);
        }

        let detection = detect_component_revision(&source, &current, "user2").unwrap();

        assert_eq!(detection.confidence, DetectionConfidence::Unknown);
        assert!(detection.candidates.len() > 1);
    }

    #[test]
    fn reports_ambiguous_when_current_fields_support_conflicting_revisions() {
        let source = source();
        let latest = convert_component_current(&source, "user2").unwrap();
        let outputs = historical_outputs(&source, &latest, "user2").unwrap();
        let fields = repair_fields("user2").unwrap();
        let mut observed = None;

        'pairs: for left in 0..outputs.len() {
            for right in left + 1..outputs.len() {
                let differing = fields
                    .iter()
                    .filter(|field| {
                        field_bytes(&outputs[left], field) != field_bytes(&outputs[right], field)
                    })
                    .collect::<Vec<_>>();
                if differing.len() < 2 {
                    continue;
                }
                let mut current = latest.clone();
                for (index, field) in differing.iter().enumerate() {
                    let selected = if index % 2 == 0 { left } else { right };
                    current[field.offset..field.offset + field.width]
                        .copy_from_slice(field_bytes(&outputs[selected], field));
                }
                let detection = detect_component_revision(&source, &current, "user2").unwrap();
                if detection.confidence == DetectionConfidence::Ambiguous {
                    observed = Some(detection);
                    break 'pairs;
                }
            }
        }

        let detection = observed.expect("mixed historical fields should produce ambiguity");
        assert!(detection.candidates.len() > 1);
    }

    #[test]
    fn combines_components_into_one_common_revision() {
        let source = source();
        let v004 =
            convert_3ds_to_cemu_named_for_revision(&source, "user2", ConverterRevision::V0_0_4)
                .unwrap();
        let v005 =
            convert_3ds_to_cemu_named_for_revision(&source, "user2", ConverterRevision::V0_0_5)
                .unwrap();
        let left = detect_component_revision(&source, &v004, "user2").unwrap();
        let right = detect_component_revision(&source, &v005, "user2").unwrap();

        let combined = combine_revision_detections(&[left, right]);

        assert_eq!(combined.confidence, DetectionConfidence::Ambiguous);
        assert!(combined.candidates.contains(&ConverterRevision::V0_0_4));
        assert!(combined.candidates.contains(&ConverterRevision::V0_0_5));
    }
}
