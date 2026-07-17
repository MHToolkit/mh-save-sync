#!/usr/bin/env python3
"""Two-phase upload-resume probe used around a server container restart."""

from __future__ import annotations

import argparse
import hashlib
import json
import time
from pathlib import Path

from importlib.machinery import SourceFileLoader


COMMON = SourceFileLoader(
    "compose_e2e", str(Path(__file__).with_name("compose-e2e.py"))
).load_module()


def identifier(label: str) -> str:
    return hashlib.sha256(label.encode()).hexdigest()


def bootstrap(api: object) -> tuple[str, str]:
    identity = COMMON.public_identity_fixture()
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
        "/v1/devices/register",
        {
            "account_handle": account,
            "cert_id": device,
            "device_public_key_b64": identity["device_public_key_b64"],
            "certificate_b64": identity["certificate_b64"],
        },
        (201,),
    )
    return account, device


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=("prepare", "finish"))
    parser.add_argument("state_file", type=Path)
    parser.add_argument("--base-url", default="http://127.0.0.1:18080")
    args = parser.parse_args()
    api = COMMON.Api(args.base_url)

    if args.phase == "prepare":
        account, device = bootstrap(api)
        run = str(time.time_ns())
        logical_save = f"resume-{run}"
        chunk = identifier(f"{run}:chunk")
        manifest = identifier(f"{run}:manifest")
        snapshot = identifier(f"{run}:snapshot")
        session = COMMON.begin(
            api,
            account,
            device,
            logical_save,
            manifest,
            [chunk],
            None,
            [],
        )
        COMMON.put_object(
            api, session["upload_id"], "chunk", chunk, b"encrypted-resume-chunk"
        )
        args.state_file.parent.mkdir(parents=True, exist_ok=True)
        args.state_file.write_text(
            json.dumps(
                {
                    "upload_id": session["upload_id"],
                    "logical_save": logical_save,
                    "manifest": manifest,
                    "snapshot": snapshot,
                }
            )
        )
        print(json.dumps({"prepared": True, "logical_save_id": logical_save}))
        return

    state = json.loads(args.state_file.read_text())
    COMMON.put_object(
        api,
        state["upload_id"],
        "manifest",
        state["manifest"],
        b"encrypted-resume-manifest",
    )
    outcome = COMMON.commit(api, state["upload_id"], state["snapshot"])
    head = api.request("GET", f"/v1/heads/{state['logical_save']}")
    if outcome["outcome"] != "first-snapshot" or head != state["snapshot"]:
        raise RuntimeError(f"resume invariant failed: {outcome=} {head=}")
    print(
        json.dumps(
            {
                "resumed_after_restart": True,
                "logical_save_id": state["logical_save"],
                "head": head,
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
