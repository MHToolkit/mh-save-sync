use crate::{
    ConversionError,
    events::preserve_event_state,
    meow_transform_table::{
        MEOW_USER_ARENA4, MEOW_USER_CROWN, MEOW_USER_ID32, MEOW_USER_MASKED_SWAP2,
        MEOW_USER_MASKED_SWAP4, MEOW_USER_OFFICIAL_FIX_ARENA4, MEOW_USER_OFFICIAL_FIX_COPY1,
        MEOW_USER_OFFICIAL_FIX_SWAP2, MEOW_USER_OFFICIAL_FIX_SWAP4, MEOW_USER_SWAP2,
        MEOW_USER_SWAP4,
    },
    profile::PAYLOAD_SIZE,
    progress::remap_quest_completion,
    transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS, SWAP_SPANS},
};

pub use crate::events::SIMPLE_EVENT_START as EVENT_FLAG_START;
pub use crate::progress::QUEST_COMPLETION_START;

// These tables are statically recovered from the local MEOW v5 transfer core.
// They are the source-specific corrections layered over the 3usavetools spans.
const EQUIPMENT_BOX_START: usize = 4392;
const EQUIPMENT_BOX_COUNT: usize = 1000;
const CURRENT_EQUIPMENT_START: usize = 31280;
const CURRENT_EQUIPMENT_COUNT: usize = 7;
const EQUIPMENT_STRIDE: usize = 16;
const SECOND_RGBA_OFFSET: usize = 0x73E4;
const FULL_WIDTH_COUNTER_OFFSETS: [usize; 3] = [0x5BA4, 0x5CC8, 0x5CD4];
const MONSTER_SLAY_START: usize = 0x5784;
const MONSTER_CAPTURE_START: usize = 0x5884;
const MONSTER_SIZE_START: usize = 0x5984;
const MONSTER_SIZE_BYTE_COUNT: usize = 0x80 * 4;
const MONSTER_IDS: [usize; 50] = [
    0x0C, 0x0E, 0x2D, 0x03, 0x33, 0x2A, 0x2B, 0x2C, 0x08, 0x36, 0x09, 0x37, 0x2E, 0x49, 0x07, 0x10,
    0x38, 0x2F, 0x13, 0x39, 0x01, 0x3E, 0x3F, 0x02, 0x40, 0x41, 0x04, 0x34, 0x05, 0x35, 0x3B, 0x3C,
    0x3D, 0x06, 0x3A, 0x29, 0x48, 0x19, 0x55, 0x18, 0x42, 0x43, 0x12, 0x0F, 0x44, 0x45, 0x14, 0x46,
    0x4A, 0x4B,
];
const MONSTER_RECORD_RANGES: [(usize, usize); 2] = [(0x5D90, 13), (0x5E60, 25)];
const FARM_LEVELS_START: usize = 0x6128;
const FARM_HARVEST_START: usize = 0x612C;
const FARM_HARVEST_COUNT: usize = 3;
const FARM_FELYNE_SLOTS_START: usize = 0x6144;
const HUNTING_FLEET_SHIP_COUNT_START: usize = 0x5BC6;
const HUNTING_FLEET_SHIP_COUNT_END: usize = HUNTING_FLEET_SHIP_COUNT_START + 2;
const HUNTING_FLEET_DISPATCH_RECORD_START: usize = 0x5D18;

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

fn copy_reversed(
    source: &[u8],
    target: &mut [u8],
    offset: usize,
    width: usize,
) -> Result<(), ConversionError> {
    validate_range(source, offset, width, "Japanese Wii U correction source")?;
    validate_range(target, offset, width, "Japanese Wii U correction target")?;
    for index in 0..width {
        target[offset + index] = source[offset + width - 1 - index];
    }
    Ok(())
}

fn transform_equipment_record(
    source: &[u8],
    target: &mut [u8],
    offset: usize,
) -> Result<(), ConversionError> {
    validate_range(source, offset, EQUIPMENT_STRIDE, "equipment source")?;
    validate_range(target, offset, EQUIPMENT_STRIDE, "equipment target")?;

    target[offset..offset + 2].copy_from_slice(&source[offset..offset + 2]);
    copy_reversed(source, target, offset + 2, 2)?;
    if (1..=5).contains(&source[offset]) {
        copy_reversed(source, target, offset + 4, 4)?;
    } else {
        target[offset + 4..offset + 8].copy_from_slice(&source[offset + 4..offset + 8]);
    }
    for relative in [8, 10, 12] {
        copy_reversed(source, target, offset + relative, 2)?;
    }
    target[offset + 14..offset + 16].copy_from_slice(&source[offset + 14..offset + 16]);
    Ok(())
}

