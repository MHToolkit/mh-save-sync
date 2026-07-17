#!/usr/bin/env python3
"""Deterministic black-box exercise for the PostgreSQL + S3 service backend."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


def object_id(label: str) -> str:
    return hashlib.sha256(label.encode()).hexdigest()


def encoded(payload: bytes) -> tuple[str, str]:
    return (
        hashlib.sha256(payload).hexdigest(),
        base64.b64encode(payload).decode(),
    )


def public_identity_fixture() -> dict[str, str]:
    repo = Path(__file__).resolve().parent.parent
    return json.loads(
        (repo / "tests/fixtures/device-identity-public.json").read_text()
    )


class Api:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")

    def request(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
        expected: tuple[int, ...] = (200,),
    ) -> Any:
        data = None
        headers: dict[str, str] = {}
        if payload is not None:
            data = json.dumps(payload, separators=(",", ":")).encode()
            headers["content-type"] = "application/json"
        request = urllib.request.Request(
            self.base_url + path,
            data=data,
            headers=headers,
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                body = response.read()
                status = response.status
        except urllib.error.HTTPError as error:
            body = error.read()
            status = error.code
        if status not in expected:
            raise RuntimeError(
                f"{method} {path}: expected {expected}, got {status}: "
                f"{body.decode(errors='replace')}"
            )
        if not body:
            return None
        try:
            return json.loads(body)
        except json.JSONDecodeError:
            return body.decode(errors="replace")


def put_object(
    api: Api,
    upload_id: str,
    kind: str,
    identifier: str,
    payload: bytes,
) -> None:
    digest, body = encoded(payload)
    suffix = "chunks" if kind == "chunk" else "manifest"
    key = "chunk_id" if kind == "chunk" else "manifest_id"
    api.request(
        "POST",
        f"/v1/snapshots/{upload_id}/{suffix}",
        {key: identifier, "sha256": digest, "bytes_b64": body},
        (204,),
    )


def begin(
    api: Api,
    account: str,
    device: str,
    logical_save: str,
    manifest: str,
    chunks: list[str],
    base: str | None,
    parents: list[str],
) -> dict[str, Any]:
    return api.request(
        "POST",
        "/v1/snapshots/begin",
        {
            "account_handle": account,
            "device_cert_id": device,
            "logical_save_id": logical_save,
            "base_head": base,
            "parents": parents,
            "encrypted_manifest_id": manifest,
            "chunk_ids": chunks,
        },
    )


def commit(api: Api, upload: str, snapshot: str) -> dict[str, Any]:
    return api.request(
        "POST",
        f"/v1/snapshots/{upload}/commit",
        {"snapshot_id": snapshot},
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:18080")
    args = parser.parse_args()
    api = Api(args.base_url)

    ready = api.request("GET", "/ready")
    if ready["backend"] != "postgres-s3":
        raise RuntimeError(f"persistent backend required, got {ready}")

    identity = public_identity_fixture()
    account = identity["account_handle"]
    device = identity["cert_id"]
    api.request(
        "POST",
        "/v1/accounts/bootstrap",
        {
            "account_handle": account,
            "root_public_key_b64": identity["root_public_key_b64"],
        },
        (201,),
    )
    api.request(
        "POST",
        "/v1/accounts/bootstrap",
        {
            "account_handle": account,
            "root_public_key_b64": base64.b64encode(bytes(32)).decode(),
        },
        (409,),
    )
    api.request(
        "POST",
        "/v1/devices/register",
        {
            "account_handle": account,
            "cert_id": device,
            "device_public_key_b64": identity["device_public_key_b64"],
            "certificate_b64": base64.b64encode(b"invalid-certificate").decode(),
        },
        (400,),
    )
    api.request(
        "POST",
        "/v1/devices/register",
        {
            "account_handle": account,
            "cert_id": device,
            "device_public_key_b64": identity["device_public_key_b64"],
            "certificate_b64": identity["certificate_b64"],
        },
        (201,),
    )

    run = str(time.time_ns())
    logical_save = f"e2e-{run}"
    chunk1, chunk2, chunk3 = (
        object_id(f"{run}:chunk:{index}") for index in range(1, 4)
    )
    manifest1, manifest2, manifest3 = (
        object_id(f"{run}:manifest:{index}") for index in range(1, 4)
    )
    snapshot1, snapshot2, snapshot3 = (
        object_id(f"{run}:snapshot:{index}") for index in range(1, 4)
    )

    first = begin(
        api, account, device, logical_save, manifest1, [chunk1], None, []
    )
    if first["missing_chunk_ids"] != [chunk1]:
        raise RuntimeError(f"unexpected first missing set: {first}")
    put_object(api, first["upload_id"], "chunk", chunk1, b"encrypted-chunk-one")
    put_object(
        api, first["upload_id"], "manifest", manifest1, b"encrypted-manifest-one"
    )
    first_commit = commit(api, first["upload_id"], snapshot1)
    if first_commit["outcome"] != "first-snapshot":
        raise RuntimeError(f"unexpected first commit: {first_commit}")

    branch_a = begin(
        api,
        account,
        device,
        logical_save,
        manifest2,
        [chunk1, chunk2],
        snapshot1,
        [snapshot1],
    )
    if branch_a["missing_chunk_ids"] != [chunk2]:
        raise RuntimeError(f"dedupe/missing-set failed: {branch_a}")
    branch_b = begin(
        api,
        account,
        device,
        logical_save,
        manifest3,
        [chunk3],
        snapshot1,
        [snapshot1],
    )
    put_object(api, branch_a["upload_id"], "chunk", chunk2, b"encrypted-chunk-two")
    put_object(
        api,
        branch_a["upload_id"],
        "manifest",
        manifest2,
        b"encrypted-manifest-two",
    )
    fast_forward = commit(api, branch_a["upload_id"], snapshot2)
    if fast_forward["outcome"] != "fast-forward":
        raise RuntimeError(f"expected fast-forward: {fast_forward}")

    put_object(
        api, branch_b["upload_id"], "chunk", chunk3, b"encrypted-chunk-three"
    )
    put_object(
        api,
        branch_b["upload_id"],
        "manifest",
        manifest3,
        b"encrypted-manifest-three",
    )
    conflict = commit(api, branch_b["upload_id"], snapshot3)
    if (
        conflict["outcome"] != "conflict"
        or conflict["head"] != snapshot2
        or conflict["conflict_snapshot"] != snapshot3
    ):
        raise RuntimeError(f"conflict CAS failed: {conflict}")

    head = api.request("GET", f"/v1/heads/{logical_save}")
    history = api.request("GET", f"/v1/history/{logical_save}")
    conflicts = api.request("GET", f"/v1/conflicts/{logical_save}")
    if head != snapshot2 or len(history) != 3 or len(conflicts) != 1:
        raise RuntimeError(
            f"history invariant failed: head={head} "
            f"history={len(history)} conflicts={len(conflicts)}"
        )

    bad_chunk = object_id(f"{run}:bad")
    bad_manifest = object_id(f"{run}:bad-manifest")
    bad = begin(
        api,
        account,
        device,
        f"{logical_save}-bad-checksum",
        bad_manifest,
        [bad_chunk],
        None,
        [],
    )
    _, body = encoded(b"corrupt-at-transport")
    api.request(
        "POST",
        f"/v1/snapshots/{bad['upload_id']}/chunks",
        {"chunk_id": bad_chunk, "sha256": "00" * 32, "bytes_b64": body},
        (400,),
    )

    print(
        json.dumps(
            {
                "backend": ready["backend"],
                "logical_save_id": logical_save,
                "head": head,
                "history_count": len(history),
                "conflict_count": len(conflicts),
                "dedupe_missing_count": len(branch_a["missing_chunk_ids"]),
                "checksum_fail_closed": True,
                "certificate_fail_closed": True,
                "account_root_immutable": True,
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
