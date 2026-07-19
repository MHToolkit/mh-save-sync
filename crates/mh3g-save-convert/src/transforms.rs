use crate::{
    ConversionError,
    profile::PAYLOAD_SIZE,
    transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS, SWAP_SPANS},
};

fn validate_payload_size(payload: &[u8]) -> Result<(), ConversionError> {
    if payload.len() != PAYLOAD_SIZE {
        return Err(ConversionError::InvalidSave(format!(
            "MH3G payload must be {PAYLOAD_SIZE} bytes, got {}",
            payload.len()
        )));
    }

    Ok(())
}

fn validate_range(
    payload: &[u8],
    start: usize,
    width: usize,
    transform: &str,
) -> Result<(), ConversionError> {
    let end = start.checked_add(width).ok_or_else(|| {
        ConversionError::InvalidSave(format!(
            "{transform} range overflows payload bounds: {start} + {width}"
        ))
    })?;

    if end > payload.len() {
        return Err(ConversionError::InvalidSave(format!(
            "{transform} range {start}..{end} exceeds payload length {}",
            payload.len()
        )));
    }

    Ok(())
}

pub fn apply_endian_swaps(payload: &mut [u8]) -> Result<(), ConversionError> {
    validate_payload_size(payload)?;
    for span in SWAP_SPANS.iter() {
        if span.start >= span.end {
            return Err(ConversionError::InvalidSave(format!(
                "endian swap has invalid range {}..{}",
                span.start, span.end
            )));
        }
        validate_range(payload, span.start, span.end - span.start, "endian swap")?;
    }

    for span in SWAP_SPANS.iter() {
        payload[span.start..span.end].reverse();
    }

    Ok(())
}

pub fn apply_monster_discovery(payload: &mut [u8]) -> Result<(), ConversionError> {
    validate_payload_size(payload)?;
    for &offset in MONSTER_DISCOVERY_OFFSETS.iter() {
        validate_range(payload, offset, 2, "monster discovery")?;
    }

    for &offset in MONSTER_DISCOVERY_OFFSETS.iter() {
        let state = payload[offset];
        let mut converted = 0_u8;

        if state & 0x01 != 0 {
            converted |= 0x80;
        }
        if state & 0x02 != 0 {
            converted |= 0x20;
        }
        if state & 0x04 != 0 {
            converted |= 0x40;
        }
        if state & 0x08 != 0 {
            converted |= 0x08;
        }

        payload[offset] = converted;
        payload[offset + 1] = 0;
    }

    Ok(())
}

fn convert_arena_record(first_half: u16, second_half: u16) -> (u16, u16) {
    let converted_first = first_half.rotate_left(8);
    let first_dropped = converted_first & 0x8000 != 0;
    let converted_first = converted_first << 1;

    let converted_second = second_half.rotate_left(8);
    let second_dropped = converted_second & 0x8000 != 0;
    let converted_second = converted_second << 1;

    (
        converted_first | u16::from(second_dropped),
        converted_second | u16::from(first_dropped),
    )
}

