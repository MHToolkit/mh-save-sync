#!/usr/bin/env python3
"""Regression tests for the MH3G Converter patch-release version helper."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts" / "mh3g-converter-version.py"


class Mh3gConverterVersionTests(unittest.TestCase):
    def manifest(self, version: str) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temporary = tempfile.TemporaryDirectory()
        path = Path(temporary.name) / "Cargo.toml"
        path.write_text(
            "[package]\nname = \"mh3g-save-convert\"\n"
            f"version = \"{version}\"\n"
            "edition = \"2024\"\n",
            encoding="utf-8",
        )
        return temporary, path

    def run_tool(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(TOOL), *args],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_prints_current_patch_version(self) -> None:
        temporary, manifest = self.manifest("0.0.7")
        with temporary:
            result = self.run_tool("--manifest", str(manifest), "--print")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "0.0.7\n")

    def test_calculates_next_patch_without_mutating_manifest(self) -> None:
        temporary, manifest = self.manifest("0.0.7")
        with temporary:
            result = self.run_tool("--manifest", str(manifest), "--next-patch")
            after = manifest.read_text(encoding="utf-8")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "0.0.8\n")
        self.assertIn('version = "0.0.7"', after)

    def test_writes_the_exact_next_patch_version(self) -> None:
        temporary, manifest = self.manifest("0.0.7")
        with temporary:
            result = self.run_tool("--manifest", str(manifest), "--next-patch", "--write")
            after = manifest.read_text(encoding="utf-8")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "0.0.8\n")
        self.assertIn('version = "0.0.8"', after)
        self.assertNotIn('version = "0.0.7"', after)

    def test_rejects_a_non_semver_package_version(self) -> None:
        temporary, manifest = self.manifest("0.0.7-dev")
        with temporary:
            result = self.run_tool("--manifest", str(manifest), "--next-patch")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected MAJOR.MINOR.PATCH", result.stderr)


if __name__ == "__main__":
    unittest.main()
