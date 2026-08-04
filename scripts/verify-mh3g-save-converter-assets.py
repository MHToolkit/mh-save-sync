#!/usr/bin/env python3
"""Verify the self-authored assets used by the native MH3G converter shells."""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path


ARTWORK_NAMES = (
    "input-route.png",
    "components-workshop.png",
    "dry-run-flow.png",
    "rollback-harbor.png",
    "cec-mailbox.png",
)
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
REQUIRED_ICO_SIZES = {16, 32, 48, 256}


def parse_png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    if len(data) < 24 or data[:8] != PNG_SIGNATURE or data[12:16] != b"IHDR":
        raise ValueError("not a PNG with an IHDR chunk")
    return struct.unpack(">II", data[16:24])


def parse_ico_sizes(path: Path) -> set[int]:
    data = path.read_bytes()
    if len(data) < 6:
        raise ValueError("ICO header is truncated")
    reserved, image_type, count = struct.unpack("<HHH", data[:6])
    if reserved != 0 or image_type != 1:
        raise ValueError("not an ICO image")
    if len(data) < 6 + count * 16:
        raise ValueError("ICO directory is truncated")
    sizes = set()
    for index in range(count):
        width, height = data[6 + index * 16], data[7 + index * 16]
        sizes.add(256 if width == 0 else width)
        if width != height:
            raise ValueError("ICO contains a non-square image")
    return sizes


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.repo_root.resolve()
    artwork_dir = root / "apps/mh3g-save-converter-macos/Resources/Artwork"
    mac_icon = root / "apps/mh3g-save-converter-macos/Resources/AppIcon/MH3GSaveConverter.icns"
    windows_icon = root / "apps/mh3g-save-converter-windows/Assets/MH3GSaveConverter.ico"

    failures: list[str] = []
    for name in ARTWORK_NAMES:
        path = artwork_dir / name
        if not path.is_file() or path.stat().st_size == 0:
            failures.append(f"missing artwork: {path}")
            continue
        try:
            width, height = parse_png_size(path)
        except ValueError as error:
            failures.append(f"invalid artwork {path}: {error}")
            continue
        if width < 800 or height < 450:
            failures.append(f"artwork is too small: {path} is {width}x{height}")

    if not mac_icon.is_file() or mac_icon.stat().st_size == 0:
        failures.append(f"missing macOS icon: {mac_icon}")
    elif mac_icon.read_bytes()[:4] != b"icns":
        failures.append(f"invalid macOS icon: {mac_icon}")

    if not windows_icon.is_file() or windows_icon.stat().st_size == 0:
        failures.append(f"missing Windows icon: {windows_icon}")
    else:
        try:
            ico_sizes = parse_ico_sizes(windows_icon)
        except ValueError as error:
            failures.append(f"invalid Windows icon {windows_icon}: {error}")
        else:
            missing_sizes = REQUIRED_ICO_SIZES - ico_sizes
            if missing_sizes:
                failures.append(
                    f"Windows icon is missing required sizes: {', '.join(map(str, sorted(missing_sizes)))}"
                )

    if failures:
        print("MH3G converter asset verification failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("MH3G converter asset verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
