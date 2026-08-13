#!/usr/bin/env python3
"""Prove critical UI evidence gates fail on deliberate temporary mutations."""
from pathlib import Path
import hashlib, json, shutil, subprocess, sys, tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)


with tempfile.TemporaryDirectory(prefix="mh3g-ui-negative-") as raw:
    temp = Path(raw) / "repo"
    shutil.copytree(ROOT, temp, ignore=shutil.ignore_patterns(".git", "target", "bin", "obj", "artifacts"))
    xaml = temp / "apps/mh3g-save-converter-windows/MainWindow.xaml"
    original = xaml.read_text(encoding="utf-8")
    xaml.write_text(original.replace('AutomationProperties.AutomationId="mh3g.converter.windows.action.inspect"', "", 1), encoding="utf-8")
    result = run([sys.executable, "scripts/verify-mh3g-save-converter-windows-ui-quality.py"], temp)
    if result.returncode == 0: raise SystemExit("missing primary action ID mutation was not rejected")

    contract = temp / ".ui-os/design/FROZEN_CONTRACT.md"
    contract.write_text(contract.read_text(encoding="utf-8").replace("920×600", "", 1), encoding="utf-8")
    result = run([sys.executable, "scripts/verify-mh3g-save-converter-windows-ui-quality.py"], temp)
    if result.returncode == 0: raise SystemExit("minimum-window contract mutation was not rejected")

    artifact = temp / "candidate.exe"
    artifact.write_bytes(b"candidate")
    evidence = temp / "evidence"
    evidence.mkdir()
    (evidence / "evidence-metadata.json").write_text(json.dumps({
        "artifact_sha256": hashlib.sha256(artifact.read_bytes()).hexdigest(),
        "commit": "expected",
    }), encoding="utf-8")
    artifact.write_bytes(b"tampered")
    result = run([sys.executable, "scripts/verify-mh3g-save-converter-windows-ui-evidence.py", str(evidence), "--artifact", str(artifact), "--commit", "expected"], temp)
    if result.returncode == 0: raise SystemExit("artifact tamper was not rejected")

    artifact.write_bytes(b"candidate")
    result = run([sys.executable, "scripts/verify-mh3g-save-converter-windows-ui-evidence.py", str(evidence), "--artifact", str(artifact), "--commit", "stale"], temp)
    if result.returncode == 0: raise SystemExit("stale commit was not rejected")

print("Windows UI Quality negative gates rejected all deliberate mutations.")
