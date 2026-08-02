#!/usr/bin/env python3
"""Regression checks for the signed disposable-Compose E2E fixture."""

from __future__ import annotations

import importlib.machinery
import struct
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COMMON = importlib.machinery.SourceFileLoader(
    "compose_e2e", str(ROOT / "scripts" / "compose-e2e.py")
).load_module()


class ComposeE2EAuthenticationTests(unittest.TestCase):
    def test_fixture_private_key_matches_the_committed_device_certificate(self) -> None:
        signer = COMMON.FixtureSigner()
        try:
            identity = COMMON.public_identity_fixture()
            self.assertEqual(signer.public_key_b64(), identity["device_public_key_b64"])
            self.assertEqual(len(signer.sign(b"compose-e2e-auth-regression")), 64)
        finally:
            signer.close()

    def test_canonical_request_matches_the_protocol_length_prefix_contract(self) -> None:
        actual = COMMON.canonical_http_request(
            "post",
            "/v1/snapshots/begin",
            b"{}",
            "00000000-0000-0000-0000-000000000000",
            "AA==",
            1_700_000_000,
        )
        fields = (
            "mh-save-sync/http-auth/v1",
            "POST",
            "/v1/snapshots/begin",
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
            "00000000-0000-0000-0000-000000000000",
            "AA==",
            "1700000000",
        )
        expected = b"".join(
            struct.pack(">I", len(field.encode("utf-8"))) + field.encode("utf-8")
            for field in fields
        )
        self.assertEqual(actual, expected)


if __name__ == "__main__":
    unittest.main()
