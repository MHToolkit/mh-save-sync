#!/usr/bin/env python3
"""Fail closed on incomplete or stale native Windows fixture evidence."""
from pathlib import Path
import argparse, hashlib, json

p = argparse.ArgumentParser()
p.add_argument("evidence_dir", type=Path)
p.add_argument("--artifact", type=Path, required=True)
p.add_argument("--commit", required=True)
args = p.parse_args()

fixtures = {
    "first-run", "input.empty", "components.optional-missing", "components.optional-skipped",
    "dry-run.ready", "dry-run.blocked", "write.authorized", "write.confirmation",
    "conversion.success", "conversion.failure", "history.empty", "history.result",
}
meta_path = args.evidence_dir / "evidence-metadata.json"
if not meta_path.is_file(): raise SystemExit("missing evidence-metadata.json")
meta = json.loads(meta_path.read_text(encoding="utf-8-sig"))
actual = hashlib.sha256(args.artifact.read_bytes()).hexdigest()
if meta.get("artifact_sha256") != actual: raise SystemExit("artifact tamper or stale evidence")
if meta.get("commit") != args.commit: raise SystemExit("stale commit evidence")
for size in ("1120x760", "920x600"):
    for motion in ("normal", "reduced"):
        for fixture in fixtures:
            png = args.evidence_dir / f"{fixture}-{size}-{motion}.png"
            uia = args.evidence_dir / f"{fixture}-{size}-{motion}-uia.json"
            if not png.is_file() or png.stat().st_size < 1000: raise SystemExit(f"missing screenshot {png.name}")
            if not uia.is_file(): raise SystemExit(f"missing UIA tree {uia.name}")
            if fixture == "dry-run.ready":
                nodes = json.loads(uia.read_text(encoding="utf-8-sig"))
                report = next(
                    (node for node in nodes if node.get("id") == "mh3g.converter.windows.details.dryRun.report"),
                    None,
                )
                if report is None or '"status":"dry-run"' not in (report.get("value") or ""):
                    raise SystemExit(f"expanded Dry Run technical report is missing from {uia.name}")
print("Native Windows UI evidence is complete and artifact/commit bound.")
