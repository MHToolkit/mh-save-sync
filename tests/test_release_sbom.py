import importlib.util
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
import uuid
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "generate_sbom", ROOT / "scripts" / "generate-sbom.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class ReleaseSbomTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", self.root], check=True)
        subprocess.run(
            ["git", "-C", self.root, "config", "user.email", "test@example.invalid"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", self.root, "config", "user.name", "SBOM Test"],
            check=True,
        )
        (self.root / "Cargo.lock").write_text(
            """version = 4

[[package]]
name = "bytes"
version = "1.12.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "8ae3f5d315924270530207e2a68396c3cc547f6dca3fbdca317cfb1a51edb593"
""",
            encoding="utf-8",
        )
        (self.root / ".gitignore").write_text("build/\n", encoding="utf-8")
        artifact_names = {
            "mh-save": b"rust-cli\n",
            "mh-save-server": b"rust-server\n",
            "app-debug.apk": b"android-apk\n",
            "MHSaveSyncMac": b"macos-app\n",
            "mh-save-macos": b"macos-cli\n",
            "mh3g-save-convert": b"converter-cli\n",
            "MH3GSaveConverterMac": b"converter-macos\n",
        }
        for name, contents in artifact_names.items():
            (self.root / name).write_bytes(contents)
        subprocess.run(
            ["git", "-C", self.root, "add", "Cargo.lock", ".gitignore"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", self.root, "commit", "-qm", "fixture"],
            check=True,
            env={
                **os.environ,
                "GIT_AUTHOR_DATE": "2026-08-07T00:00:00Z",
                "GIT_COMMITTER_DATE": "2026-08-07T00:00:00Z",
            },
        )
        self.source_ref = subprocess.check_output(
            ["git", "-C", self.root, "rev-parse", "HEAD"], text=True
        ).strip()
        self.identity_path = self.root / "release-identity.json"
        components = [
            ("rust-cli", "mh-save", "0.1.0", "mh-save", "executable", None),
            (
                "rust-server",
                "mh-save-server",
                "0.1.0",
                "mh-save-server",
                "executable",
                None,
            ),
            (
                "android-apk",
                "org.mhtoolkit.savesync",
                "0.1.0-alpha.4",
                "app-debug.apk",
                "apk",
                5,
            ),
            (
                "macos-app",
                "org.mhtoolkit.mh-save-sync.alpha",
                "0.1.0-alpha.4",
                "MHSaveSyncMac",
                "executable",
                5,
            ),
            (
                "macos-cli",
                "org.mhtoolkit.mh-save-sync.cli",
                "0.1.0",
                "mh-save-macos",
                "executable",
                None,
            ),
            (
                "mh3g-converter-cli",
                "mh3g-save-convert",
                "0.0.13",
                "mh3g-save-convert",
                "executable",
                None,
            ),
            (
                "mh3g-converter-macos",
                "org.mhtoolkit.mh3g-save-converter",
                "0.0.13",
                "MH3GSaveConverterMac",
                "executable",
                1,
            ),
        ]
        self.identity_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "repository": "MHToolkit/mh-save-sync",
                    "source_ref": self.source_ref,
                    "components": [
                        {
                            "kind": kind,
                            "package_id": package_id,
                            "version": version,
                            "artifact_path": artifact,
                            "artifact_role": role,
                            **(
                                {"version_code": version_code}
                                if kind == "android-apk"
                                else {"build_number": version_code}
                                if version_code
                                else {}
                            ),
                        }
                        for kind, package_id, version, artifact, role, version_code in components
                    ],
                },
                indent=2,
            ),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def generate(self, sbom: Path, receipt: Path) -> None:
        identity = MODULE.load_release_identity(self.identity_path, self.root)
        MODULE.write_json(MODULE.build_release_bom(identity), sbom)
        MODULE.write_sbom_identity(sbom, identity, receipt)

    def test_dependency_bom_is_deterministic_and_not_fixed_to_old_version(self) -> None:
        first = MODULE.build_dependency_bom(self.root)
        second = MODULE.build_dependency_bom(self.root)

        self.assertEqual(first, second)
        self.assertEqual(
            first["metadata"]["component"]["version"], self.source_ref
        )
        self.assertNotEqual(first["metadata"]["component"]["version"], "0.1.0-alpha.1")

    def test_release_bom_is_deterministic_artifact_bound_and_verifies(self) -> None:
        first = self.root / "first.cdx.json"
        second = self.root / "second.cdx.json"
        first_receipt = self.root / "first.sbom-identity.json"
        second_receipt = self.root / "second.sbom-identity.json"

        self.generate(first, first_receipt)
        self.generate(second, second_receipt)

        self.assertEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(first_receipt.read_bytes(), second_receipt.read_bytes())
        MODULE.verify_release_sbom(
            first, first_receipt, self.identity_path, self.root
        )
        document = json.loads(first.read_text(encoding="utf-8"))
        self.assertEqual(document["metadata"]["component"]["version"], "0.1.0-alpha.4")
        serial = uuid.UUID(document["serialNumber"].removeprefix("urn:uuid:"))
        self.assertEqual(serial.version, 5)
        self.assertEqual(
            {component["name"] for component in document["components"]},
            {
                "bytes",
                "Cargo.lock",
                "mh-save",
                "mh-save-server",
                "org.mhtoolkit.savesync",
                "org.mhtoolkit.mh-save-sync.alpha",
                "org.mhtoolkit.mh-save-sync.cli",
                "mh3g-save-convert",
                "org.mhtoolkit.mh3g-save-converter",
            },
        )
        for component in document["components"]:
            if component["name"] not in {"bytes", "Cargo.lock"}:
                properties = {item["name"]: item["value"] for item in component["properties"]}
                self.assertEqual(properties["mhtoolkit.source_ref"], self.source_ref)
                self.assertEqual(len(component["hashes"][0]["content"]), 64)
                if component["name"] == "org.mhtoolkit.mh-save-sync.alpha":
                    self.assertEqual(properties["mhtoolkit.build_number"], "5")
                if component["name"] == "org.mhtoolkit.mh3g-save-converter":
                    self.assertEqual(properties["mhtoolkit.build_number"], "1")
        dependency_ref = "pkg:cargo/bytes@1.12.0"
        cargo_lock_ref = f"urn:mhtoolkit:cargo-lock:{self.source_ref}"
        relationships = {
            entry["ref"]: set(entry["dependsOn"])
            for entry in document["dependencies"]
        }
        self.assertEqual(relationships[cargo_lock_ref], {dependency_ref})
        for artifact_ref in (
            "pkg:generic/mh-save@0.1.0?kind=rust-cli",
            "pkg:generic/mh-save-server@0.1.0?kind=rust-server",
            "pkg:generic/mh3g-save-convert@0.0.13?kind=mh3g-converter-cli",
        ):
            self.assertEqual(relationships[artifact_ref], set())
        root_ref = f"pkg:github/MHToolkit/mh-save-sync@{self.source_ref}"
        self.assertIn(cargo_lock_ref, relationships[root_ref])

        receipt = json.loads(first_receipt.read_text(encoding="utf-8"))
        self.assertEqual(receipt["format"], "cyclonedx-json")
        self.assertEqual(receipt["source_ref"], self.source_ref)
        self.assertEqual(
            receipt["artifact_sha256"],
            "sha256:" + hashlib.sha256(b"android-apk\n").hexdigest(),
        )

    def test_missing_artifact_fails_closed(self) -> None:
        (self.root / "app-debug.apk").unlink()

        with self.assertRaisesRegex(ValueError, "artifact does not exist"):
            MODULE.load_release_identity(self.identity_path, self.root)

    def test_wrong_source_ref_fails_closed(self) -> None:
        identity = json.loads(self.identity_path.read_text(encoding="utf-8"))
        identity["source_ref"] = "f" * 40
        self.identity_path.write_text(json.dumps(identity), encoding="utf-8")

        with self.assertRaisesRegex(ValueError, "source_ref must match checked-out HEAD"):
            MODULE.load_release_identity(self.identity_path, self.root)

    def test_tracked_dirty_source_fails_closed(self) -> None:
        with (self.root / "Cargo.lock").open("a", encoding="utf-8") as stream:
            stream.write("# drift\n")

        with self.assertRaisesRegex(ValueError, "tracked source tree must be clean"):
            MODULE.load_release_identity(self.identity_path, self.root)

    def test_tracked_staged_source_fails_closed(self) -> None:
        with (self.root / "Cargo.lock").open("a", encoding="utf-8") as stream:
            stream.write("# staged drift\n")
        subprocess.run(["git", "-C", self.root, "add", "Cargo.lock"], check=True)

        with self.assertRaisesRegex(ValueError, "tracked source tree must be clean"):
            MODULE.load_release_identity(self.identity_path, self.root)

    def test_untracked_and_ignored_build_outputs_are_allowed(self) -> None:
        (self.root / "loose-build-output.bin").write_bytes(b"untracked\n")
        (self.root / "build").mkdir()
        (self.root / "build" / "ignored.bin").write_bytes(b"ignored\n")

        identity = MODULE.load_release_identity(self.identity_path, self.root)

        self.assertEqual(identity["source_ref"], self.source_ref)

    def test_missing_and_duplicate_required_kind_fail_closed(self) -> None:
        identity = json.loads(self.identity_path.read_text(encoding="utf-8"))
        identity["components"][-1] = dict(identity["components"][0])
        self.identity_path.write_text(json.dumps(identity), encoding="utf-8")

        with self.assertRaisesRegex(ValueError, "duplicate component kind"):
            MODULE.load_release_identity(self.identity_path, self.root)

        identity["components"] = identity["components"][:-1]
        self.identity_path.write_text(json.dumps(identity), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "exactly one component"):
            MODULE.load_release_identity(self.identity_path, self.root)

    def test_optional_distribution_kinds_are_bound_when_present(self) -> None:
        optional = {
            "macos-save-sync-zip": ("MHSaveSync.zip", "archive"),
            "mh3g-converter-macos-zip": ("MH3GConverter.zip", "archive"),
            "mh3g-converter-windows-zip": ("MH3GConverter-Windows.zip", "archive"),
            "mh3g-converter-windows-portable": ("MH3GConverter.exe", "executable"),
            "mh3g-converter-windows-setup": ("MH3GConverter-Setup.exe", "installer"),
        }
        identity = json.loads(self.identity_path.read_text(encoding="utf-8"))
        for kind, (name, role) in optional.items():
            (self.root / name).write_bytes((kind + "\n").encode())
            identity["components"].append(
                {
                    "kind": kind,
                    "package_id": f"org.mhtoolkit.{kind}",
                    "version": "0.1.0-alpha.4",
                    "artifact_path": name,
                    "artifact_role": role,
                }
            )
        self.identity_path.write_text(json.dumps(identity), encoding="utf-8")

        normalized = MODULE.load_release_identity(self.identity_path, self.root)
        document = MODULE.build_release_bom(normalized)

        self.assertTrue(optional.keys() <= {item["kind"] for item in normalized["components"]})
        component_kinds = {
            prop["value"]
            for component in document["components"]
            for prop in component.get("properties", [])
            if prop["name"] == "mhtoolkit.artifact_kind"
        }
        self.assertTrue(optional.keys() <= component_kinds)

    def test_duplicate_optional_or_unknown_kind_fails_closed(self) -> None:
        identity = json.loads(self.identity_path.read_text(encoding="utf-8"))
        optional = {
            "kind": "macos-save-sync-zip",
            "package_id": "org.mhtoolkit.save-sync.zip",
            "version": "0.1.0-alpha.4",
            "artifact_path": "mh-save",
            "artifact_role": "archive",
        }
        identity["components"].extend([optional, dict(optional)])
        self.identity_path.write_text(json.dumps(identity), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate component kind"):
            MODULE.load_release_identity(self.identity_path, self.root)

        identity["components"] = identity["components"][:-2]
        identity["components"].append({**optional, "kind": "future-distribution"})
        self.identity_path.write_text(json.dumps(identity), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "unknown component kind"):
            MODULE.load_release_identity(self.identity_path, self.root)

    def test_legacy_dependency_cli_is_preserved(self) -> None:
        self.assertEqual(
            MODULE._normalize_cli_argv(["artifacts/sbom/release.cdx.json"]),
            ["dependencies", "artifacts/sbom/release.cdx.json"],
        )
        output = self.root / "legacy.cdx.json"
        original_root = MODULE.ROOT
        MODULE.ROOT = self.root
        try:
            self.assertEqual(MODULE.main([str(output)]), 0)
            MODULE.verify_dependency_sbom(output, self.root)
        finally:
            MODULE.ROOT = original_root

    def test_unknown_format_fails_closed(self) -> None:
        sbom = self.root / "release.cdx.json"
        receipt = self.root / "release.sbom-identity.json"
        self.generate(sbom, receipt)
        document = json.loads(sbom.read_text(encoding="utf-8"))
        document["specVersion"] = "9.9"
        sbom.write_text(json.dumps(document), encoding="utf-8")

        with self.assertRaisesRegex(ValueError, "unsupported SBOM format"):
            MODULE.verify_release_sbom(sbom, receipt, self.identity_path, self.root)

    def test_artifact_tampering_fails_closed(self) -> None:
        sbom = self.root / "release.cdx.json"
        receipt = self.root / "release.sbom-identity.json"
        self.generate(sbom, receipt)
        (self.root / "app-debug.apk").write_bytes(b"tampered\n")

        with self.assertRaisesRegex(ValueError, "SBOM does not match"):
            MODULE.verify_release_sbom(sbom, receipt, self.identity_path, self.root)

    def test_sbom_tampering_fails_closed(self) -> None:
        sbom = self.root / "release.cdx.json"
        receipt = self.root / "release.sbom-identity.json"
        self.generate(sbom, receipt)
        document = json.loads(sbom.read_text(encoding="utf-8"))
        document["components"][1]["hashes"][0]["content"] = "0" * 64
        sbom.write_text(json.dumps(document), encoding="utf-8")

        with self.assertRaisesRegex(ValueError, "SBOM does not match"):
            MODULE.verify_release_sbom(sbom, receipt, self.identity_path, self.root)

    def test_receipt_tampering_fails_closed(self) -> None:
        sbom = self.root / "release.cdx.json"
        receipt = self.root / "release.sbom-identity.json"
        self.generate(sbom, receipt)
        identity = json.loads(receipt.read_text(encoding="utf-8"))
        identity["source_ref"] = "f" * 40
        receipt.write_text(json.dumps(identity), encoding="utf-8")

        with self.assertRaisesRegex(ValueError, "SBOM identity receipt does not match"):
            MODULE.verify_release_sbom(sbom, receipt, self.identity_path, self.root)


if __name__ == "__main__":
    unittest.main()
