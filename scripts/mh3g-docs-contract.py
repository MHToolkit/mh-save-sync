#!/usr/bin/env python3
"""Fail closed when MH3G converter documentation drifts from its CLI contract."""

from __future__ import annotations

import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ROOT_DOCS = ("README.md", "README.zh-CN.md")
CONTRACT_DOCS = (
    "docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.md",
    "docs/MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md",
)
PACKAGE_DOCS = (
    "packaging/mh3g-save-convert/README-Windows.txt",
    "packaging/mh3g-save-convert/README-macOS.txt",
)
COMMANDS = (
    "inspect",
    "inspect-progress",
    "inspect-events",
    "inspect-cec",
    "convert",
    "convert-system",
    "convert-extras",
    "convert-cec",
    "rollback",
    "rollback-cec",
)
EXTDATA_SUFFIX = "extdata/00000000/00000481/user/"
CEC_SUFFIX = "CEC/00048100/"
WINDOWS_TEMPLATE = "packaging/mh3g-save-convert/README-Windows.txt"
DIRECT_ZIP_CLAIMS = (
    "ZIP input is supported",
    "ZIP archives are supported directly",
    "direct ZIP import",
    "直接支持 ZIP",
    "支持直接读取 ZIP",
    "可直接读取 ZIP",
)


def read_required(relative_path: str, failures: list[str]) -> str | None:
    path = ROOT / relative_path
    if not path.is_file():
        failures.append(f"missing required documentation: {relative_path}")
        return None
    return path.read_text(encoding="utf-8")


def require_contains(
    failures: list[str], relative_path: str, content: str, token: str, purpose: str
) -> None:
    if token not in content:
        failures.append(f"{relative_path}: missing {purpose}: {token}")


def main() -> int:
    failures: list[str] = []
    root_docs = {
        relative_path: read_required(relative_path, failures)
        for relative_path in ROOT_DOCS
    }
    contract_docs = {
        relative_path: read_required(relative_path, failures)
        for relative_path in CONTRACT_DOCS
    }
    for relative_path in PACKAGE_DOCS:
        read_required(relative_path, failures)

    english = root_docs["README.md"]
    chinese = root_docs["README.zh-CN.md"]
    if english is not None:
        require_contains(
            failures,
            "README.md",
            english,
            "[简体中文](README.zh-CN.md)",
            "Chinese language link",
        )
    if chinese is not None:
        require_contains(
            failures,
            "README.zh-CN.md",
            chinese,
            "[English](README.md)",
            "English language link",
        )

    english_contract = contract_docs[CONTRACT_DOCS[0]]
    chinese_contract = contract_docs[CONTRACT_DOCS[1]]
    if english_contract is not None:
        require_contains(
            failures,
            CONTRACT_DOCS[0],
            english_contract,
            "[简体中文](MH3G_3DS_TO_CEMU_FILE_CONTRACT.zh-CN.md)",
            "Chinese contract link",
        )
    if chinese_contract is not None:
        require_contains(
            failures,
            CONTRACT_DOCS[1],
            chinese_contract,
            "[English](MH3G_3DS_TO_CEMU_FILE_CONTRACT.md)",
            "English contract link",
        )

    for relative_path, content in root_docs.items():
        if content is None:
            continue
        for command in COMMANDS:
            require_contains(
                failures, relative_path, content, f"`{command}`", "command reference"
            )
        require_contains(
            failures, relative_path, content, EXTDATA_SUFFIX, "ExtData input suffix"
        )
        require_contains(
            failures, relative_path, content, CEC_SUFFIX, "CEC input suffix")
        for claim in DIRECT_ZIP_CLAIMS:
            if claim in content:
                failures.append(f"{relative_path}: unsupported direct ZIP claim: {claim}")

    workflow_path = ".github/workflows/mh3g-converter-windows.yml"
    workflow = read_required(workflow_path, failures)
    if workflow is not None:
        require_contains(
            failures,
            workflow_path,
            workflow,
            WINDOWS_TEMPLATE,
            "tracked Windows package README template",
        )

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1

    print("mh3g documentation contract: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
