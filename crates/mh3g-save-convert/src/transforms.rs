use crate::{
    ConversionError,
    events::preserve_event_state,
    meow_transform_table::{
        MEOW_CARD_ARENA4, MEOW_CARD_CROWN, MEOW_CARD_SWAP2, MEOW_CARD_SWAP4, MEOW_CARDBOX_ARENA4,
        MEOW_CARDBOX_CROWN, MEOW_CARDBOX_SWAP2, MEOW_CARDBOX_SWAP4, MEOW_USER_ARENA4,
        MEOW_USER_CROWN, MEOW_USER_ID32, MEOW_USER_MASKED_SWAP2, MEOW_USER_MASKED_SWAP4,
        MEOW_USER_OFFICIAL_FIX_ARENA4, MEOW_USER_OFFICIAL_FIX_COPY1, MEOW_USER_OFFICIAL_FIX_SWAP2,
        MEOW_USER_OFFICIAL_FIX_SWAP4, MEOW_USER_SWAP2, MEOW_USER_SWAP4,
    },
    profile::PAYLOAD_SIZE,
    progress::remap_quest_completion,
    revision::ConverterRevision,
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
// The monster-list/book state is a byte-packed 28-byte array on both
// platforms. Five paired official transfers preserve every byte verbatim.
// Treating its tail as u16 lanes moves the final 0x00 byte from 0x577B to
// 0x577A; in the Yoruaski sample that suppresses Deviljho from the list even
// though both the acquired-book bit and the personal monster record are set.
const MONSTER_GUIDE_PACKED_STATE_START: usize = 0x5760;
const MONSTER_GUIDE_PACKED_STATE_LEN: usize = 0x1C;
// Both games expose one u16 slay counter and one u16 capture counter for each
// known monster ID 0x00..=0x55. The tables keep the same numeric values, but
// 3DS stores each lane little-endian and Wii U stores it big-endian. The
// historical MEOW table omitted the slay lanes for IDs 0x1A..=0x1C; Wii U then
// interpreted ordinary counts as values above 9999 and the UI saturated them
// to 9999.
const MONSTER_SLAY_COUNT_START: usize = 0x5784;
const MONSTER_CAPTURE_COUNT_START: usize = 0x5884;
const MONSTER_COUNT_ENTRY_COUNT: usize = 0x56;
const MONSTER_COUNT_STRIDE: usize = 2;
// The Wii U executable indexes this region as a 1536-bit item-acquisition
// bitset: item_id >> 5 selects one of 48 u32 words and item_id & 31 selects the
// bit inside that word. Five independently paired official transfers agree
// that every word changes from 3DS little endian to Wii U big endian.
//
// This is not a table of 48 monster records. One user-visible consequence of
// getting the word order wrong is losing item 0x4F8 (the Deviljho book), which
// removes the ninth page from Hunter's Notes even though the 3DS source owns
// the book.
const ITEM_ACQUIRED_BITSET_START: usize = 0x65C4;
const ITEM_ACQUIRED_BITSET_WORD_COUNT: usize = 48;
const ITEM_ACQUIRED_BITSET_WORD_SIZE: usize = 4;
#[cfg(test)]
const DEVILJHO_BOOK_ITEM_ID: usize = 0x4F8;
// These appearance scalars and the adjacent RGBA value are serialized as
// little-endian four-byte values on 3DS and big-endian values on Wii U. The
// older static table covered only the scalar at 0x73C4 and the later RGBA
// values, leaving this subset in mixed byte order.
const PLAYER_APPEARANCE_SCALAR_OFFSETS: [usize; 3] = [0x73B8, 0x73BC, 0x73C8];
const PLAYER_APPEARANCE_PACKED_STYLE_OFFSET: usize = 0x73D0;
const PLAYER_APPEARANCE_RGBA_OFFSET: usize = 0x73D8;
const FULL_WIDTH_COUNTER_OFFSETS: [usize; 3] = [0x5BA4, 0x5CC8, 0x5CD4];
const MONSTER_IDS: [usize; 50] = [
    0x0C, 0x0E, 0x2D, 0x03, 0x33, 0x2A, 0x2B, 0x2C, 0x08, 0x36, 0x09, 0x37, 0x2E, 0x49, 0x07, 0x10,
    0x38, 0x2F, 0x13, 0x39, 0x01, 0x3E, 0x3F, 0x02, 0x40, 0x41, 0x04, 0x34, 0x05, 0x35, 0x3B, 0x3C,
    0x3D, 0x06, 0x3A, 0x29, 0x48, 0x19, 0x55, 0x18, 0x42, 0x43, 0x12, 0x0F, 0x44, 0x45, 0x14, 0x46,
    0x4A, 0x4B,
];
// The 3DS hunting-record builder maps display Deviljho (0x07) through a
// second, non-display size cache at 0x5984 + 0x47 * 4. It uses the lower
// minimum and higher maximum from both records before formatting the UI.
const DEVILJHO_LINKED_SIZE_CACHE_ID: usize = 0x47;
const MONSTER_RECORD_RANGES: [(usize, usize); 2] = [(0x5D90, 13), (0x5E60, 25)];
const FARM_LEVELS_START: usize = 0x6128;
const FARM_HARVEST_START: usize = 0x612C;
const FARM_HARVEST_COUNT: usize = 3;
const FARM_FELYNE_SLOTS_START: usize = 0x6144;
const HUNTING_FLEET_SHIP_COUNT_START: usize = 0x5BC6;
const HUNTING_FLEET_SHIP_COUNT_END: usize = HUNTING_FLEET_SHIP_COUNT_START + 2;
const HUNTING_FLEET_DISPATCH_RECORD_START: usize = 0x5D18;
// Cha-Cha and Kayamba have two adjacent, fixed-width companion records. Five
// independently paired 3DS -> Wii U transfers agree on the field boundaries
// through relative 0x140: one u32 prefix, u16 scalars through 0xDE, then a
// byte-packed mask/mastery block which must retain its exact byte order.
//
// Releases through 0.0.18 inherited two narrower schema assumptions from the
// recovered transfer table: offsets 0x04..0x0B were treated as two u32 values,
// and relative 0xE4 was treated as an isolated u16. Both assumptions disagree
// with every paired transfer. In particular, swapping the packed bytes at
// 0xE4 puts zero in the field read by the companion status screen; a quest
// completion can then serialize that zero back over the mask record.
const SHAKALAKA_RECORD_START: usize = 0x6F44;
const SHAKALAKA_RECORD_COUNT: usize = 2;
const SHAKALAKA_RECORD_STRIDE: usize = 0x148;
const HISTORICAL_SHAKALAKA_U32_HEADER_SIZE: usize = 0x0C;
const SHAKALAKA_U32_PREFIX_SIZE: usize = 0x04;
const SHAKALAKA_MASK_STATE_START: usize = 0xDE;
const HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET: usize = 0xE4;
const SHAKALAKA_MASK_STATE_END: usize = 0x140;
const OFFLINE_HUNTER_EQUIPMENT_CACHE_START: usize = 0x75B0;
const OFFLINE_HUNTER_HEADER_START: usize = 0x75E0;
const OFFLINE_HUNTER_COUNT: usize = 6;
const OFFLINE_HUNTER_STRIDE: usize = 0x70;
const OFFLINE_HUNTER_HR_OFFSET: usize = 0x04;
const OFFLINE_HUNTER_NAME_START: usize = 0x1C;
const OFFLINE_HUNTER_NAME_SIZE: usize = 0x10;
const OFFLINE_HUNTER_EQUIPMENT_COUNT: usize = 5;
const OFFLINE_HUNTER_EQUIPMENT_STRIDE: usize = 8;
const OFFLINE_HUNTER_TAIL_COLOR_OFFSETS: [usize; 2] = [0x28, 0x2C];
const OFFLINE_HUNTER_CANDIDATE_IDS_START: usize = 0x7848;
const OFFLINE_HUNTER_CANDIDATE_ID_COUNT: usize = 3;
const CARD_BODY_SIZE: usize = 0x57_FFC;
const CARDBOX_BODY_SIZE: usize = 0x2F_FFC;
pub const GUILD_CARD_SLOT_SIZE: usize = 0xE00;
// The native Wii U lookup scans exactly 0x62 card slots. Logical slot 98 and
// the trailing body are summary/index metadata with a different record shape.
const GUILD_CARD_SLOT_COUNT: usize = 0x62;
const GUILD_CARD_HR_OFFSET: usize = 0x14;
const GUILD_CARD_EQUIPMENT_START: usize = 0x4C;
const GUILD_CARD_EQUIPMENT_COUNT: usize = 5;
const GUILD_CARD_EQUIPMENT_STRIDE: usize = 0x10;
const GUILD_CARD_TAIL_COLOR_OFFSETS: [usize; 2] = [0x110, 0x114];
const GUILD_CARD_WEAPON_USAGE_START: usize = 0x12C;
const GUILD_CARD_WEAPON_USAGE_COUNT: usize = 36;
const GUILD_CARD_MONSTER_LOG_START: usize = 0x7C0;
const GUILD_CARD_MONSTER_LOG_COUNT: usize = 50;
const GUILD_CARD_MONSTER_LOG_STRIDE: usize = 10;
// The player record and each full guild-card slot have 110 arena values: the
// original 62-row table plus the 48 rows handled by the official transfer fix.
// They start at different offsets but share the packed four-byte layout. The
// table must not run into the unrelated fields that follow it.
const USER_ARENA_RECORD_START: usize = 0x83A8;
const USER_ARENA_RECORD_COUNT: usize = 110;
const GUILD_CARD_ARENA_RECORD_START: usize = 0x9B4;
const GUILD_CARD_ARENA_RECORD_COUNT: usize = 110;
const ARENA_RECORD_STRIDE: usize = 4;
const GUILD_CARD_TRAILER_START: usize = GUILD_CARD_SLOT_COUNT * GUILD_CARD_SLOT_SIZE;
const GUILD_CARD_SUMMARY_START: usize = GUILD_CARD_TRAILER_START + 0x0C;
const GUILD_CARD_SUMMARY_RECORD_COUNT: usize = 33;
const GUILD_CARD_SUMMARY_RECORD_STRIDE: usize = 0x38;
const GUILD_CARD_SUMMARY_FRIENDSHIP_OFFSET: usize = 0x28;

/// Which guild-card body is being converted.
///
/// `card1`/`card2`/`card3` share one full-size body layout, while `cardbox`
/// uses a compact layout with an independent static operation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildCardBodyKind {
    Card,
    Cardbox,
}

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

