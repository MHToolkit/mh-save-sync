#!/usr/bin/env python3
"""Regression gate for Android key-rotation permission ownership.

AndroidX emits a package-owned signature permission.  A v3 proof of rotation
must carry the predecessor's permission capability, otherwise Android rejects
the in-place update before app data can be preserved.
"""

from pathlib import Path
import unittest


REPO = Path(__file__).resolve().parents[1]
PACKAGER = REPO / "scripts" / "android-package-signer-migration.sh"
RUNBOOK = REPO / "docs" / "runbooks" / "ANDROID_SIGNER_MIGRATION.md"


class AndroidSignerMigrationContractTest(unittest.TestCase):
    def test_packager_rejects_lineage_without_permission_capability(self) -> None:
        script = PACKAGER.read_text(encoding="utf-8")
        self.assertIn("old_permission_capability", script)
        self.assertIn(
            '|| blocked "old signer lacks signature-permission migration capability"',
            script,
        )

    def test_regression_is_documented_as_duplicate_permission_not_reinstall(self) -> None:
        runbook = RUNBOOK.read_text(encoding="utf-8")
        self.assertIn("INSTALL_FAILED_DUPLICATE_PERMISSION", runbook)
        self.assertIn("permission-capable lineage, not an uninstall", runbook)


if __name__ == "__main__":
    unittest.main()
