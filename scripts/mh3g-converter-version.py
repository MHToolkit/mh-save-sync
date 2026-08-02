#!/usr/bin/env python3
"""Read and patch-bump the MH3G Converter package version safely.

The release workflow deliberately uses the package manifest as its only version
source.  Keeping this tiny dependency-free helper in the repository makes the
same rule available to local packaging, CI, and regression tests.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path


SEMVER = re.compile(r"^(?P<major>0|[1-9][0-9]*)\.(?P<minor>0|[1-9][0-9]*)\.(?P<patch>0|[1-9][0-9]*)$")
SECTION = re.compile(r"(?m)^\[package\]\s*$")
NEXT_SECTION = re.compile(r"(?m)^\[")
VERSION = re.compile(r'(?m)^(?P<prefix>\s*version\s*=\s*")(?P<value>[^"]+)(?P<suffix>"\s*(?:#.*)?)$')


def package_version_location(text: str) -> re.Match[str]:
    section = SECTION.search(text)
    if section is None:
        raise ValueError("Cargo manifest has no [package] section")
    next_section = NEXT_SECTION.search(text, section.end())
    end = next_section.start() if next_section is not None else len(text)
    matches = list(VERSION.finditer(text, section.end(), end))
    if len(matches) != 1:
        raise ValueError("[package] must contain exactly one version field")
    return matches[0]


def read_version(manifest: Path) -> tuple[str, str, re.Match[str]]:
    text = manifest.read_text(encoding="utf-8")
    match = package_version_location(text)
    version = match.group("value")
    if SEMVER.fullmatch(version) is None:
        raise ValueError(f"expected MAJOR.MINOR.PATCH package version, got {version!r}")
    return text, version, match


def next_patch(version: str) -> str:
    match = SEMVER.fullmatch(version)
    assert match is not None
    return f"{match.group('major')}.{match.group('minor')}.{int(match.group('patch')) + 1}"


def atomic_write(path: Path, text: str) -> None:
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=path.parent, prefix=f".{path.name}.", delete=False
    ) as temporary:
        temporary.write(text)
        temporary_path = Path(temporary.name)
    try:
        os.chmod(temporary_path, path.stat().st_mode)
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "crates/mh3g-save-convert/Cargo.toml",
        help="package Cargo.toml (default: MH3G Converter manifest)",
    )
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--print", action="store_true", help="print the current exact package version")
    action.add_argument("--next-patch", action="store_true", help="print the next patch version")
    parser.add_argument("--write", action="store_true", help="rewrite --next-patch to the manifest atomically")
    args = parser.parse_args()
    if args.write and not args.next_patch:
        parser.error("--write requires --next-patch")

    try:
        text, current, match = read_version(args.manifest)
        result = next_patch(current) if args.next_patch else current
        if args.write:
            rewritten = f"{match.group('prefix')}{result}{match.group('suffix')}"
            atomic_write(args.manifest, text[: match.start()] + rewritten + text[match.end() :])
    except (OSError, ValueError) as error:
        print(f"mh3g-converter-version: {error}", file=sys.stderr)
        return 2

    print(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