/// Convert one complete contiguous arena-record table.
///
/// Arena times are packed as two linked u16 lanes, so a regular byte swap is
/// not valid.  `transform_arena4` implements the official rotate/carry
/// conversion for each four-byte row. This helper deliberately walks the
/// declared schema rather than only the static offsets encountered in one
/// body: zero rows and the same rows in later repeated guild-card slots need
/// exactly the same conversion when they become non-zero in another player's
/// save.
fn apply_arena_record_table(
    source: &[u8],
    target: &mut [u8],
    start: usize,
    count: usize,
) -> Result<(), ConversionError> {
    for record in 0..count {
        transform_arena4(source, target, start + record * ARENA_RECORD_STRIDE)?;
    }
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

/// Reproduce the historical 0.0.5/0.0.6 Hunter's Notes visibility rule.
///
/// Released converters inferred discovery from non-zero hunt counters. Keep
/// that behavior isolated here so compatibility repair can still recognize
/// their byte output. Current conversion must not use it: paired official
/// transfers prove that an undiscovered row remains hidden even when its
/// counters are non-zero.
fn apply_historical_hunter_notes_display_state(
    source: &[u8],
    target: &mut [u8],
    slay_offset: usize,
    capture_offset: usize,
    state_offset: usize,
) -> Result<(), ConversionError> {
    validate_range(source, slay_offset, 2, "Hunter's Notes slay source")?;
    validate_range(source, capture_offset, 2, "Hunter's Notes capture source")?;
    transform_crown(source, target, state_offset)?;

    let slays = u16::from_le_bytes(
        source[slay_offset..slay_offset + 2]
            .try_into()
            .expect("validated Hunter's Notes slay range"),
    );
    let captures = u16::from_le_bytes(
        source[capture_offset..capture_offset + 2]
            .try_into()
            .expect("validated Hunter's Notes capture range"),
    );
    if slays != 0 || captures != 0 {
        target[state_offset] |= 0x80;
    }

    Ok(())
}

/// Apply the authoritative current Hunter's Notes state mapping.
///
/// Personal records, received guild cards, and CEC/offline-hall partner cards
/// all use the same source-owned discovery/crown byte. Official transfer pairs
/// preserve that state independently of hunt counters, so visibility must be
/// derived only from this byte.
fn apply_current_hunter_notes_display_state(
    source: &[u8],
    target: &mut [u8],
    state_offset: usize,
) -> Result<(), ConversionError> {
    transform_crown(source, target, state_offset)
}

fn apply_guild_card_monster_log_corrections(
    source: &[u8],
    target: &mut [u8],
    revision: ConverterRevision,
) -> Result<(), ConversionError> {
    for slot in 0..GUILD_CARD_SLOT_COUNT {
        apply_guild_card_monster_log_slot_corrections(
            source,
            target,
            slot * GUILD_CARD_SLOT_SIZE,
            revision,
        )?;
    }

    Ok(())
}

/// Reapply the complete Hunter's Notes schema to one received-card slot.
///
/// Card files and experimental CEC records store the same 0xE00-byte card
/// shape. Keeping this correction slot-relative makes their monster names and
/// crown bits agree with the personal guild-card view.
fn apply_guild_card_monster_log_slot_corrections(
    source: &[u8],
    target: &mut [u8],
    slot_start: usize,
    revision: ConverterRevision,
) -> Result<(), ConversionError> {
    for row in 0..GUILD_CARD_MONSTER_LOG_COUNT {
        let record_start =
            slot_start + GUILD_CARD_MONSTER_LOG_START + row * GUILD_CARD_MONSTER_LOG_STRIDE;
        // A monster-log row is four adjacent u16 values (slays, captures,
        // maximum size, minimum size), followed by crown/discovery bytes.
        // The static MEOW table treats a subset of these bytes as broader
        // scalar spans. Reassert the confirmed field boundaries after it.
        for relative in [0, 2, 4, 6] {
            copy_reversed(source, target, record_start + relative, 2)?;
        }

        if revision >= ConverterRevision::V0_0_5 {
            apply_historical_hunter_notes_display_state(
                source,
                target,
                record_start,
                record_start + 2,
                record_start + 8,
            )?;
        }
    }

    Ok(())
}

fn apply_current_guild_card_monster_log_slot_corrections(
    source: &[u8],
    target: &mut [u8],
    slot_start: usize,
) -> Result<(), ConversionError> {
    for row in 0..GUILD_CARD_MONSTER_LOG_COUNT {
        let state_offset =
            slot_start + GUILD_CARD_MONSTER_LOG_START + row * GUILD_CARD_MONSTER_LOG_STRIDE + 8;
        apply_current_hunter_notes_display_state(source, target, state_offset)?;
    }
    Ok(())
}

fn apply_current_guild_card_monster_log_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for slot in 0..GUILD_CARD_SLOT_COUNT {
        apply_current_guild_card_monster_log_slot_corrections(
            source,
            target,
            slot * GUILD_CARD_SLOT_SIZE,
        )?;
    }
    Ok(())
}

