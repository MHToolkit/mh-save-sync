#!/usr/bin/env python3
"""Behavioral tests for the macOS converter bundle smoke script."""

from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
SMOKE_SCRIPT = REPO_ROOT / "scripts" / "mh3g-save-converter-macos-smoke.sh"


FAKE_CLI = r'''#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path


def record_call():
    with Path(os.environ["SMOKE_TRACE"]).open("a", encoding="utf-8") as trace:
        trace.write(" ".join(sys.argv[1:]) + "\n")


def option(name):
    return Path(sys.argv[sys.argv.index(name) + 1])


record_call()
arguments = sys.argv[1:]

if arguments == ["--help"]:
    print("mh3g-save-convert fake help")
elif arguments and arguments[0] == "inspect":
    print(json.dumps({"status": "inspected"}))
elif arguments and arguments[0] == "convert" and "--dry-run" in arguments:
    print(json.dumps({"status": "dry-run"}))
elif arguments and arguments[0] == "convert" and "--write" in arguments:
    if os.environ["SMOKE_FAKE_MODE"] == "emulator-guard":
        print(
            "unsafe install refused: emulator process is running: Nemessix",
            file=sys.stderr,
        )
        raise SystemExit(1)

    target = option("--output")
    target.write_bytes(b"synthetic converted save")
    target.with_name(".user2.mh3g-install.json").write_text("{}", encoding="utf-8")
    print(json.dumps({"status": "written"}))
elif arguments and arguments[0] == "rollback":
    manifest = option("--manifest")
    manifest.with_name("user2").unlink(missing_ok=True)
    manifest.unlink(missing_ok=True)
    print(json.dumps({"status": "rolled-back"}))
else:
    print(f"unexpected fake CLI arguments: {arguments}", file=sys.stderr)
    raise SystemExit(2)
'''


FAKE_UI = r'''#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

if sys.argv[1:] != ["--diagnostics"]:
    raise SystemExit(2)

with Path(os.environ["SMOKE_TRACE"]).open("a", encoding="utf-8") as trace:
    trace.write("ui --diagnostics\\n")

print(json.dumps({"ui_version": "test", "cli_version": "mh3g-save-convert test"}))
'''


class MacOSConverterSmokeTests(unittest.TestCase):
    def create_fake_app(self, directory: Path) -> Path:
        app_root = directory / "bundle-root"
        macos = app_root / "MH3G Save Converter.app" / "Contents" / "MacOS"
        macos.mkdir(parents=True)

        cli = macos / "mh3g-save-convert"
        ui = macos / "MH3GSaveConverterMac"
        cli.write_text(FAKE_CLI, encoding="utf-8")
        ui.write_text(FAKE_UI, encoding="utf-8")
        cli.chmod(cli.stat().st_mode | stat.S_IXUSR)
        ui.chmod(ui.stat().st_mode | stat.S_IXUSR)
        return app_root

    def run_smoke(self, app_root: Path, mode: str, trace: Path) -> subprocess.CompletedProcess[str]:
        environment = os.environ.copy()
        environment.update({"SMOKE_FAKE_MODE": mode, "SMOKE_TRACE": str(trace)})
        return subprocess.run(
            ["bash", str(SMOKE_SCRIPT), str(app_root)],
            check=False,
            capture_output=True,
            env=environment,
            text=True,
        )

    def test_emulator_guard_is_a_successful_safety_smoke_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            directory = Path(temporary_directory)
            trace = directory / "guard.trace"
            result = self.run_smoke(
                self.create_fake_app(directory), "emulator-guard", trace
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("emulator safety guard verified", result.stdout)
            self.assertNotIn("JSONDecodeError", result.stdout + result.stderr)

            calls = trace.read_text(encoding="utf-8")
            self.assertIn("--help", calls)
            self.assertIn("inspect", calls)
            self.assertIn("--dry-run", calls)
            self.assertIn("--write", calls)
            self.assertNotIn("rollback", calls)
            self.assertIn("ui --diagnostics", calls)

    def test_normal_path_keeps_full_synthetic_write_and_rollback(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            directory = Path(temporary_directory)
            trace = directory / "normal.trace"
            result = self.run_smoke(self.create_fake_app(directory), "normal", trace)

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("macOS app synthetic smoke passed", result.stdout)
            calls = trace.read_text(encoding="utf-8")
            self.assertIn("--write", calls)
            self.assertIn("rollback", calls)


if __name__ == "__main__":
    unittest.main()
