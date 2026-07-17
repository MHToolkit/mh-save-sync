#!/usr/bin/env python3
"""Generate a minimal CycloneDX SBOM from repository lockfiles.

The output is intentionally generated locally instead of relying on a hosted
scanner so CI can prove that the published artifact has a reproducible bill of
materials without sending source or build products to a third-party service.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import pathlib
import subprocess
import sys
import tomllib
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]


def git_output(*args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", *args], cwd=ROOT, text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        return "unknown"


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def cargo_components() -> list[dict[str, Any]]:
    lock_path = ROOT / "Cargo.lock"
    data = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    components: list[dict[str, Any]] = []
    for package in sorted(
        data.get("package", []),
        key=lambda p: (p.get("name", ""), p.get("version", ""), p.get("source", "")),
    ):
        name = package["name"]
        version = package["version"]
        source = package.get("source", "")
        component: dict[str, Any] = {
            "type": "library",
            "bom-ref": f"pkg:cargo/{name}@{version}",
            "name": name,
            "version": version,
            "purl": f"pkg:cargo/{name}@{version}",
            "scope": "required",
        }
        checksum = package.get("checksum")
        if checksum:
            component["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        if source:
            component["externalReferences"] = [
                {
                    "type": "distribution",
                    "url": source,
                }
            ]
        components.append(component)
    return components


def android_components() -> list[dict[str, Any]]:
    wrapper = ROOT / "apps/android/gradle/wrapper/gradle-wrapper.properties"
    app_build = ROOT / "apps/android/app/build.gradle.kts"
    components: list[dict[str, Any]] = []
    if wrapper.exists():
        text = wrapper.read_text(encoding="utf-8")
        distribution = next(
            (
                line.split("=", 1)[1].strip().replace("\\:", ":")
                for line in text.splitlines()
                if line.startswith("distributionUrl=")
            ),
            "unknown",
        )
        components.append(
            {
                "type": "framework",
                "bom-ref": "gradle-wrapper",
                "name": "Gradle Wrapper",
                "version": distribution.rsplit("-", 2)[-2]
                if distribution.count("-") >= 2
                else "unknown",
                "hashes": [{"alg": "SHA-256", "content": sha256_file(wrapper)}],
                "externalReferences": [{"type": "distribution", "url": distribution}],
            }
        )
    if app_build.exists():
        components.append(
            {
                "type": "application",
                "bom-ref": "mh-save-sync-android-shell",
                "name": "mh-save-sync-android-shell",
                "version": "0.1.0",
                "hashes": [{"alg": "SHA-256", "content": sha256_file(app_build)}],
            }
        )
    return components


def build_bom() -> dict[str, Any]:
    revision = git_output("rev-parse", "HEAD")
    now = dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    components = cargo_components() + android_components()
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{hashlib.sha256(revision.encode()).hexdigest()[:32]}",
        "version": 1,
        "metadata": {
            "timestamp": now,
            "tools": {
                "components": [
                    {
                        "type": "application",
                        "name": "mh-save-sync scripts/generate-sbom.py",
                        "version": "1",
                    }
                ]
            },
            "component": {
                "type": "application",
                "name": "mh-save-sync",
                "version": "0.1.0-alpha.1",
                "bom-ref": "pkg:github/MHToolkit/mh-save-sync",
                "externalReferences": [
                    {
                        "type": "vcs",
                        "url": "https://github.com/MHToolkit/mh-save-sync",
                    }
                ],
                "properties": [
                    {"name": "git.commit", "value": revision},
                    {"name": "git.branch", "value": git_output("branch", "--show-current")},
                ],
            },
        },
        "components": components,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "output",
        nargs="?",
        default="artifacts/sbom/mh-save-sync.cdx.json",
        help="output CycloneDX JSON path",
    )
    args = parser.parse_args()
    out_path = (ROOT / args.output).resolve() if not pathlib.Path(args.output).is_absolute() else pathlib.Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    bom = build_bom()
    out_path.write_text(json.dumps(bom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {out_path} components={len(bom['components'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