fn apply_guild_card_arena_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for slot in 0..GUILD_CARD_SLOT_COUNT {
        apply_arena_record_table(
            source,
            target,
            slot * GUILD_CARD_SLOT_SIZE + GUILD_CARD_ARENA_RECORD_START,
            GUILD_CARD_ARENA_RECORD_COUNT,
        )?;
    }
    Ok(())
}

fn apply_guild_card_weapon_usage_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for slot in 0..GUILD_CARD_SLOT_COUNT {
        let slot_start = slot * GUILD_CARD_SLOT_SIZE;
        for counter in 0..GUILD_CARD_WEAPON_USAGE_COUNT {
            copy_reversed(
                source,
                target,
                slot_start + GUILD_CARD_WEAPON_USAGE_START + counter * 2,
                2,
            )?;
        }
    }

    Ok(())
}

fn apply_guild_card_hr_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for slot in 0..GUILD_CARD_SLOT_COUNT {
        copy_reversed(
            source,
            target,
            slot * GUILD_CARD_SLOT_SIZE + GUILD_CARD_HR_OFFSET,
            2,
        )?;
    }

    Ok(())
}

fn transform_compact_equipment_header(
    source: &[u8],
    target: &mut [u8],
    offset: usize,
) -> Result<(), ConversionError> {
    validate_range(source, offset, 8, "compact equipment source")?;
    validate_range(target, offset, 8, "compact equipment target")?;

    // Offline-hunter and received-card caches use a compact version of an
    // equipment record: two packed bytes, an independent u16, and an
    // independent u32. Reversing the final six bytes as one scalar corrupts
    // the equipment/material IDs used to construct the partner model.
    target[offset..offset + 2].copy_from_slice(&source[offset..offset + 2]);
    copy_reversed(source, target, offset + 2, 2)?;
    copy_reversed(source, target, offset + 4, 4)?;
    Ok(())
}

fn apply_guild_card_equipment_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for slot in 0..GUILD_CARD_SLOT_COUNT {
        let slot_start = slot * GUILD_CARD_SLOT_SIZE;
        for equipment in 0..GUILD_CARD_EQUIPMENT_COUNT {
            transform_compact_equipment_header(
                source,
                target,
                slot_start + GUILD_CARD_EQUIPMENT_START + equipment * GUILD_CARD_EQUIPMENT_STRIDE,
            )?;
        }
        for relative in GUILD_CARD_TAIL_COLOR_OFFSETS {
            copy_reversed(source, target, slot_start + relative, 4)?;
        }
    }
    Ok(())
}

fn apply_offline_hunter_roster_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for hunter in 0..OFFLINE_HUNTER_COUNT {
        let header_start = OFFLINE_HUNTER_HEADER_START + hunter * OFFLINE_HUNTER_STRIDE;
        copy_reversed(source, target, header_start + OFFLINE_HUNTER_HR_OFFSET, 2)?;
        let name_start = header_start + OFFLINE_HUNTER_NAME_START;
        validate_range(
            source,
            name_start,
            OFFLINE_HUNTER_NAME_SIZE,
            "offline hunter name source",
        )?;
        validate_range(
            target,
            name_start,
            OFFLINE_HUNTER_NAME_SIZE,
            "offline hunter name target",
        )?;
        target[name_start..name_start + OFFLINE_HUNTER_NAME_SIZE]
            .copy_from_slice(&source[name_start..name_start + OFFLINE_HUNTER_NAME_SIZE]);

        let cache_start = OFFLINE_HUNTER_EQUIPMENT_CACHE_START + hunter * OFFLINE_HUNTER_STRIDE;
        for equipment in 0..OFFLINE_HUNTER_EQUIPMENT_COUNT {
            transform_compact_equipment_header(
                source,
                target,
                cache_start + equipment * OFFLINE_HUNTER_EQUIPMENT_STRIDE,
            )?;
        }
        for relative in OFFLINE_HUNTER_TAIL_COLOR_OFFSETS {
            copy_reversed(source, target, cache_start + relative, 4)?;
        }
    }
    Ok(())
}

