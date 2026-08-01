#!/usr/bin/env python3
"""Build an auditable "continued playing on Wii U" MH3G save fixture.

The original 3DS slot is read only and used solely for an integrity hash. The
output is a copy of an existing Cemu save directory, optionally overlaid with a
second real Cemu snapshot, plus small changes to already documented Wii U
fields. Real save bytes and generated fixtures must stay outside Git.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path
from typing import Any


CEMU_HEADER_SIZE = 0x28
THREE_DS_SLOT_SIZE = 0x8A00
COMPONENT_SIZES = {
    "user1": 0x8A24,
    "system": 0x3024,
    "card1": 0x58024,
    "card2": 0x58024,
    "card3": 0x58024,
    "cardbox": 0x30024,
    "quest1": 0x29024,
    "quest2": 0x29024,
    "quest3": 0x29024,
    "quest4": 0x29024,
    "cec": 0x83624,
}
REQUIRED_COMPONENTS = tuple(COMPONENT_SIZES)

RESOURCE_COUNTER_OFFSET = CEMU_HEADER_SIZE + 0x5BA4
QUEST_COMPLETION_OFFSET = CEMU_HEADER_SIZE + 0x6E5C
CARD_SLOT_COUNT = 0x62
CARD_SLOT_SIZE = 0xE00
CARD_SUMMARY_OFFSET = CARD_SLOT_COUNT * CARD_SLOT_SIZE + 0x0C
CARD_SUMMARY_COUNT = 33
CARD_SUMMARY_STRIDE = 0x38
CARD_FRIENDSHIP_OFFSET = 0x28
CARDBOX_SCALAR_OFFSETS = (
    1976,
    1978,
    1980,
    1982,
    1986,
    1988,
    1990,
    1992,
)
CEC_HEADER_SIZE = 40
CEC_RECORD_AREA_OFFSET = 0x1FC
CEC_RECORD_SLOT_SIZE = 0x2A00
CEC_RECORD_SLOT_COUNT = 50
CEC_CARD_COUNT = 3
CEC_WEAPON_USAGE_OFFSET = 0x12C
CEC_WEAPON_USAGE_COUNT = 36
USER_ARENA_RECORD_START = 0x83A8
USER_ARENA_RECORD_COUNT = 110
USER_MONSTER_LOG_START = 0x81B4
USER_MONSTER_LOG_COUNT = 50
USER_MONSTER_LOG_STRIDE = 10
SHAKALAKA_RECORD_START = 0x6F44
SHAKALAKA_RECORD_COUNT = 2
SHAKALAKA_RECORD_STRIDE = 0x148
SHAKALAKA_MASK_STATE_START = 0xDE
SHAKALAKA_LAMP_MASK_MASTERY_END = 0xE6
CARD_MONSTER_LOG_START = 0x7C0
CARD_MONSTER_LOG_COUNT = 50
CARD_MONSTER_LOG_STRIDE = 10
CARD_ARENA_RECORD_START = 0x9B4
CARD_ARENA_RECORD_COUNT = 110


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def read_component(directory: Path, name: str) -> bytearray:
    path = directory / name
    if not path.is_file():
        raise ValueError(f"missing required Wii U component: {path}")
    data = bytearray(path.read_bytes())
    expected = COMPONENT_SIZES[name]
    if len(data) != expected:
        raise ValueError(
            f"invalid {name} size at {path}: expected {expected}, got {len(data)}"
        )
    return data


def write_component(directory: Path, name: str, data: bytearray) -> None:
    expected = COMPONENT_SIZES[name]
    if len(data) != expected:
        raise ValueError(
            f"refusing to write invalid {name} size: expected {expected}, got {len(data)}"
        )
    (directory / name).write_bytes(data)


def compatibility_repair_offsets(name: str) -> set[int]:
    offsets: set[int] = set()
    if name == "user1":
        arena = CEMU_HEADER_SIZE + USER_ARENA_RECORD_START
        offsets.update(range(arena, arena + USER_ARENA_RECORD_COUNT * 4))
        for row in range(USER_MONSTER_LOG_COUNT):
            offsets.add(
                CEMU_HEADER_SIZE
                + USER_MONSTER_LOG_START
                + row * USER_MONSTER_LOG_STRIDE
                + 8
            )
        for companion in range(SHAKALAKA_RECORD_COUNT):
            start = (
                CEMU_HEADER_SIZE
                + SHAKALAKA_RECORD_START
                + companion * SHAKALAKA_RECORD_STRIDE
            )
            offsets.update(range(start, start + SHAKALAKA_LAMP_MASK_MASTERY_END))
    elif name in {"card1", "card2", "card3"}:
        for slot in range(CARD_SLOT_COUNT):
            start = CEMU_HEADER_SIZE + slot * CARD_SLOT_SIZE
            arena = start + CARD_ARENA_RECORD_START
            offsets.update(range(arena, arena + CARD_ARENA_RECORD_COUNT * 4))
            for row in range(CARD_MONSTER_LOG_COUNT):
                offsets.add(
                    start
                    + CARD_MONSTER_LOG_START
                    + row * CARD_MONSTER_LOG_STRIDE
                    + 8
                )
    return offsets


def overlay_played_component(
    output: Path, played: Path, name: str
) -> dict[str, Any]:
    baseline_data = read_component(output, name)
    played_data = read_component(played, name)
    protected = compatibility_repair_offsets(name)
    applied = 0
    protected_differences = 0
    for offset, (before, after) in enumerate(zip(baseline_data, played_data)):
        if before == after:
            continue
        if offset in protected:
            protected_differences += 1
            continue
        baseline_data[offset] = after
        applied += 1
    write_component(output, name, baseline_data)
    return {
        "component": name,
        "applied_bytes": applied,
        "preserved_historical_repair_bytes": protected_differences,
    }


def increment_be(data: bytearray, offset: int, width: int) -> tuple[int, int]:
    old = int.from_bytes(data[offset : offset + width], "big")
    maximum = (1 << (width * 8)) - 1
    new = old + 1 if old < maximum else old - 1
    data[offset : offset + width] = new.to_bytes(width, "big")
    return old, new


def mutate_user_slot(output: Path, quest_catalog: Path) -> list[dict[str, Any]]:
    data = read_component(output, "user1")
    changes: list[dict[str, Any]] = []

    old, new = increment_be(data, RESOURCE_COUNTER_OFFSET, 4)
    changes.append(
        {
            "component": "user1",
            "field": "moga-resource-counter",
            "offset": RESOURCE_COUNTER_OFFSET,
            "width": 4,
            "before": old,
            "after": new,
        }
    )

    catalog = json.loads(quest_catalog.read_text(encoding="utf-8"))
    selected: dict[str, Any] | None = None
    for quest in catalog:
        table_index = int(quest["target_table_index"])
        word_offset = QUEST_COMPLETION_OFFSET + (table_index // 32) * 4
        bit = table_index % 32
        word = int.from_bytes(data[word_offset : word_offset + 4], "big")
        if word & (1 << bit) == 0:
            data[word_offset : word_offset + 4] = (word | (1 << bit)).to_bytes(
                4, "big"
            )
            selected = {
                "component": "user1",
                "field": "completed-quest",
                "offset": word_offset,
                "width": 4,
                "quest_id": quest["quest_id"],
                "title_en": quest.get("title_en"),
                "target_table_index": table_index,
                "before": word,
                "after": word | (1 << bit),
            }
            break
    if selected is None:
        raise ValueError("user1 has no unset catalogued quest bit to advance")
    changes.append(selected)

    write_component(output, "user1", data)
    return changes


def mutate_card_friendship(output: Path) -> dict[str, Any]:
    data = read_component(output, "card3")
    candidates: list[tuple[int, int]] = []
    for record in range(CARD_SUMMARY_COUNT):
        offset = (
            CEMU_HEADER_SIZE
            + CARD_SUMMARY_OFFSET
            + record * CARD_SUMMARY_STRIDE
            + CARD_FRIENDSHIP_OFFSET
        )
        value = int.from_bytes(data[offset : offset + 4], "big")
        candidates.append((record, value))
    record, _ = next(
        ((record, value) for record, value in candidates if 0 < value < 0xFFFFFFFF),
        candidates[0],
    )
    offset = (
        CEMU_HEADER_SIZE
        + CARD_SUMMARY_OFFSET
        + record * CARD_SUMMARY_STRIDE
        + CARD_FRIENDSHIP_OFFSET
    )
    old, new = increment_be(data, offset, 4)
    write_component(output, "card3", data)
    return {
        "component": "card3",
        "field": "received-card-friendship",
        "record": record,
        "offset": offset,
        "width": 4,
        "before": old,
        "after": new,
    }


def mutate_cardbox_scalar(output: Path) -> dict[str, Any]:
    data = read_component(output, "cardbox")
    candidates = [
        (offset, int.from_bytes(data[CEMU_HEADER_SIZE + offset : CEMU_HEADER_SIZE + offset + 2], "big"))
        for offset in CARDBOX_SCALAR_OFFSETS
    ]
    payload_offset, _ = next(
        ((offset, value) for offset, value in candidates if 0 < value < 0xFFFF),
        candidates[0],
    )
    offset = CEMU_HEADER_SIZE + payload_offset
    old, new = increment_be(data, offset, 2)
    write_component(output, "cardbox", data)
    return {
        "component": "cardbox",
        "field": "compact-card-u16-scalar",
        "offset": offset,
        "width": 2,
        "before": old,
        "after": new,
    }


def seed_cec_from_received_card(output: Path, data: bytearray) -> dict[str, Any]:
    card_data = read_component(output, "card1")
    card_payload = card_data[CEMU_HEADER_SIZE:]
    selected: tuple[int, bytes] | None = None
    for slot in range(CARD_SLOT_COUNT):
        start = slot * CARD_SLOT_SIZE
        card = bytes(card_payload[start : start + CARD_SLOT_SIZE])
        if any(card):
            selected = (slot, card)
            break
    if selected is None:
        raise ValueError("card1 contains no non-empty card slot for the CEC fixture")

    source_slot, card = selected
    target_offset = CEC_HEADER_SIZE + CEC_RECORD_AREA_OFFSET
    data[target_offset : target_offset + CARD_SLOT_SIZE] = card
    return {
        "component": "cec",
        "field": "seed-received-card-record",
        "source_component": "card1",
        "source_card_slot": source_slot,
        "target_record": 0,
        "target_card": 0,
        "offset": target_offset,
        "width": CARD_SLOT_SIZE,
    }


def mutate_cec_weapon_usage(output: Path) -> list[dict[str, Any]]:
    data = read_component(output, "cec")
    changes: list[dict[str, Any]] = []
    record_area = data[
        CEC_HEADER_SIZE
        + CEC_RECORD_AREA_OFFSET : CEC_HEADER_SIZE
        + CEC_RECORD_AREA_OFFSET
        + CEC_RECORD_SLOT_COUNT * CEC_RECORD_SLOT_SIZE
    ]
    if not any(record_area):
        changes.append(seed_cec_from_received_card(output, data))

    selected: tuple[int, int, int, int] | None = None
    fallback: tuple[int, int, int, int] | None = None

    for record in range(CEC_RECORD_SLOT_COUNT):
        record_start = CEC_HEADER_SIZE + CEC_RECORD_AREA_OFFSET + record * CEC_RECORD_SLOT_SIZE
        record_bytes = data[record_start : record_start + CEC_RECORD_SLOT_SIZE]
        if not any(record_bytes):
            continue
        for card in range(CEC_CARD_COUNT):
            card_start = record_start + card * CARD_SLOT_SIZE
            card_bytes = data[card_start : card_start + CARD_SLOT_SIZE]
            if not any(card_bytes):
                continue
            for usage in range(CEC_WEAPON_USAGE_COUNT):
                offset = card_start + CEC_WEAPON_USAGE_OFFSET + usage * 2
                value = int.from_bytes(data[offset : offset + 2], "big")
                if fallback is None:
                    fallback = (record, card, usage, offset)
                if 0 < value < 0xFFFF:
                    selected = (record, card, usage, offset)
                    break
            if selected is not None:
                break
        if selected is not None:
            break

    chosen = selected or fallback
    if chosen is None:
        raise ValueError("CEC contains no non-empty record/card to advance")
    record, card, usage, offset = chosen
    old, new = increment_be(data, offset, 2)
    write_component(output, "cec", data)
    changes.append(
        {
            "component": "cec",
            "field": "received-card-weapon-usage",
            "record": record,
            "card": card,
            "weapon_index": usage,
            "offset": offset,
            "width": 2,
            "before": old,
            "after": new,
        }
    )
    return changes


def validate_directory(directory: Path) -> None:
    for name in REQUIRED_COMPONENTS:
        read_component(directory, name)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-3ds", type=Path, required=True)
    parser.add_argument("--baseline-wiiu-dir", type=Path, required=True)
    parser.add_argument("--played-wiiu-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument(
        "--quest-catalog",
        type=Path,
        default=Path(__file__).resolve().parents[1]
        / "crates/mh3g-save-convert/data/quest_catalog.json",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    source = args.source_3ds.resolve()
    baseline = args.baseline_wiiu_dir.resolve()
    played = args.played_wiiu_dir.resolve()
    output = args.output_dir.resolve()
    manifest = args.manifest.resolve()

    if not source.is_file() or source.stat().st_size != THREE_DS_SLOT_SIZE:
        raise ValueError(
            f"source 3DS user1 must be a {THREE_DS_SLOT_SIZE}-byte file: {source}"
        )
    if output.exists():
        raise ValueError(f"output directory already exists: {output}")
    if manifest.exists():
        raise ValueError(f"manifest already exists: {manifest}")
    validate_directory(baseline)
    validate_directory(played)

    source_hash_before = sha256(source)
    baseline_hashes = {name: sha256(baseline / name) for name in REQUIRED_COMPONENTS}
    played_hashes = {name: sha256(played / name) for name in REQUIRED_COMPONENTS}

    shutil.copytree(baseline, output)
    overlays: list[dict[str, Any]] = []
    for name in REQUIRED_COMPONENTS:
        if played_hashes[name] != baseline_hashes[name]:
            overlays.append(overlay_played_component(output, played, name))

    mutations = mutate_user_slot(output, args.quest_catalog.resolve())
    mutations.append(mutate_card_friendship(output))
    mutations.append(mutate_cardbox_scalar(output))
    mutations.extend(mutate_cec_weapon_usage(output))
    validate_directory(output)

    source_hash_after = sha256(source)
    if source_hash_after != source_hash_before:
        raise RuntimeError("original 3DS source changed while building the fixture")

    output_hashes = {name: sha256(output / name) for name in REQUIRED_COMPONENTS}
    changed = [
        name
        for name in REQUIRED_COMPONENTS
        if output_hashes[name] != baseline_hashes[name]
    ]
    required_classes = {
        "user1": {"user1"},
        "system": {"system"},
        "cards": {"card1", "card2", "card3"},
        "cardbox": {"cardbox"},
        "quests": {"quest1", "quest2", "quest3", "quest4"},
        "cec": {"cec"},
    }
    missing_classes = [
        label for label, names in required_classes.items() if names.isdisjoint(changed)
    ]
    if missing_classes:
        raise ValueError(
            "fixture does not change every required component class: "
            + ", ".join(missing_classes)
        )

    report = {
        "operation": "build-mh3g-continued-wiiu-fixture",
        "source_3ds": str(source),
        "source_3ds_sha256_before": source_hash_before,
        "source_3ds_sha256_after": source_hash_after,
        "baseline_wiiu_dir": str(baseline),
        "played_wiiu_dir": str(played),
        "output_dir": str(output),
        "overlay_components": overlays,
        "changed_components": changed,
        "mutations": mutations,
        "files": {
            name: {
                "baseline_sha256": baseline_hashes[name],
                "played_sha256": played_hashes[name],
                "output_sha256": output_hashes[name],
                "size": COMPONENT_SIZES[name],
            }
            for name in REQUIRED_COMPONENTS
        },
    }
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
