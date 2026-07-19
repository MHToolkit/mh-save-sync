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