fn apply_guild_card_metadata_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    // Received-card summary records follow the 98 full card slots and a
    // 12-byte trailer header. The 3DS stores each friendship score as a
    // little-endian u32 at +0x28 of a 0x38-byte record;
    // Wii U reads the same field as big-endian. The recovered sparse table
    // covered only the first four records, leaving later cards with values
    // such as 25,007,226.88 after the platform conversion.
    for record in 0..GUILD_CARD_SUMMARY_RECORD_COUNT {
        let offset = GUILD_CARD_SUMMARY_START
            + record * GUILD_CARD_SUMMARY_RECORD_STRIDE
            + GUILD_CARD_SUMMARY_FRIENDSHIP_OFFSET;
        copy_reversed(source, target, offset, 4)?;
    }

    Ok(())
}

/// Apply the first fixed guild-card slot's mapping to a standalone slot.
///
/// CEC/StreetPass records pack three 0xE00-byte received cards, while the
/// `card1`/`card2`/`card3` files contain the same slot shape inside a larger
/// body. The recovered MEOW table is expressed in full-body offsets, so only
/// operations in the first slot are selected and then applied to the packed
/// slot. This keeps the CEC path from copying 3DS little-endian scalars into a
/// Wii U cache unchanged.
pub fn apply_japanese_wiiu_guild_card_slot_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    apply_japanese_wiiu_guild_card_slot_corrections_for_revision(
        source,
        target,
        ConverterRevision::LAST_HISTORICAL,
    )?;
    apply_current_guild_card_monster_log_slot_corrections(source, target, 0)
}

pub(crate) fn apply_japanese_wiiu_guild_card_slot_corrections_for_revision(
    source: &[u8],
    target: &mut [u8],
    revision: ConverterRevision,
) -> Result<(), ConversionError> {
    if source.len() != GUILD_CARD_SLOT_SIZE || target.len() != GUILD_CARD_SLOT_SIZE {
        return Err(ConversionError::InvalidSave(format!(
            "MH3G guild-card slot must be {GUILD_CARD_SLOT_SIZE} bytes, got source {} and target {}",
            source.len(),
            target.len()
        )));
    }

    for &offset in MEOW_CARD_SWAP4
        .iter()
        .filter(|&&offset| offset < GUILD_CARD_SLOT_SIZE)
    {
        copy_reversed(source, target, offset, 4)?;
    }
    for &offset in MEOW_CARD_ARENA4
        .iter()
        .filter(|&&offset| offset < GUILD_CARD_SLOT_SIZE)
    {
        transform_arena4(source, target, offset)?;
    }

    if revision >= ConverterRevision::V0_0_4 {
        apply_arena_record_table(
            source,
            target,
            GUILD_CARD_ARENA_RECORD_START,
            GUILD_CARD_ARENA_RECORD_COUNT,
        )?;
    }
    for &offset in MEOW_CARD_SWAP2
        .iter()
        .filter(|&&offset| offset < GUILD_CARD_SLOT_SIZE)
    {
        copy_reversed(source, target, offset, 2)?;
    }
    for &offset in MEOW_CARD_CROWN
        .iter()
        .filter(|&&offset| offset < GUILD_CARD_SLOT_SIZE)
    {
        transform_crown(source, target, offset)?;
    }

    // The embedded weapon-use table is 36 adjacent u16 counters.  The sparse
    // MEOW offsets do not cover zero fields and sometimes use a 4-byte span,
    // which reverses a pair of counters rather than their byte order.
    for counter in 0..GUILD_CARD_WEAPON_USAGE_COUNT {
        copy_reversed(
            source,
            target,
            GUILD_CARD_WEAPON_USAGE_START + counter * 2,
            2,
        )?;
    }
    copy_reversed(source, target, GUILD_CARD_HR_OFFSET, 2)?;

    for equipment in 0..GUILD_CARD_EQUIPMENT_COUNT {
        transform_compact_equipment_header(
            source,
            target,
            GUILD_CARD_EQUIPMENT_START + equipment * GUILD_CARD_EQUIPMENT_STRIDE,
        )?;
    }
    for offset in GUILD_CARD_TAIL_COLOR_OFFSETS {
        copy_reversed(source, target, offset, 4)?;
    }

    apply_guild_card_monster_log_slot_corrections(source, target, 0, revision)?;

    Ok(())
}

/// Apply the platform-specific field mapping to an MH3G guild-card body.
///
/// The static operations are recovered from the local MEOW v5 transfer core.
/// Every operation reads the original 3DS body and writes to `target`, matching
/// the reference transform's source-based semantics.
pub fn apply_japanese_wiiu_guild_card_corrections(
    kind: GuildCardBodyKind,
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    apply_japanese_wiiu_guild_card_corrections_for_revision(
        kind,
        source,
        target,
        ConverterRevision::LAST_HISTORICAL,
    )?;
    apply_current_japanese_wiiu_guild_card_corrections(kind, source, target)
}

pub(crate) fn apply_current_japanese_wiiu_guild_card_corrections(
    kind: GuildCardBodyKind,
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    let expected_size = match kind {
        GuildCardBodyKind::Card => CARD_BODY_SIZE,
        GuildCardBodyKind::Cardbox => CARDBOX_BODY_SIZE,
    };
    if source.len() != expected_size || target.len() != expected_size {
        return Err(ConversionError::InvalidSave(format!(
            "MH3G {kind:?} body must be {expected_size} bytes, got source {} and target {}",
            source.len(),
            target.len()
        )));
    }
    if kind == GuildCardBodyKind::Card {
        apply_current_guild_card_monster_log_corrections(source, target)?;
    }
    Ok(())
}

pub(crate) fn apply_japanese_wiiu_guild_card_corrections_for_revision(
    kind: GuildCardBodyKind,
    source: &[u8],
    target: &mut [u8],
    revision: ConverterRevision,
) -> Result<(), ConversionError> {
    let (expected_size, swap4, arena4, swap2, crown): (
        usize,
        &[usize],
        &[usize],
        &[usize],
        &[usize],
    ) = match kind {
        GuildCardBodyKind::Card => (
            CARD_BODY_SIZE,
            &MEOW_CARD_SWAP4,
            &MEOW_CARD_ARENA4,
            &MEOW_CARD_SWAP2,
            &MEOW_CARD_CROWN,
        ),
        GuildCardBodyKind::Cardbox => (
            CARDBOX_BODY_SIZE,
            &MEOW_CARDBOX_SWAP4,
            &MEOW_CARDBOX_ARENA4,
            &MEOW_CARDBOX_SWAP2,
            &MEOW_CARDBOX_CROWN,
        ),
    };

    if source.len() != expected_size || target.len() != expected_size {
        return Err(ConversionError::InvalidSave(format!(
            "MH3G {kind:?} body must be {expected_size} bytes, got source {} and target {}",
            source.len(),
            target.len()
        )));
    }

    for &offset in swap4 {
        copy_reversed(source, target, offset, 4)?;
    }
    for &offset in arena4 {
        transform_arena4(source, target, offset)?;
    }
    for &offset in swap2 {
        copy_reversed(source, target, offset, 2)?;
    }
    for &offset in crown {
        transform_crown(source, target, offset)?;
    }

    if kind == GuildCardBodyKind::Card {
        apply_guild_card_hr_corrections(source, target)?;
        apply_guild_card_equipment_corrections(source, target)?;
        apply_guild_card_weapon_usage_corrections(source, target)?;
        apply_guild_card_monster_log_corrections(source, target, revision)?;
        if revision >= ConverterRevision::V0_0_4 {
            apply_guild_card_arena_corrections(source, target)?;
        }
        apply_guild_card_metadata_corrections(source, target)?;
    }

    Ok(())
}