fn transform_item_record_table(
    source: &[u8],
    target: &mut [u8],
    start: usize,
    count: usize,
) -> Result<(), ConversionError> {
    for index in 0..count {
        let offset = start + index * 4;
        copy_reversed(source, target, offset, 2)?;
        copy_reversed(source, target, offset + 2, 2)?;
    }
    Ok(())
}

fn transform_user_id(
    source: &[u8],
    target: &mut [u8],
    offset: usize,
) -> Result<(), ConversionError> {
    validate_range(source, offset, 4, "user id source")?;
    validate_range(target, offset, 4, "user id target")?;
    let mut nibbles = [0_u8; 8];
    for (index, byte) in source[offset..offset + 4].iter().copied().enumerate() {
        nibbles[index * 2] = byte >> 4;
        nibbles[index * 2 + 1] = byte & 0x0f;
    }
    const PERMUTATION: [usize; 8] = [3, 0, 1, 5, 2, 6, 7, 4];
    let mut output = [0_u8; 4];
    for index in 0..4 {
        output[index] =
            (nibbles[PERMUTATION[index * 2]] << 4) | nibbles[PERMUTATION[index * 2 + 1]];
    }
    target[offset..offset + 4].copy_from_slice(&output);
    Ok(())
}

fn transform_arena4(
    source: &[u8],
    target: &mut [u8],
    offset: usize,
) -> Result<(), ConversionError> {
    validate_range(source, offset, 4, "arena source")?;
    validate_range(target, offset, 4, "arena target")?;
    let value = u32::from_le_bytes(source[offset..offset + 4].try_into().unwrap()).rotate_left(17);
    target[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn transform_crown(source: &[u8], target: &mut [u8], offset: usize) -> Result<(), ConversionError> {
    validate_range(source, offset, 1, "crown source")?;
    validate_range(target, offset, 1, "crown target")?;
    let state = source[offset];
    target[offset] =
        ((state & 0x01) << 7) | ((state & 0x02) << 4) | ((state & 0x04) << 4) | (state & 0x08);
    Ok(())
}

fn apply_confirmed_numeric_and_record_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for offset in FULL_WIDTH_COUNTER_OFFSETS {
        copy_reversed(source, target, offset, 4)?;
    }

    // 0x5984 is a packed physical-size cache, not the endian-converted
    // 50-entry hunter-record display log at 0x81B4.  The reference converter
    // preserves it byte-for-byte; reversing its u16 pairs produces impossible
    // monster measurements in MH3G HD.
    target[MONSTER_SIZE_START..MONSTER_SIZE_START + MONSTER_SIZE_BYTE_COUNT]
        .copy_from_slice(&source[MONSTER_SIZE_START..MONSTER_SIZE_START + MONSTER_SIZE_BYTE_COUNT]);

    for (index, monster_id) in MONSTER_IDS.into_iter().enumerate() {
        let slay_offset = MONSTER_SLAY_START + monster_id * 2;
        let capture_offset = MONSTER_CAPTURE_START + monster_id * 2;
        for offset in [slay_offset, capture_offset] {
            copy_reversed(source, target, offset, 2)?;
        }
        let slay = u16::from_le_bytes(source[slay_offset..slay_offset + 2].try_into().unwrap());
        let capture = u16::from_le_bytes(
            source[capture_offset..capture_offset + 2]
                .try_into()
                .unwrap(),
        );
        let discovery_offset = 0x81B4 + index * 10 + 8;
        if slay != 0 || capture != 0 || source[discovery_offset] & 0x01 != 0 {
            target[discovery_offset] |= 0x80;
        }
    }

    for (start, count) in MONSTER_RECORD_RANGES {
        for record in 0..count {
            let offset = start + record * 16;
            validate_range(source, offset, 16, "monster record source")?;
            validate_range(target, offset, 16, "monster record target")?;
            target[offset..offset + 8].copy_from_slice(&source[offset..offset + 8]);
            for relative in [8, 10, 12, 14] {
                copy_reversed(source, target, offset + relative, 2)?;
            }
        }
    }

    // These farm fields are packed bytes, not scalar u32 values. MEOW's
    // blanket swap4 reverses the facility levels and active Felyne slots.
    target[FARM_LEVELS_START..FARM_LEVELS_START + 4]
        .copy_from_slice(&source[FARM_LEVELS_START..FARM_LEVELS_START + 4]);
    for record in 0..FARM_HARVEST_COUNT {
        let offset = FARM_HARVEST_START + record * 8;
        // The Wii U title's actual user2 serializer walks this record at
        // file offset 0x6130 + 8*i (payload 0x612c + 8*i) and swaps only
        // +0, +2, and +6. The +4/+5 pair is two packed byte counters, so
        // reversing it makes the game decrement/display the wrong counter.
        for relative in [0, 2, 6] {
            copy_reversed(source, target, offset + relative, 2)?;
        }
        target[offset + 4..offset + 6].copy_from_slice(&source[offset + 4..offset + 6]);
    }
    target[FARM_FELYNE_SLOTS_START..FARM_FELYNE_SLOTS_START + 4]
        .copy_from_slice(&source[FARM_FELYNE_SLOTS_START..FARM_FELYNE_SLOTS_START + 4]);

    // MH3G HD reads the first byte as the hunting fleet count. The generic MEOW
    // swap2 table turns source [0x03, 0x00] into [0x00, 0x03], making the Wii U
    // title enumerate zero ships. A normal Wii U reference has the same pair
    // as the 3DS source, so preserve the verified two-byte field together.
    target[HUNTING_FLEET_SHIP_COUNT_START..HUNTING_FLEET_SHIP_COUNT_END]
        .copy_from_slice(&source[HUNTING_FLEET_SHIP_COUNT_START..HUNTING_FLEET_SHIP_COUNT_END]);

    // A 3DS before/after capture records this dispatched-ship field as the
    // ordered bytes [0x02, 0x01], rather than one scalar u16. Preserve that
    // observed byte order. It is only a dispatch-record correction: it has
    // not been shown to control the Wii U fleet-unlock UI.
    target[HUNTING_FLEET_DISPATCH_RECORD_START..HUNTING_FLEET_DISPATCH_RECORD_START + 2]
        .copy_from_slice(
            &source[HUNTING_FLEET_DISPATCH_RECORD_START..HUNTING_FLEET_DISPATCH_RECORD_START + 2],
        );

    Ok(())
}

/// Complete the statically recovered Wii U record corrections.
pub fn apply_japanese_wiiu_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    validate_payload_size(source)?;
    validate_payload_size(target)?;

    // MEOW v5 starts every operation from the original 3DS body. Do not feed
    // it the broader 3usavetools table: that table converts unrelated fields.
    for offset in MEOW_USER_SWAP4 {
        copy_reversed(source, target, offset, 4)?;
    }
    for offset in MEOW_USER_ARENA4 {
        transform_arena4(source, target, offset)?;
    }
    for offset in MEOW_USER_ID32 {
        transform_user_id(source, target, offset)?;
    }
    for offset in MEOW_USER_SWAP2 {
        copy_reversed(source, target, offset, 2)?;
    }
    for offset in MEOW_USER_CROWN {
        transform_crown(source, target, offset)?;
    }

    for record in 0..EQUIPMENT_BOX_COUNT {
        transform_equipment_record(
            source,
            target,
            EQUIPMENT_BOX_START + record * EQUIPMENT_STRIDE,
        )?;
    }
    for record in 0..CURRENT_EQUIPMENT_COUNT {
        transform_equipment_record(
            source,
            target,
            CURRENT_EQUIPMENT_START + record * EQUIPMENT_STRIDE,
        )?;
    }
    for offset in (264..3872).step_by(2) {
        copy_reversed(source, target, offset, 2)?;
    }
    transform_item_record_table(source, target, 168, 24)?;
    transform_item_record_table(source, target, 264, 32)?;
    transform_item_record_table(source, target, 392, 1000)?;
    transform_item_record_table(source, target, 3872, 36)?;
    for record in 0..24 {
        let offset = 20392 + record * 76;
        for field in (offset..offset + 56).step_by(2) {
            copy_reversed(source, target, field, 2)?;
        }
        target[offset + 56..offset + 76].copy_from_slice(&source[offset + 56..offset + 76]);
    }
    for offset in (31520..31592).step_by(2) {
        copy_reversed(source, target, offset, 2)?;
    }
    for offset in MEOW_USER_MASKED_SWAP2 {
        copy_reversed(source, target, offset, 2)?;
    }
    for offset in MEOW_USER_MASKED_SWAP4 {
        copy_reversed(source, target, offset, 4)?;
    }
    for offset in MEOW_USER_OFFICIAL_FIX_SWAP2 {
        copy_reversed(source, target, offset, 2)?;
    }
    for offset in MEOW_USER_OFFICIAL_FIX_SWAP4 {
        copy_reversed(source, target, offset, 4)?;
    }
    for offset in MEOW_USER_OFFICIAL_FIX_ARENA4 {
        transform_arena4(source, target, offset)?;
    }
    for offset in MEOW_USER_OFFICIAL_FIX_COPY1 {
        target[offset] = source[offset];
    }

    // These fields are read as big-endian values by the Wii U title. MEOW v5
    // copies them unchanged, while the 3DS body stores their logical values in
    // little-endian order.
    preserve_event_state(source, target)?;
    remap_quest_completion(source, target)?;
    copy_reversed(source, target, SECOND_RGBA_OFFSET, 4)?;
    apply_confirmed_numeric_and_record_corrections(source, target)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        profile::PAYLOAD_SIZE,
        transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS},
    };

    use super::{
        EQUIPMENT_BOX_START, EVENT_FLAG_START, QUEST_COMPLETION_START, SECOND_RGBA_OFFSET,
        apply_arena_records, apply_endian_swaps, apply_japanese_wiiu_corrections,
        apply_monster_discovery,
    };

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
                assert_eq!(
                    payload[offset + 1],
                    0xFF,
                    "source state {source_state:#04x}"
                );
            }

            for (index, (&before, &after)) in original.iter().zip(&payload).enumerate() {
                let declared = MONSTER_DISCOVERY_OFFSETS.contains(&index);
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

    #[test]
    fn japanese_wiiu_corrections_preserve_packed_record_bytes_and_swap_numeric_fields() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let mut target = vec![0xA5; PAYLOAD_SIZE];
        source[EQUIPMENT_BOX_START..EQUIPMENT_BOX_START + 16].copy_from_slice(&[
            0x03, 0x00, 0x00, 0x04, 0x0c, 0x05, 0x03, 0x00, 0x00, 0x00, 0x64, 0x00, 0x01, 0x00,
            0x34, 0x12,
        ]);

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[EQUIPMENT_BOX_START..EQUIPMENT_BOX_START + 16],
            &[
                0x03, 0x00, 0x04, 0x00, 0x00, 0x03, 0x05, 0x0c, 0x00, 0x00, 0x00, 0x64, 0x00, 0x01,
                0x34, 0x12,
            ]
        );
    }

    #[test]
    fn japanese_wiiu_corrections_swap_full_width_progress_counters() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[0x5ba4..0x5ba8].copy_from_slice(&26_765_u32.to_le_bytes());
        source[0x5cc8..0x5ccc].copy_from_slice(&31_848_u32.to_le_bytes());
        source[0x5cd4..0x5cd8].copy_from_slice(&80_668_u32.to_le_bytes());
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            u32::from_be_bytes(target[0x5ba4..0x5ba8].try_into().unwrap()),
            26_765
        );
        assert_eq!(
            u32::from_be_bytes(target[0x5cc8..0x5ccc].try_into().unwrap()),
            31_848
        );
        assert_eq!(
            u32::from_be_bytes(target[0x5cd4..0x5cd8].try_into().unwrap()),
            80_668
        );
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_monster_values_and_discovery() {
        const MONSTER_INDEX: usize = 2;
        const MONSTER_ID: usize = 0x2d;

        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let slay_offset = 0x5784 + MONSTER_ID * 2;
        let capture_offset = 0x5884 + MONSTER_ID * 2;
        let size_offset = 0x5984 + MONSTER_ID * 4;
        let discovery_offset = 0x81b4 + MONSTER_INDEX * 10 + 8;
        source[slay_offset..slay_offset + 2].copy_from_slice(&1_u16.to_le_bytes());
        source[capture_offset..capture_offset + 2].copy_from_slice(&2_u16.to_le_bytes());
        source[size_offset..size_offset + 2].copy_from_slice(&100_u16.to_le_bytes());
        source[size_offset + 2..size_offset + 4].copy_from_slice(&112_u16.to_le_bytes());
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            u16::from_be_bytes(target[slay_offset..slay_offset + 2].try_into().unwrap()),
            1
        );
        assert_eq!(
            u16::from_be_bytes(
                target[capture_offset..capture_offset + 2]
                    .try_into()
                    .unwrap()
            ),
            2
        );
        assert_eq!(&target[size_offset..size_offset + 4], &[100, 0, 112, 0]);
        assert_ne!(target[discovery_offset] & 0x80, 0);
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_packed_physical_monster_sizes() {
        // The display log at 0x81B4 is endian-converted independently.  This
        // earlier table is a packed physical-size cache, whose bytes are kept
        // in place by the reference converter.  Reversing these pairs changes
        // the numerical scale used by the Wii U hunter-record UI.
        const MONSTER_ID: usize = 0x3d;
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let size_offset = 0x5984 + MONSTER_ID * 4;
        source[size_offset..size_offset + 4].copy_from_slice(&[0x64, 0x00, 0x64, 0x00]);
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[size_offset..size_offset + 4],
            &[0x64, 0x00, 0x64, 0x00]
        );
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_packed_monster_record_fields() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[0x5d90..0x5da0].copy_from_slice(&[
            0x03, 0x00, 0x00, 0x04, 0x0c, 0x05, 0x03, 0x00, 0x00, 0x00, 0x64, 0x00, 0x01, 0x00,
            0x34, 0x12,
        ]);
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[0x5d90..0x5da0],
            &[
                0x03, 0x00, 0x00, 0x04, 0x0c, 0x05, 0x03, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x01,
                0x12, 0x34,
            ]
        );
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_farm_level_and_felyne_slot_order() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[0x6128..0x612c].copy_from_slice(&[0x02, 0x03, 0x03, 0x03]);
        source[0x612c..0x6130].copy_from_slice(&[0xaf, 0x00, 0x65, 0x01]);
        source[0x6144..0x6148].copy_from_slice(&[0x03, 0x00, 0x00, 0x00]);

        let mut target = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(&target[0x6128..0x612c], &[0x02, 0x03, 0x03, 0x03]);
        assert_eq!(&target[0x612c..0x6130], &[0x00, 0xaf, 0x01, 0x65]);
        assert_eq!(&target[0x6144..0x6148], &[0x03, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_farm_harvest_field_boundaries() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[0x612c..0x6134].copy_from_slice(&[0xaf, 0x00, 0x65, 0x01, 0x07, 0x0a, 0x27, 0xe5]);

        let mut target = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[0x612c..0x6134],
            &[0x00, 0xaf, 0x01, 0x65, 0x07, 0x0a, 0xe5, 0x27]
        );
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_hunting_fleet_ship_count_field() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        // Three ships are unlocked in the 3DS save. MEOW's generic swap2
        // would otherwise move this 0x03 into the adjacent byte.
        source[0x5bc6..0x5bc8].copy_from_slice(&[0x03, 0x00]);

        let mut target = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(&target[0x5bc6..0x5bc8], &[0x03, 0x00]);
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_observed_hunting_fleet_dispatch_field_order() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        // 3DS capture after dispatching the red hunting ship: the leading
        // status field is [0x02, 0x01], not a scalar u16 to byte-swap.
        source[0x5d18..0x5d1a].copy_from_slice(&[0x02, 0x01]);

        let mut target = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(&target[0x5d18..0x5d1a], &[0x02, 0x01]);
    }

    #[test]
    fn japanese_wiiu_corrections_swap_confirmed_event_flags_and_second_rgba() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[EVENT_FLAG_START..EVENT_FLAG_START + 4].copy_from_slice(&[0x34, 0x12, 0xCD, 0xAB]);
        source[SECOND_RGBA_OFFSET..SECOND_RGBA_OFFSET + 4]
            .copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[EVENT_FLAG_START..EVENT_FLAG_START + 4],
            &[0x12, 0x34, 0xAB, 0xCD]
        );
        assert_eq!(
            &target[SECOND_RGBA_OFFSET..SECOND_RGBA_OFFSET + 4],
            &[0x44, 0x33, 0x22, 0x11]
        );
    }

    #[test]
    fn japanese_wiiu_corrections_preserve_quest_completion_bits_across_endianness() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let logical_words = [0x0000_0401_u32, 0x8000_0001, 0x1234_5678, 0xA5A5_5A5A];
        for (index, word) in logical_words.into_iter().enumerate() {
            let offset = QUEST_COMPLETION_START + index * 4;
            source[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        }
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        for (index, expected) in logical_words.into_iter().enumerate() {
            let offset = QUEST_COMPLETION_START + index * 4;
            assert_eq!(
                u32::from_be_bytes(target[offset..offset + 4].try_into().unwrap()),
                expected,
                "quest completion word {index}"
            );
        }
    }

    #[test]
    fn japanese_wiiu_corrections_reject_wrong_lengths_before_mutating() {
        let source = vec![0_u8; PAYLOAD_SIZE];
        let mut target = vec![0x5A; PAYLOAD_SIZE - 1];
        let original = target.clone();

        assert!(apply_japanese_wiiu_corrections(&source, &mut target).is_err());
        assert_eq!(target, original);
    }
}
