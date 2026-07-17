#!/usr/bin/env python3
"""Audit local emulator/app availability for adapter runtime verification.

This is an evidence-preflight tool, not a compatibility upgrader.  It reports
whether the current macOS host and attached Android devices contain the package,
bundle, process, or root hints required to begin a real RuntimeVerified
save->mutate->restore->emulator-readable loop.

Privacy boundary: the output records identifiers and booleans only.  It does
not enumerate user save files, save bytes, character names, ROM paths, keys, or
plaintext save-tree contents.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import plistlib
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "artifacts/runtime/runtime_evidence_audit.json"


def run(cmd: list[str], timeout: float = 20.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def adapter_descriptors() -> list[dict[str, Any]]:
    proc = run(["cargo", "run", "-q", "-p", "save-cli", "--bin", "mh-save", "--", "adapters"], timeout=60.0)
    if proc.returncode != 0:
        raise SystemExit(
            f"failed to load adapter descriptors via save-cli: exit={proc.returncode} stderr={proc.stderr.strip()}"
        )
    return json.loads(proc.stdout)


def adb_devices(adb: str) -> list[dict[str, Any]]:
    proc = run([adb, "devices", "-l"], timeout=10.0)
    if proc.returncode != 0:
        return [{"adb_error": proc.stderr.strip() or proc.stdout.strip()}]
    devices: list[dict[str, Any]] = []
    for line in proc.stdout.splitlines()[1:]:
        line = line.strip()
        if not line:
            continue
        parts = line.split()
        serial = parts[0]
        state = parts[1] if len(parts) > 1 else "unknown"
        info = " ".join(parts[2:])
        packages: list[str] = []
        if state == "device":
            pkg_proc = run([adb, "-s", serial, "shell", "pm", "list", "packages"], timeout=20.0)
            if pkg_proc.returncode == 0:
                packages = sorted(
                    row.removeprefix("package:").strip()
                    for row in pkg_proc.stdout.splitlines()
                    if row.startswith("package:")
                )
        devices.append({"serial": serial, "state": state, "info": info, "packages": packages})
    return devices


def macos_bundle_ids(search_roots: list[Path]) -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for root in search_roots:
        if not root.exists():
            continue
        for app in root.glob("*.app"):
            info_plist = app / "Contents/Info.plist"
            if not info_plist.exists():
                continue
            try:
                with info_plist.open("rb") as fh:
                    info = plistlib.load(fh)
            except Exception:
                continue
            bundle_id = info.get("CFBundleIdentifier")
            if isinstance(bundle_id, str):
                # Store the app bundle name only, not full user path.
                found.setdefault(bundle_id, []).append(app.name)
    return {key: sorted(value) for key, value in sorted(found.items())}


def running_process_names(names: list[str]) -> dict[str, bool]:
    result: dict[str, bool] = {}
    for name in names:
        proc = run(["pgrep", "-x", name], timeout=5.0)
        result[name] = proc.returncode == 0
    return result


def root_hint_exists(hint: str | None) -> bool | None:
    if not hint or not hint.startswith("~"):
        return None
    expanded = Path(os.path.expanduser(hint))
    return expanded.exists()


def summarize_adapter(
    descriptor: dict[str, Any],
    android_devices: list[dict[str, Any]],
    mac_bundles: dict[str, list[str]],
) -> dict[str, Any]:
    platform = descriptor["platform"]
    package_ids = descriptor.get("package_ids") or []
    bundle_ids = descriptor.get("bundle_ids") or []
    process_names = descriptor.get("process_names") or []

    package_matches = []
    if platform == "android":
        for device in android_devices:
            packages = set(device.get("packages") or [])
            package_matches.append(
                {
                    "serial": device.get("serial"),
                    "state": device.get("state"),
                    "matched_package_ids": [pkg for pkg in package_ids if pkg in packages],
                    "required_package_ids": package_ids,
                }
            )

    bundle_matches = []
    if platform == "macos":
        for bundle_id in bundle_ids:
            bundle_matches.append(
                {
                    "bundle_id": bundle_id,
                    "found_app_names": mac_bundles.get(bundle_id, []),
                }
            )

    process_matches = running_process_names(process_names) if platform == "macos" and process_names else {}

    can_begin_runtime_verification = False
    blockers: list[str] = []
    if platform == "android":
        if package_ids and any(match["matched_package_ids"] for match in package_matches):
            can_begin_runtime_verification = True
        elif package_ids:
            blockers.append("required Android package not installed on attached ADB devices")
        else:
            blockers.append("generic Android folder requires user-selected SAF tree and fixture flow, not package detection")
    elif platform == "macos":
        if bundle_ids and any(match["found_app_names"] for match in bundle_matches):
            can_begin_runtime_verification = True
        elif bundle_ids:
            blockers.append("required macOS app bundle not found in audited app roots")
        else:
            blockers.append("generic macOS folder requires user-selected folder and fixture flow, not bundle detection")
    else:
        blockers.append("contract-only generic descriptor has no runtime app identity")

    if descriptor.get("support_level") == "runtime-verified":
        blockers.append("descriptor already claims runtime-verified; audit still requires evidence bundle review")

    return {
        "emulator_id": descriptor["emulator_id"],
        "platform": platform,
        "support_level_declared": descriptor.get("support_level"),
        "root_acquisition": descriptor.get("root_acquisition"),
        "package_audit": package_matches,
        "bundle_audit": bundle_matches,
        "process_audit": process_matches,
        "user_root_hint_exists": root_hint_exists(descriptor.get("user_root_hint")),
        "can_begin_runtime_verification": can_begin_runtime_verification,
        "runtime_verification_blockers": blockers,
        "declared_evidence_fingerprint": descriptor.get("evidence_fingerprint"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default=os.environ.get("ADB", str(Path.home() / "Library/Android/sdk/platform-tools/adb")))
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument(
        "--mac-app-root",
        action="append",
        type=Path,
        default=[Path("/Applications"), Path.home() / "Applications"],
        help="macOS app directory to scan for .app bundle IDs; may be repeated",
    )
    args = parser.parse_args()

    descriptors = adapter_descriptors()
    android = adb_devices(args.adb) if Path(args.adb).exists() else [{"adb_error": f"adb not found: {args.adb}"}]
    mac_bundles = macos_bundle_ids(args.mac_app_root)
    adapter_results = [summarize_adapter(d, android, mac_bundles) for d in descriptors]

    report = {
        "runtime_evidence_audit": True,
        "generated_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "privacy_boundary": "identifiers and booleans only; no save tree enumeration",
        "adb_available": Path(args.adb).exists(),
        "android_devices": [
            {
                "serial": d.get("serial"),
                "state": d.get("state"),
                "info": d.get("info"),
                "package_count": len(d.get("packages") or []),
                "adb_error": d.get("adb_error"),
            }
            for d in android
        ],
        "mac_app_roots_scanned": [str(p) for p in args.mac_app_root],
        "adapter_count": len(adapter_results),
        "adapters": adapter_results,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