fn apply_confirmed_numeric_and_record_corrections(
    source: &[u8],
    target: &mut [u8],
    revision: ConverterRevision,
) -> Result<(), ConversionError> {
    for candidate in 0..OFFLINE_HUNTER_CANDIDATE_ID_COUNT {
        copy_reversed(
            source,
            target,
            OFFLINE_HUNTER_CANDIDATE_IDS_START + candidate * 2,
            2,
        )?;
    }

    for offset in FULL_WIDTH_COUNTER_OFFSETS {
        copy_reversed(source, target, offset, 4)?;
    }

    for (index, monster_id) in MONSTER_IDS.into_iter().enumerate() {
        let slay_offset = MONSTER_SLAY_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
        let capture_offset = MONSTER_CAPTURE_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
        let size_offset = 0x5984 + monster_id * 4;
        for offset in [slay_offset, capture_offset, size_offset, size_offset + 2] {
            copy_reversed(source, target, offset, 2)?;
        }

        let discovery_offset = 0x81B4 + index * 10 + 8;
        if revision >= ConverterRevision::V0_0_5 {
            apply_historical_hunter_notes_display_state(
                source,
                target,
                slay_offset,
                capture_offset,
                discovery_offset,
            )?;
        } else {
            let slays =
                u16::from_le_bytes(source[slay_offset..slay_offset + 2].try_into().unwrap());
            let captures = u16::from_le_bytes(
                source[capture_offset..capture_offset + 2]
                    .try_into()
                    .unwrap(),
            );
            if slays != 0 || captures != 0 || source[discovery_offset] & 0x01 != 0 {
                target[discovery_offset] |= 0x80;
            }
        }
    }

    let deviljho_linked_size_offset = 0x5984 + DEVILJHO_LINKED_SIZE_CACHE_ID * 4;
    copy_reversed(source, target, deviljho_linked_size_offset, 2)?;
    copy_reversed(source, target, deviljho_linked_size_offset + 2, 2)?;

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

fn apply_historical_shakalaka_companion_corrections(
    source: &[u8],
    target: &mut [u8],
    revision: ConverterRevision,
) -> Result<(), ConversionError> {
    for companion in 0..SHAKALAKA_RECORD_COUNT {
        let record_start = SHAKALAKA_RECORD_START + companion * SHAKALAKA_RECORD_STRIDE;

        for relative in (0..HISTORICAL_SHAKALAKA_U32_HEADER_SIZE).step_by(4) {
            copy_reversed(source, target, record_start + relative, 4)?;
        }
        let scalar_end = if revision == ConverterRevision::V0_0_4 {
            HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET + 2
        } else {
            SHAKALAKA_MASK_STATE_START
        };
        for relative in (HISTORICAL_SHAKALAKA_U32_HEADER_SIZE..scalar_end).step_by(2) {
            copy_reversed(source, target, record_start + relative, 2)?;
        }
        if revision >= ConverterRevision::V0_0_6 {
            copy_reversed(
                source,
                target,
                record_start + HISTORICAL_SHAKALAKA_LAMP_SWAP_OFFSET,
                2,
            )?;
        }
    }

    Ok(())
}

/// Reassert the companion schema proven by paired official transfers.
///
/// This is intentionally layered after the historical converter replay. The
/// historical function above must stay byte-reproducible so compatibility
/// repair can still recognize 0.0.3-0.0.6 output, while current conversion
/// must use the corrected field boundaries.
fn apply_current_shakalaka_companion_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    for companion in 0..SHAKALAKA_RECORD_COUNT {
        let record_start = SHAKALAKA_RECORD_START + companion * SHAKALAKA_RECORD_STRIDE;

        copy_reversed(source, target, record_start, SHAKALAKA_U32_PREFIX_SIZE)?;
        for relative in (SHAKALAKA_U32_PREFIX_SIZE..SHAKALAKA_MASK_STATE_START).step_by(2) {
            copy_reversed(source, target, record_start + relative, 2)?;
        }
        target[record_start + SHAKALAKA_MASK_STATE_START..record_start + SHAKALAKA_MASK_STATE_END]
            .copy_from_slice(
                &source[record_start + SHAKALAKA_MASK_STATE_START
                    ..record_start + SHAKALAKA_MASK_STATE_END],
            );
    }

    Ok(())
}

/// Apply corrections proven by official-transfer pairs after the last
/// historically reproducible 0.0.6 conversion semantics.
///
/// Keep this separate from `apply_japanese_wiiu_corrections_for_revision`:
/// compatibility repair must still be able to recreate the exact 0.0.3-
/// 0.0.6 output before comparing it with the current Wii U save.
fn apply_current_official_transfer_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    apply_current_shakalaka_companion_corrections(source, target)?;

    target[MONSTER_GUIDE_PACKED_STATE_START
        ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN]
        .copy_from_slice(
            &source[MONSTER_GUIDE_PACKED_STATE_START
                ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN],
        );

    // Reassert the complete counter schema from the original source rather
    // than extending the historical MEOW table. This keeps 0.0.3-0.0.6 replay
    // byte-reproducible for compatibility detection while fixing every current
    // slay/capture lane, including the three historically omitted small
    // monsters (Giggi, Aptonoth, and Popo).
    for monster_id in 0..MONSTER_COUNT_ENTRY_COUNT {
        for table_start in [MONSTER_SLAY_COUNT_START, MONSTER_CAPTURE_COUNT_START] {
            copy_reversed(
                source,
                target,
                table_start + monster_id * MONSTER_COUNT_STRIDE,
                MONSTER_COUNT_STRIDE,
            )?;
        }
    }

    for index in 0..MONSTER_IDS.len() {
        apply_current_hunter_notes_display_state(source, target, 0x81B4 + index * 10 + 8)?;
    }

    for word in 0..ITEM_ACQUIRED_BITSET_WORD_COUNT {
        copy_reversed(
            source,
            target,
            ITEM_ACQUIRED_BITSET_START + word * ITEM_ACQUIRED_BITSET_WORD_SIZE,
            ITEM_ACQUIRED_BITSET_WORD_SIZE,
        )?;
    }

    for offset in PLAYER_APPEARANCE_SCALAR_OFFSETS {
        copy_reversed(source, target, offset, 4)?;
    }

    // 0x73D0 is not one u32: the leading style ID is a u16 while the final
    // two bytes are packed selectors. Reassert that field boundary after the
    // historical blanket four-byte transform.
    copy_reversed(source, target, PLAYER_APPEARANCE_PACKED_STYLE_OFFSET, 2)?;
    target[PLAYER_APPEARANCE_PACKED_STYLE_OFFSET + 2..PLAYER_APPEARANCE_PACKED_STYLE_OFFSET + 4]
        .copy_from_slice(
            &source[PLAYER_APPEARANCE_PACKED_STYLE_OFFSET + 2
                ..PLAYER_APPEARANCE_PACKED_STYLE_OFFSET + 4],
        );
    copy_reversed(source, target, PLAYER_APPEARANCE_RGBA_OFFSET, 4)?;

    Ok(())
}

