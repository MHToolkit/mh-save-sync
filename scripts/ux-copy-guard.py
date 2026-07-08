#!/usr/bin/env python3
"""Fail if first-run Chinese UX copy regresses to internal sync jargon."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = [
    ROOT / "apps/android/app/src/main/java/org/mhtoolkit/savesync/MainActivity.kt",
    ROOT / "apps/android/app/src/main/java/org/mhtoolkit/savesync/SyncMessages.kt",
    ROOT / "apps/android/app/src/main/java/org/mhtoolkit/savesync/SyncServerProbe.kt",
    ROOT / "apps/android/app/src/main/java/org/mhtoolkit/savesync/ReconcileWorker.kt",
    ROOT / "apps/macos/Sources/MHSaveSyncMac/main.swift",
]

FORBIDDEN = [
    "CAS",
    "HEAD",
    "SAF",
    "staging",
    "manifest/hash",
    "/ready",
    "查看冲突处理示例",
    "今天 21:18",
    "parent=mac-上一版",
    "锁定",
    "标记会话",
    "同步会话",
]

STRING_LITERAL_RE = re.compile(r'"(?:\\.|[^"\\])*"', re.S)
HAS_CJK_RE = re.compile(r"[\u4e00-\u9fff]")


def decode_literal(raw: str) -> str:
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw.strip('"')


violations: list[tuple[str, int, str, str]] = []
for path in FILES:
    text = path.read_text(encoding="utf-8")
    line_starts = [0]
    for match in re.finditer(r"\n", text):
        line_starts.append(match.end())
    for match in STRING_LITERAL_RE.finditer(text):
        value = decode_literal(match.group(0))
        if not HAS_CJK_RE.search(value):
            continue
        for term in FORBIDDEN:
            if term in value:
                line_no = 1 + sum(start <= match.start() for start in line_starts)
                violations.append((str(path.relative_to(ROOT)), line_no, term, value))

if violations:
    for rel, line_no, term, value in violations:
        print(f"{rel}:{line_no}: forbidden UX term {term!r} in Chinese copy: {value}")
    raise SystemExit(1)

print(
    json.dumps(
        {
            "ux_copy_guard": True,
            "checked_files": [str(path.relative_to(ROOT)) for path in FILES],
            "forbidden_terms": FORBIDDEN,
        },
        ensure_ascii=False,
        sort_keys=True,
    )
)