pub fn apply_arena_records(payload: &mut [u8]) -> Result<(), ConversionError> {
    validate_payload_size(payload)?;
    for &offset in ARENA_RECORD_OFFSETS.iter() {
        validate_range(payload, offset, 4, "arena record")?;
    }

    for &offset in ARENA_RECORD_OFFSETS.iter() {
        let first_half = u16::from_be_bytes([payload[offset], payload[offset + 1]]);
        let second_half = u16::from_be_bytes([payload[offset + 2], payload[offset + 3]]);
        let (converted_first, converted_second) = convert_arena_record(first_half, second_half);

        payload[offset..offset + 2].copy_from_slice(&converted_first.to_be_bytes());
        payload[offset + 2..offset + 4].copy_from_slice(&converted_second.to_be_bytes());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        profile::PAYLOAD_SIZE,
        transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS},
    };

    use super::{apply_arena_records, apply_endian_swaps, apply_monster_discovery};

    #[test]
    fn transforms_endian_swaps_reverse_declared_spans_only() {
        let mut payload = vec![0_u8; PAYLOAD_SIZE];
        payload[0x20..0x24].copy_from_slice(&[1, 2, 3, 4]);
        payload[0x2A..0x2C].copy_from_slice(&[5, 6]);
        payload[0x10] = 0xAA;

        apply_endian_swaps(&mut payload).unwrap();

        assert_eq!(&payload[0x20..0x24], &[4, 3, 2, 1]);
        assert_eq!(&payload[0x2A..0x2C], &[6, 5]);
        assert_eq!(payload[0x10], 0xAA);
    }

    #[test]
    fn transforms_endian_swaps_reject_wrong_length_before_mutating() {
        let mut payload = vec![0x5A; PAYLOAD_SIZE - 1];
        let original = payload.clone();

        assert!(apply_endian_swaps(&mut payload).is_err());
        assert_eq!(payload, original);
    }

    #[test]
    fn transforms_monster_discovery_maps_all_source_flag_combinations() {
        for source_state in 0_u8..=0x0F {
            let mut payload = vec![0xA5; PAYLOAD_SIZE];
            for &offset in MONSTER_DISCOVERY_OFFSETS.iter() {
                payload[offset] = source_state;
                payload[offset + 1] = 0xFF;
            }
            let original = payload.clone();

            apply_monster_discovery(&mut payload).unwrap();

            let expected = ((source_state & 0x01) << 7)
                | ((source_state & 0x02) << 4)
                | ((source_state & 0x04) << 4)
                | (source_state & 0x08);
            for &offset in MONSTER_DISCOVERY_OFFSETS.iter() {
                assert_eq!(
                    payload[offset], expected,
                    "source state {source_state:#04x}"
                );
                assert_eq!(payload[offset + 1], 0, "source state {source_state:#04x}");
            }

            for (index, (&before, &after)) in original.iter().zip(&payload).enumerate() {
                let declared = MONSTER_DISCOVERY_OFFSETS
                    .iter()
                    .any(|&offset| index == offset || index == offset + 1);
                assert!(
                    declared || before == after,
                    "unexpected mutation at {index:#x}"
                );
            }
        }
    }

    #[test]
    fn transforms_arena_records_port_upstream_bit_equations() {
        let cases: [(u16, u16, u16, u16); 5] = [
            (0x0000, 0x0000, 0x0000, 0x0000),
            (0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF),
            (0x0080, 0x0000, 0x0000, 0x0001),
            (0x0000, 0x0080, 0x0001, 0x0000),
            (0x55AA, 0xAA55, 0x54AA, 0xAB55),
        ];

        for (first, second, expected_first, expected_second) in cases {
            let mut payload = vec![0xA5; PAYLOAD_SIZE];
            for &offset in ARENA_RECORD_OFFSETS.iter() {
                payload[offset..offset + 2].copy_from_slice(&first.to_be_bytes());
                payload[offset + 2..offset + 4].copy_from_slice(&second.to_be_bytes());
            }
            let original = payload.clone();

            apply_arena_records(&mut payload).unwrap();

            for &offset in ARENA_RECORD_OFFSETS.iter() {
                assert_eq!(
                    u16::from_be_bytes([payload[offset], payload[offset + 1]]),
                    expected_first
                );
                assert_eq!(
                    u16::from_be_bytes([payload[offset + 2], payload[offset + 3]]),
                    expected_second
                );
            }

            for (index, (&before, &after)) in original.iter().zip(&payload).enumerate() {
                let declared = ARENA_RECORD_OFFSETS
                    .iter()
                    .any(|&offset| (offset..offset + 4).contains(&index));
                assert!(
                    declared || before == after,
                    "unexpected mutation at {index:#x}"
                );
            }
        }
    }

    #[test]
    fn transforms_special_fields_reject_wrong_length_before_mutating() {
        for transform in [apply_monster_discovery, apply_arena_records] {
            let mut payload = vec![0x5A; PAYLOAD_SIZE - 1];
            let original = payload.clone();

            assert!(transform(&mut payload).is_err());
            assert_eq!(payload, original);
        }
    }
}