/// Complete the statically recovered Wii U record corrections.
pub fn apply_japanese_wiiu_corrections(
    source: &[u8],
    target: &mut [u8],
) -> Result<(), ConversionError> {
    apply_japanese_wiiu_corrections_for_revision(
        source,
        target,
        ConverterRevision::LAST_HISTORICAL,
    )?;
    apply_current_official_transfer_corrections(source, target)
}

pub(crate) fn apply_japanese_wiiu_corrections_for_revision(
    source: &[u8],
    target: &mut [u8],
    revision: ConverterRevision,
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
    if revision >= ConverterRevision::V0_0_4 {
        apply_arena_record_table(
            source,
            target,
            USER_ARENA_RECORD_START,
            USER_ARENA_RECORD_COUNT,
        )?;
    }
    for offset in MEOW_USER_OFFICIAL_FIX_COPY1 {
        target[offset] = source[offset];
    }

    // Replay the exact historical Cha-Cha/Kayamba behavior here. Current
    // conversion corrects its field boundaries only in
    // `apply_current_official_transfer_corrections`, after the historical
    // result has been kept available for compatibility detection.
    if revision >= ConverterRevision::V0_0_4 {
        apply_historical_shakalaka_companion_corrections(source, target, revision)?;
    }

    // These fields are read as big-endian values by the Wii U title. MEOW v5
    // copies them unchanged, while the 3DS body stores their logical values in
    // little-endian order.
    preserve_event_state(source, target)?;
    remap_quest_completion(source, target)?;
    copy_reversed(source, target, SECOND_RGBA_OFFSET, 4)?;
    apply_offline_hunter_roster_corrections(source, target)?;
    apply_confirmed_numeric_and_record_corrections(source, target, revision)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        profile::PAYLOAD_SIZE,
        transform_table::{ARENA_RECORD_OFFSETS, MONSTER_DISCOVERY_OFFSETS},
    };

    use super::{
        DEVILJHO_BOOK_ITEM_ID, EQUIPMENT_BOX_START, EVENT_FLAG_START, GUILD_CARD_SLOT_SIZE,
        ITEM_ACQUIRED_BITSET_START, ITEM_ACQUIRED_BITSET_WORD_COUNT,
        ITEM_ACQUIRED_BITSET_WORD_SIZE, MONSTER_CAPTURE_COUNT_START, MONSTER_COUNT_ENTRY_COUNT,
        MONSTER_COUNT_STRIDE, MONSTER_GUIDE_PACKED_STATE_LEN, MONSTER_GUIDE_PACKED_STATE_START,
        MONSTER_IDS, MONSTER_SLAY_COUNT_START, PLAYER_APPEARANCE_PACKED_STYLE_OFFSET,
        PLAYER_APPEARANCE_RGBA_OFFSET, PLAYER_APPEARANCE_SCALAR_OFFSETS, QUEST_COMPLETION_START,
        SECOND_RGBA_OFFSET, apply_arena_records, apply_endian_swaps,
        apply_japanese_wiiu_corrections, apply_japanese_wiiu_corrections_for_revision,
        apply_japanese_wiiu_guild_card_slot_corrections, apply_monster_discovery,
    };
    use crate::revision::ConverterRevision;

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
    fn japanese_wiiu_corrections_preserve_signed_charm_points_as_raw_i8_bytes() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[EQUIPMENT_BOX_START..EQUIPMENT_BOX_START + 16].copy_from_slice(&[
            0x06, 0x03, 0x34, 0x12, 0x13, 0xF6, 0x20, 0x05, 0x34, 0x12, 0x78, 0x56, 0xBC, 0x9A,
            0xAA, 0x55,
        ]);
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[EQUIPMENT_BOX_START..EQUIPMENT_BOX_START + 16],
            &[
                0x06, 0x03, 0x12, 0x34, 0x13, 0xF6, 0x20, 0x05, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC,
                0xAA, 0x55,
            ]
        );
        assert_eq!(target[EQUIPMENT_BOX_START + 5] as i8, -10);
        assert_eq!(target[EQUIPMENT_BOX_START + 7] as i8, 5);
    }

    #[test]
    fn current_corrections_swap_every_item_acquisition_word() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        for word in 0..ITEM_ACQUIRED_BITSET_WORD_COUNT {
            let offset = ITEM_ACQUIRED_BITSET_START + word * ITEM_ACQUIRED_BITSET_WORD_SIZE;
            let value = 0x1020_3000_u32 + word as u32;
            source[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        source[ITEM_ACQUIRED_BITSET_START - 1] = 0x5A;
        source[ITEM_ACQUIRED_BITSET_START
            + ITEM_ACQUIRED_BITSET_WORD_COUNT * ITEM_ACQUIRED_BITSET_WORD_SIZE] = 0xA5;

        let mut historical = source.clone();
        apply_japanese_wiiu_corrections_for_revision(
            &source,
            &mut historical,
            ConverterRevision::V0_0_6,
        )
        .unwrap();
        let mut current = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut current).unwrap();

        for word in 0..ITEM_ACQUIRED_BITSET_WORD_COUNT {
            let offset = ITEM_ACQUIRED_BITSET_START + word * ITEM_ACQUIRED_BITSET_WORD_SIZE;
            assert_eq!(
                &current[offset..offset + 4],
                &source[offset..offset + 4]
                    .iter()
                    .rev()
                    .copied()
                    .collect::<Vec<_>>(),
                "item-acquisition word {word}"
            );
            assert_eq!(
                &historical[offset..offset + 4],
                &source[offset..offset + 4],
                "0.0.6 replay must remain byte-identical"
            );
        }
        assert_eq!(
            current[ITEM_ACQUIRED_BITSET_START - 1],
            historical[ITEM_ACQUIRED_BITSET_START - 1]
        );
        assert_eq!(
            current[ITEM_ACQUIRED_BITSET_START
                + ITEM_ACQUIRED_BITSET_WORD_COUNT * ITEM_ACQUIRED_BITSET_WORD_SIZE],
            historical[ITEM_ACQUIRED_BITSET_START
                + ITEM_ACQUIRED_BITSET_WORD_COUNT * ITEM_ACQUIRED_BITSET_WORD_SIZE]
        );
    }

    #[test]
    fn current_corrections_preserve_deviljho_book_unlock_bit() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let word_index = DEVILJHO_BOOK_ITEM_ID >> 5;
        let bit_index = DEVILJHO_BOOK_ITEM_ID & 31;
        let offset = ITEM_ACQUIRED_BITSET_START + word_index * ITEM_ACQUIRED_BITSET_WORD_SIZE;

        // Observed in the Yoruaski 3DS source: this word owns the Deviljho
        // book (bit 24) and the preceding book (bit 23), while the dummy item
        // at bit 15 remains absent.
        let source_word = 0xFFFF_7FFE_u32;
        source[offset..offset + 4].copy_from_slice(&source_word.to_le_bytes());

        let mut current = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut current).unwrap();
        let current_word = u32::from_be_bytes(current[offset..offset + 4].try_into().unwrap());

        assert_eq!(&current[offset..offset + 4], &[0xFF, 0xFF, 0x7F, 0xFE]);
        assert_eq!(current_word, source_word);
        assert_ne!(current_word & (1 << bit_index), 0, "Deviljho book");
        assert_ne!(current_word & (1 << (bit_index - 1)), 0, "preceding book");
        assert_eq!(current_word & (1 << 15), 0, "dummy item");
    }

    #[test]
    fn current_corrections_preserve_monster_guide_packed_state_bytes() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[MONSTER_GUIDE_PACKED_STATE_START
            ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN]
            .fill(0xFF);
        source[MONSTER_GUIDE_PACKED_STATE_START] = 0x02;
        source[MONSTER_GUIDE_PACKED_STATE_START + 1] = 0x00;
        source[MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN - 1] = 0x00;

        let mut historical = source.clone();
        apply_japanese_wiiu_corrections_for_revision(
            &source,
            &mut historical,
            ConverterRevision::V0_0_6,
        )
        .unwrap();
        let mut current = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut current).unwrap();

        let range = MONSTER_GUIDE_PACKED_STATE_START
            ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN;
        assert_eq!(&current[range.clone()], &source[range]);
        assert_eq!(
            &historical[MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN - 4
                ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN],
            &[0xFF, 0xFF, 0x00, 0xFF]
        );
    }

    #[test]
    fn current_corrections_convert_every_monster_count_lane_to_big_endian() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[MONSTER_GUIDE_PACKED_STATE_START
            ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN]
            .copy_from_slice(&[
                0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x5A, 0xA5, 0x11, 0x22, 0x33, 0x44,
                0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xF0, 0x0F, 0xC3, 0x3C,
            ]);
        for monster_id in 0..MONSTER_COUNT_ENTRY_COUNT {
            let slays = 0x1100_u16 + monster_id as u16;
            let captures = 0x2200_u16 + monster_id as u16;
            let slay_offset = MONSTER_SLAY_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
            let capture_offset = MONSTER_CAPTURE_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
            source[slay_offset..slay_offset + 2].copy_from_slice(&slays.to_le_bytes());
            source[capture_offset..capture_offset + 2].copy_from_slice(&captures.to_le_bytes());
        }
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        for monster_id in 0..MONSTER_COUNT_ENTRY_COUNT {
            let slay_offset = MONSTER_SLAY_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
            let capture_offset = MONSTER_CAPTURE_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
            assert_eq!(
                u16::from_be_bytes(target[slay_offset..slay_offset + 2].try_into().unwrap()),
                0x1100_u16 + monster_id as u16,
                "slay count for monster ID {monster_id:#04x}"
            );
            assert_eq!(
                u16::from_be_bytes(
                    target[capture_offset..capture_offset + 2]
                        .try_into()
                        .unwrap()
                ),
                0x2200_u16 + monster_id as u16,
                "capture count for monster ID {monster_id:#04x}"
            );
        }
        assert_eq!(
            &target[MONSTER_GUIDE_PACKED_STATE_START
                ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN],
            &source[MONSTER_GUIDE_PACKED_STATE_START
                ..MONSTER_GUIDE_PACKED_STATE_START + MONSTER_GUIDE_PACKED_STATE_LEN]
        );
    }

    #[test]
    fn current_corrections_fix_small_monster_counts_that_historically_saturated_to_9999() {
        const CASES: [(usize, u16); 3] = [(0x1A, 0x0584), (0x1B, 0x00EC), (0x1C, 0x015C)];
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        for (monster_id, value) in CASES {
            let offset = MONSTER_SLAY_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
            source[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        let mut historical = source.clone();
        apply_japanese_wiiu_corrections_for_revision(
            &source,
            &mut historical,
            ConverterRevision::V0_0_6,
        )
        .unwrap();
        let mut current = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut current).unwrap();

        for (monster_id, value) in CASES {
            let offset = MONSTER_SLAY_COUNT_START + monster_id * MONSTER_COUNT_STRIDE;
            assert_eq!(
                &historical[offset..offset + 2],
                &value.to_le_bytes(),
                "historical replay for monster ID {monster_id:#04x}"
            );
            assert!(
                u16::from_be_bytes(historical[offset..offset + 2].try_into().unwrap()) > 9999,
                "historical Wii U interpretation should reproduce the saturated UI value"
            );
            assert_eq!(
                &current[offset..offset + 2],
                &value.to_be_bytes(),
                "current conversion for monster ID {monster_id:#04x}"
            );
        }
    }

    #[test]
    fn current_corrections_match_official_appearance_field_boundaries() {
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        for (index, offset) in PLAYER_APPEARANCE_SCALAR_OFFSETS.into_iter().enumerate() {
            source[offset..offset + 4].copy_from_slice(&(0.15_f32 + index as f32).to_le_bytes());
        }
        source[PLAYER_APPEARANCE_PACKED_STYLE_OFFSET..PLAYER_APPEARANCE_PACKED_STYLE_OFFSET + 4]
            .copy_from_slice(&[0x08, 0x00, 0x08, 0x01]);
        source[PLAYER_APPEARANCE_RGBA_OFFSET..PLAYER_APPEARANCE_RGBA_OFFSET + 4]
            .copy_from_slice(&[0xFF, 0xE6, 0xEF, 0xFA]);

        let mut current = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut current).unwrap();

        for offset in PLAYER_APPEARANCE_SCALAR_OFFSETS {
            assert_eq!(
                &current[offset..offset + 4],
                &source[offset..offset + 4]
                    .iter()
                    .rev()
                    .copied()
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(
            &current
                [PLAYER_APPEARANCE_PACKED_STYLE_OFFSET..PLAYER_APPEARANCE_PACKED_STYLE_OFFSET + 4],
            &[0x00, 0x08, 0x08, 0x01]
        );
        assert_eq!(
            &current[PLAYER_APPEARANCE_RGBA_OFFSET..PLAYER_APPEARANCE_RGBA_OFFSET + 4],
            &[0xFA, 0xEF, 0xE6, 0xFF]
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
        source[discovery_offset] = 0x01;
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
        assert_eq!(
            u16::from_be_bytes(target[size_offset..size_offset + 2].try_into().unwrap()),
            100
        );
        assert_eq!(
            u16::from_be_bytes(target[size_offset + 2..size_offset + 4].try_into().unwrap()),
            112
        );
        assert_ne!(target[discovery_offset] & 0x80, 0);
    }

    #[test]
    fn current_hunter_notes_do_not_infer_discovery_from_hunt_counts() {
        const MONSTER_INDEX: usize = 2;
        const MONSTER_ID: usize = 0x2D;

        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let slay_offset = 0x5784 + MONSTER_ID * 2;
        let capture_offset = 0x5884 + MONSTER_ID * 2;
        let discovery_offset = 0x81B4 + MONSTER_INDEX * 10 + 8;
        source[slay_offset..slay_offset + 2].copy_from_slice(&9_u16.to_le_bytes());
        source[capture_offset..capture_offset + 2].copy_from_slice(&3_u16.to_le_bytes());

        let mut target = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(target[discovery_offset], 0x00);
        assert_eq!(MONSTER_IDS[MONSTER_INDEX], MONSTER_ID);
    }

    #[test]
    fn historical_hunter_notes_replay_keeps_released_counter_inference() {
        const MONSTER_INDEX: usize = 2;
        const MONSTER_ID: usize = 0x2D;

        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let slay_offset = 0x5784 + MONSTER_ID * 2;
        let discovery_offset = 0x81B4 + MONSTER_INDEX * 10 + 8;
        source[slay_offset..slay_offset + 2].copy_from_slice(&1_u16.to_le_bytes());

        let mut historical = source.clone();
        apply_japanese_wiiu_corrections_for_revision(
            &source,
            &mut historical,
            ConverterRevision::V0_0_6,
        )
        .unwrap();
        assert_eq!(historical[discovery_offset], 0x80);

        let mut current = source.clone();
        apply_japanese_wiiu_corrections(&source, &mut current).unwrap();
        assert_eq!(current[discovery_offset], 0x00);
    }

    #[test]
    fn hunter_notes_state_mapping_is_shared_by_personal_and_received_cards() {
        const MONSTER_INDEX: usize = 2;
        const MONSTER_ID: usize = 0x2D;
        const SOURCE_STATE: u8 = 0x0E;
        const EXPECTED_WIIU_STATE: u8 = 0x68;

        // The personal record stores counters and display state in separate
        // tables; a received card stores them together in a ten-byte row.
        // The physical offsets differ, but the crown/discovery mapping must
        // be identical across the personal card and an offline-hall partner.
        let mut personal_source = vec![0_u8; PAYLOAD_SIZE];
        let personal_state_offset = 0x81B4 + MONSTER_INDEX * 10 + 8;
        personal_source[personal_state_offset] = SOURCE_STATE;
        let mut personal_target = personal_source.clone();
        apply_japanese_wiiu_corrections(&personal_source, &mut personal_target).unwrap();

        let mut received_source = vec![0_u8; GUILD_CARD_SLOT_SIZE];
        let received_state_offset = 0x7C0 + MONSTER_INDEX * 10 + 8;
        received_source[received_state_offset] = SOURCE_STATE;
        let mut received_target = received_source.clone();
        apply_japanese_wiiu_guild_card_slot_corrections(&received_source, &mut received_target)
            .unwrap();

        assert_eq!(
            personal_target[personal_state_offset], EXPECTED_WIIU_STATE,
            "personal guild card"
        );
        assert_eq!(
            received_target[received_state_offset], EXPECTED_WIIU_STATE,
            "received/black-slave guild card"
        );
        // Keep the test's selected personal monster tied to the record order
        // rather than silently relying on an unrelated cache row.
        assert_eq!(MONSTER_IDS[MONSTER_INDEX], MONSTER_ID);
    }

    #[test]
    fn japanese_wiiu_corrections_swap_nonzero_physical_monster_sizes() {
        // The Cemu hunter-record UI consumes this cache as big-endian u16
        // pairs. A real active 3DS record uses little-endian [min, max].
        const MONSTER_ID: usize = 0x0c;
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        let size_offset = 0x5984 + MONSTER_ID * 4;
        source[size_offset..size_offset + 4].copy_from_slice(&[0x5b, 0x00, 0x7a, 0x00]);
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[size_offset..size_offset + 4],
            &[0x00, 0x5b, 0x00, 0x7a]
        );
    }

    #[test]
    fn japanese_wiiu_corrections_swap_deviljho_linked_size_cache() {
        // The game combines display monster 0x07 with non-display cache
        // record 0x47 when showing Deviljho's hunting-record size range.
        // The latter is not part of the 50-row display map, but is still
        // stored as a pair of endian-sensitive u16 values.
        const DEVILJHO_LINKED_MONSTER_ID: usize = 0x47;
        let size_offset = 0x5984 + DEVILJHO_LINKED_MONSTER_ID * 4;
        let mut source = vec![0_u8; PAYLOAD_SIZE];
        source[size_offset..size_offset + 4].copy_from_slice(&[0x59, 0x00, 0x78, 0x00]);
        let mut target = source.clone();

        apply_japanese_wiiu_corrections(&source, &mut target).unwrap();

        assert_eq!(
            &target[size_offset..size_offset + 4],
            &[0x00, 0x59, 0x00, 0x78]
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
