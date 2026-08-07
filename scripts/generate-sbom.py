#!/usr/bin/env python3
"""Generate and verify deterministic dependency or artifact-bound release SBOMs."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tomllib
import uuid
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote


ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = "MHToolkit/mh-save-sync"
SOURCE_REF = re.compile(r"^[0-9a-f]{40}$")
REQUIRED_KINDS = (
    "rust-cli",
    "rust-server",
    "android-apk",
    "macos-app",
    "macos-cli",
    "mh3g-converter-cli",
    "mh3g-converter-macos",
)
OPTIONAL_DISTRIBUTION_KINDS = (
    "macos-save-sync-zip",
    "mh3g-converter-macos-zip",
    "mh3g-converter-windows-zip",
    "mh3g-converter-windows-portable",
    "mh3g-converter-windows-setup",
)
EXPECTED_ROLES = {
    "rust-cli": "executable",
    "rust-server": "executable",
    "android-apk": "apk",
    "macos-app": "executable",
    "macos-cli": "executable",
    "mh3g-converter-cli": "executable",
    "mh3g-converter-macos": "executable",
    "macos-save-sync-zip": "archive",
    "mh3g-converter-macos-zip": "archive",
    "mh3g-converter-windows-zip": "archive",
    "mh3g-converter-windows-portable": "executable",
    "mh3g-converter-windows-setup": "installer",
}


def _git(repo_root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(repo_root), *args],
            text=True,
            stderr=subprocess.STDOUT,
        ).strip()
    except subprocess.CalledProcessError as error:
        raise ValueError(f"git evidence unavailable: {error.output.strip()}") from error


def _git_diff_is_clean(repo_root: Path, *args: str) -> bool:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "diff", "--quiet", *args],
        check=False,
    )
    if result.returncode not in (0, 1):
        raise ValueError("git diff evidence unavailable")
    return result.returncode == 0


def _require_clean_tracked_tree(repo_root: Path) -> None:
    if not _git_diff_is_clean(repo_root, "HEAD", "--") or not _git_diff_is_clean(
        repo_root, "--cached", "--"
    ):
        raise ValueError("tracked source tree must be clean")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _commit_timestamp(repo_root: Path, source_ref: str) -> str:
    raw = _git(repo_root, "show", "-s", "--format=%cI", source_ref)
    try:
        parsed = datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError("source commit timestamp is invalid") from error
    return parsed.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _source_identity(repo_root: Path, expected_ref: str | None = None) -> dict:
    source_ref = _git(repo_root, "rev-parse", "HEAD")
    if not SOURCE_REF.fullmatch(source_ref):
        raise ValueError("checked-out HEAD is not a full commit SHA")
    if expected_ref is not None and source_ref != expected_ref:
        raise ValueError("source_ref must match checked-out HEAD")
    _require_clean_tracked_tree(repo_root)
    return {
        "source_ref": source_ref,
        "timestamp": _commit_timestamp(repo_root, source_ref),
    }


def _cargo_components(repo_root: Path) -> list[dict]:
    lock_path = repo_root / "Cargo.lock"
    try:
        lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"Cargo.lock is unreadable: {error}") from error
    components: list[dict] = []
    for package in lock.get("package", []):
        if not isinstance(package, dict):
            continue
        if package.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            continue
        name = package.get("name")
        version = package.get("version")
        checksum = package.get("checksum")
        if not all(isinstance(value, str) and value for value in (name, version)):
            raise ValueError("locked registry package identity is invalid")
        if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
            raise ValueError(f"{name} {version} lacks a proven Cargo.lock SHA-256")
        components.append(
            {
                "type": "library",
                "bom-ref": f"pkg:cargo/{name}@{version}",
                "name": name,
                "version": version,
                "purl": f"pkg:cargo/{name}@{version}",
                "scope": "required",
                "hashes": [{"alg": "SHA-256", "content": checksum}],
                "externalReferences": [
                    {
                        "type": "distribution",
                        "url": (
                            "https://crates.io/api/v1/crates/"
                            + quote(name, safe="")
                            + "/"
                            + quote(version, safe="")
                            + "/download"
                        ),
                    }
                ],
            }
        )
    return sorted(components, key=lambda item: (item["name"], item["version"]))


def _serial(source_ref: str, components: list[dict]) -> str:
    canonical = json.dumps(
        {
            "source_ref": source_ref,
            "components": [
                {
                    "bom-ref": item["bom-ref"],
                    "hash": item.get("hashes", [{}])[0].get("content"),
                }
                for item in components
            ],
        },
        sort_keys=True,
        separators=(",", ":"),
    )
    return "urn:uuid:" + str(uuid.uuid5(uuid.NAMESPACE_URL, canonical))


def _cargo_lock_component(repo_root: Path, source_ref: str) -> dict:
    lock_path = repo_root / "Cargo.lock"
    if not lock_path.is_file():
        raise ValueError("Cargo.lock is missing")
    return {
        "type": "file",
        "bom-ref": f"urn:mhtoolkit:cargo-lock:{source_ref}",
        "name": "Cargo.lock",
        "version": source_ref,
        "scope": "required",
        "hashes": [{"alg": "SHA-256", "content": _sha256(lock_path)}],
        "properties": [
            {"name": "mhtoolkit.source_ref", "value": source_ref},
            {"name": "mhtoolkit.evidence_scope", "value": "workspace-lock-aggregate"},
        ],
    }


def _metadata(source: dict, version: str, root_ref: str) -> dict:
    return {
        "timestamp": source["timestamp"],
        "tools": {
            "components": [
                {
                    "type": "application",
                    "name": "mh-save-sync scripts/generate-sbom.py",
                    "version": "2",
                }
            ]
        },
        "component": {
            "type": "application",
            "name": "mh-save-sync",
            "version": version,
            "bom-ref": root_ref,
            "externalReferences": [
                {
                    "type": "vcs",
                    "url": (
                        "https://github.com/MHToolkit/mh-save-sync.git@"
                        + source["source_ref"]
                    ),
                }
            ],
            "properties": [
                {"name": "mhtoolkit.source_ref", "value": source["source_ref"]}
            ],
        },
    }


def build_dependency_bom(repo_root: Path = ROOT) -> dict:
    source = _source_identity(repo_root)
    components = _cargo_components(repo_root)
    root_ref = f"pkg:github/MHToolkit/mh-save-sync@{source['source_ref']}"
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": _serial(source["source_ref"], components),
        "version": 1,
        "metadata": _metadata(source, source["source_ref"], root_ref),
        "components": components,
        "dependencies": [
            {"ref": root_ref, "dependsOn": [item["bom-ref"] for item in components]}
        ],
    }


def load_release_identity(identity_path: Path, repo_root: Path = ROOT) -> dict:
    try:
        raw = json.loads(identity_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"release identity is unreadable: {error}") from error
    if not isinstance(raw, dict) or raw.get("schema_version") != 1:
        raise ValueError("release identity schema_version must be 1")
    if raw.get("repository") != REPOSITORY:
        raise ValueError(f"release identity repository must be {REPOSITORY}")
    source_ref = raw.get("source_ref")
    if not isinstance(source_ref, str) or not SOURCE_REF.fullmatch(source_ref):
        raise ValueError("source_ref must be a 40-character lowercase commit SHA")
    source = _source_identity(repo_root, source_ref)

    components = raw.get("components")
    if not isinstance(components, list):
        raise ValueError("release identity components must be a list")
    if any(not isinstance(item, dict) for item in components):
        raise ValueError("release identity components must be objects")
    kinds = [item.get("kind") for item in components]
    allowed_kinds = set(REQUIRED_KINDS) | set(OPTIONAL_DISTRIBUTION_KINDS)
    unknown = sorted({kind for kind in kinds if kind not in allowed_kinds}, key=str)
    if unknown:
        raise ValueError(f"unknown component kind: {unknown[0]}")
    duplicates = sorted({kind for kind in kinds if kinds.count(kind) > 1}, key=str)
    if duplicates:
        raise ValueError(f"duplicate component kind: {duplicates[0]}")
    missing = sorted(set(REQUIRED_KINDS) - set(kinds))
    if missing:
        raise ValueError(
            "release identity requires exactly one component of every required kind: "
            + ", ".join(missing)
        )

    normalized: list[dict] = []
    ordered_kinds = list(REQUIRED_KINDS) + [
        kind for kind in OPTIONAL_DISTRIBUTION_KINDS if kind in kinds
    ]
    for kind in ordered_kinds:
        component = next(item for item in components if item.get("kind") == kind)
        package_id = component.get("package_id")
        version = component.get("version")
        if not isinstance(package_id, str) or not package_id.strip():
            raise ValueError(f"{kind}: package_id is required")
        if not isinstance(version, str) or not version.strip():
            raise ValueError(f"{kind}: version is required")
        if component.get("artifact_role") != EXPECTED_ROLES[kind]:
            raise ValueError(f"{kind}: artifact_role is invalid")
        path_value = component.get("artifact_path")
        if not isinstance(path_value, str) or not path_value:
            raise ValueError(f"{kind}: artifact_path is required")
        artifact = Path(path_value)
        if not artifact.is_absolute():
            artifact = repo_root / artifact
        if not artifact.is_file() or artifact.stat().st_size <= 0:
            raise ValueError(f"{kind}: artifact does not exist or is empty")
        normalized_component = {
            "kind": kind,
            "package_id": package_id,
            "version": version,
            "artifact_name": artifact.name,
            "artifact_role": component["artifact_role"],
            "artifact_sha256": _sha256(artifact),
        }
        if kind == "android-apk":
            version_code = component.get("version_code")
            if not isinstance(version_code, int) or version_code <= 0:
                raise ValueError("android-apk: version_code must be a positive integer")
            normalized_component["version_code"] = version_code
        if kind in {"macos-app", "mh3g-converter-macos"}:
            build_number = component.get("build_number")
            if not isinstance(build_number, int) or build_number <= 0:
                raise ValueError(f"{kind}: build_number must be a positive integer")
            normalized_component["build_number"] = build_number
        normalized.append(normalized_component)

    return {
        **source,
        "components": normalized,
        "cargo_components": _cargo_components(repo_root),
        "cargo_lock_component": _cargo_lock_component(repo_root, source_ref),
    }


def _artifact_component(component: dict, source_ref: str) -> dict:
    qualifier = "kind=" + quote(component["kind"], safe="")
    if "version_code" in component:
        qualifier += f"&version_code={component['version_code']}"
    bom_ref = (
        "pkg:generic/"
        + quote(component["package_id"], safe="")
        + "@"
        + quote(component["version"], safe="")
        + "?"
        + qualifier
    )
    properties = [
        {"name": "mhtoolkit.source_ref", "value": source_ref},
        {"name": "mhtoolkit.artifact_kind", "value": component["kind"]},
        {"name": "mhtoolkit.artifact_role", "value": component["artifact_role"]},
        {"name": "mhtoolkit.artifact_name", "value": component["artifact_name"]},
    ]
    if "version_code" in component:
        properties.append(
            {"name": "mhtoolkit.version_code", "value": str(component["version_code"])}
        )
    if "build_number" in component:
        properties.append(
            {"name": "mhtoolkit.build_number", "value": str(component["build_number"])}
        )
    return {
        "type": "application",
        "bom-ref": bom_ref,
        "name": component["package_id"],
        "version": component["version"],
        "purl": bom_ref,
        "scope": "required",
        "hashes": [
            {"alg": "SHA-256", "content": component["artifact_sha256"]}
        ],
        "externalReferences": [
            {
                "type": "vcs",
                "url": (
                    "https://github.com/MHToolkit/mh-save-sync.git@" + source_ref
                ),
            }
        ],
        "properties": properties,
    }


def build_release_bom(identity: dict) -> dict:
    artifact_components = [
        _artifact_component(component, identity["source_ref"])
        for component in identity["components"]
    ]
    cargo_components = identity["cargo_components"]
    cargo_lock_component = identity["cargo_lock_component"]
    components = [cargo_lock_component] + cargo_components + artifact_components
    android = next(
        component
        for component in identity["components"]
        if component["kind"] == "android-apk"
    )
    root_ref = f"pkg:github/MHToolkit/mh-save-sync@{identity['source_ref']}"
    cargo_refs = [component["bom-ref"] for component in cargo_components]
    dependencies = [
        {
            "ref": root_ref,
            "dependsOn": [
                cargo_lock_component["bom-ref"],
                *[component["bom-ref"] for component in artifact_components],
            ],
        }
    ]
    dependencies.append(
        {"ref": cargo_lock_component["bom-ref"], "dependsOn": cargo_refs}
    )
    dependencies.extend(
        {"ref": component["bom-ref"], "dependsOn": []}
        for component in artifact_components
    )
    dependencies.extend(
        {"ref": component["bom-ref"], "dependsOn": []}
        for component in cargo_components
    )
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": _serial(identity["source_ref"], components),
        "version": 1,
        "metadata": _metadata(identity, android["version"], root_ref),
        "components": components,
        "dependencies": dependencies,
    }


def write_json(document: dict, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _sbom_identity(sbom: Path, identity: dict) -> dict:
    android = next(
        component
        for component in identity["components"]
        if component["kind"] == "android-apk"
    )
    return {
        "format": "cyclonedx-json",
        "sha256": "sha256:" + _sha256(sbom),
        "artifact_sha256": "sha256:" + android["artifact_sha256"],
        "source_ref": identity["source_ref"],
    }


def write_sbom_identity(sbom: Path, identity: dict, receipt: Path) -> None:
    write_json(_sbom_identity(sbom, identity), receipt)


def _load_cyclonedx(path: Path) -> dict:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"SBOM is unreadable: {error}") from error
    if not isinstance(document, dict) or (
        document.get("bomFormat") != "CycloneDX"
        or document.get("specVersion") != "1.5"
    ):
        raise ValueError("unsupported SBOM format")
    return document


def verify_dependency_sbom(sbom: Path, repo_root: Path = ROOT) -> None:
    if _load_cyclonedx(sbom) != build_dependency_bom(repo_root):
        raise ValueError("dependency SBOM does not match clean source ref or Cargo.lock")


def verify_release_sbom(
    sbom: Path,
    receipt: Path,
    identity_path: Path,
    repo_root: Path = ROOT,
) -> None:
    actual = _load_cyclonedx(sbom)
    identity = load_release_identity(identity_path, repo_root)
    if actual != build_release_bom(identity):
        raise ValueError("SBOM does not match source ref, artifact SHA, or package identity")
    try:
        actual_receipt = json.loads(receipt.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"SBOM identity receipt is unreadable: {error}") from error
    if actual_receipt != _sbom_identity(sbom, identity):
        raise ValueError("SBOM identity receipt does not match source ref or artifact")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    dependencies = subparsers.add_parser("dependencies")
    dependencies.add_argument("output", type=Path)
    dependencies.add_argument("--repo-root", type=Path, default=ROOT)

    verify_dependencies = subparsers.add_parser("verify-dependencies")
    verify_dependencies.add_argument("--sbom", required=True, type=Path)
    verify_dependencies.add_argument("--repo-root", type=Path, default=ROOT)

    release = subparsers.add_parser("release")
    release.add_argument("--identity", required=True, type=Path)
    release.add_argument("--output", required=True, type=Path)
    release.add_argument("--receipt", required=True, type=Path)
    release.add_argument("--repo-root", type=Path, default=ROOT)

    verify_release = subparsers.add_parser("verify-release")
    verify_release.add_argument("--identity", required=True, type=Path)
    verify_release.add_argument("--sbom", required=True, type=Path)
    verify_release.add_argument("--receipt", required=True, type=Path)
    verify_release.add_argument("--repo-root", type=Path, default=ROOT)
    return parser


def _normalize_cli_argv(argv: list[str]) -> list[str]:
    commands = {"dependencies", "verify-dependencies", "release", "verify-release"}
    if len(argv) == 1 and argv[0] not in commands and not argv[0].startswith("-"):
        return ["dependencies", argv[0]]
    return argv


def main(argv: list[str] | None = None) -> int:
    cli_argv = list(sys.argv[1:] if argv is None else argv)
    arguments = _parser().parse_args(_normalize_cli_argv(cli_argv))
    try:
        if arguments.command == "dependencies":
            write_json(build_dependency_bom(arguments.repo_root), arguments.output)
            print(arguments.output)
        elif arguments.command == "verify-dependencies":
            verify_dependency_sbom(arguments.sbom, arguments.repo_root)
            print("dependency SBOM gate passed")
        elif arguments.command == "release":
            identity = load_release_identity(arguments.identity, arguments.repo_root)
            write_json(build_release_bom(identity), arguments.output)
            write_sbom_identity(arguments.output, identity, arguments.receipt)
            print(arguments.output)
            print(arguments.receipt)
        else:
            verify_release_sbom(
                arguments.sbom,
                arguments.receipt,
                arguments.identity,
                arguments.repo_root,
            )
            print("release SBOM identity gate passed")
    except ValueError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
