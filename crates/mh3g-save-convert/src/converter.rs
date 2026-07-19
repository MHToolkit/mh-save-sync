use crate::{
    ConversionError,
    profile::{CEMU_SIZE, JP_3DS_HEADER, JP_CEMU_HEADER, SaveProfile, inspect_bytes},
    transforms::{apply_arena_records, apply_endian_swaps, apply_monster_discovery},
};

/// Convert one Japanese MH3G 3DS slot into the Japanese Cemu slot format.
///
/// The conversion is deliberately pure: the input is never modified and no
/// filesystem or emulator state is accessed.
pub fn convert_3ds_to_cemu(source: &[u8]) -> Result<Vec<u8>, ConversionError> {
    let inspection = inspect_bytes(source)?;
    if inspection.profile != SaveProfile::JpThreeDs {
        return Err(ConversionError::InvalidSave(format!(
            "expected a Japanese MH3G 3DS save with header {:02X?}",
            JP_3DS_HEADER
        )));
    }

    let mut payload = source[JP_3DS_HEADER.len()..].to_vec();
    apply_endian_swaps(&mut payload)?;
    apply_monster_discovery(&mut payload)?;
    apply_arena_records(&mut payload)?;

    let mut output = Vec::with_capacity(CEMU_SIZE);
    output.extend_from_slice(&JP_CEMU_HEADER);
    output.extend_from_slice(&payload);

    inspect_bytes(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use crate::{
        converter::convert_3ds_to_cemu,
        profile::{
            CEMU_SIZE, JP_3DS_HEADER, JP_CEMU_HEADER, PAYLOAD_SIZE, SaveProfile, THREE_DS_SIZE,
            inspect_bytes,
        },
        transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS, SWAP_SPANS},
    };

    fn synthetic_3ds_source() -> Vec<u8> {
        let mut source = (0..THREE_DS_SIZE)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        source
    }

    fn transformed_payload_mask() -> Vec<bool> {
        let mut mask = vec![false; PAYLOAD_SIZE];

        for span in SWAP_SPANS.iter() {
            mask[span.start..span.end].fill(true);
        }
        for &offset in MONSTER_DISCOVERY_OFFSETS.iter() {
            mask[offset..offset + 2].fill(true);
        }
        for &offset in ARENA_RECORD_OFFSETS.iter() {
            mask[offset..offset + 4].fill(true);
        }

        mask
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
    fn applies_each_declared_conversion_stage_to_the_payload() {
        let endian_span = SWAP_SPANS[0];
        let monster_offset = MONSTER_DISCOVERY_OFFSETS[0];
        let arena_offset = ARENA_RECORD_OFFSETS[0];
        assert_eq!((endian_span.start, endian_span.end), (28, 32));
        assert_eq!(monster_offset, 33_212);
        assert_eq!(arena_offset, 33_704);

        let mut source = synthetic_3ds_source();
        source[JP_3DS_HEADER.len() + endian_span.start..JP_3DS_HEADER.len() + endian_span.end]
            .copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        source[JP_3DS_HEADER.len() + monster_offset..JP_3DS_HEADER.len() + monster_offset + 2]
            .copy_from_slice(&[0x07, 0xA5]);
        source[JP_3DS_HEADER.len() + arena_offset..JP_3DS_HEADER.len() + arena_offset + 4]
            .copy_from_slice(&[0x55, 0xAA, 0xAA, 0x55]);

        let output = convert_3ds_to_cemu(&source).unwrap();
        let payload = &output[JP_CEMU_HEADER.len()..];

        assert_eq!(
            &payload[endian_span.start..endian_span.end],
            &[0x44, 0x33, 0x22, 0x11]
        );
        assert_eq!(&payload[monster_offset..monster_offset + 2], &[0xE0, 0]);
        assert_eq!(
            &payload[arena_offset..arena_offset + 4],
            &[0x54, 0xAA, 0xAB, 0x55]
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
    fn preserves_every_payload_byte_not_listed_by_a_transform() {
        let source = synthetic_3ds_source();
        let output = convert_3ds_to_cemu(&source).unwrap();
        let transformed = transformed_payload_mask();

        for (index, changed) in transformed.into_iter().enumerate() {
            if !changed {
                assert_eq!(
                    output[JP_CEMU_HEADER.len() + index],
                    source[JP_3DS_HEADER.len() + index],
                    "unexpected mutation at payload offset {index:#x}"
                );
            }
        }
    }
}
