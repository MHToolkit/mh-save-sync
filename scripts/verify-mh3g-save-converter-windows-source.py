#!/usr/bin/env python3
"""Fast source-level contract checks for the unpackaged WinUI shell.

This intentionally runs on macOS/Linux hosts where Windows App SDK compilation
is unavailable. A Windows x64 build remains the release gate.
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as element_tree
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps" / "mh3g-save-converter-windows"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def read(relative: str) -> str:
    return (APP / relative).read_text(encoding="utf-8")


def main() -> int:
    for relative in ("App.xaml", "MainWindow.xaml", "Controls/StageArtwork.xaml", "app.manifest"):
        element_tree.parse(APP / relative)

    project = read("MH3GSaveConverter.Windows.csproj")
    for expected in (
        "net8.0-windows10.0.19041.0",
        "<UseWinUI>true</UseWinUI>",
        "Microsoft.WindowsAppSDK",
        "<WindowsPackageType>None</WindowsPackageType>",
        "<PlatformTarget>x64</PlatformTarget>",
    ):
        require(expected in project, f"project is missing {expected}")
    for artwork in (
        "input-route.png",
        "components-workshop.png",
        "dry-run-flow.png",
        "rollback-harbor.png",
        "cec-mailbox.png",
    ):
        require((APP / "assets" / "Artwork" / artwork).is_file(), f"missing packaged artwork {artwork}")

    bridge = read("Services/ConverterCliClient.cs")
    for expected in ("UseShellExecute = false", "startInfo.ArgumentList.Add(argument)", "JsonDocument.Parse(candidate)"):
        require(expected in bridge, f"argv/JSON bridge is missing {expected}")
    require("startInfo.Arguments" not in bridge, "CLI bridge must not build a command-string argument list")
    require("cmd.exe" not in bridge and "powershell" not in bridge.lower(), "CLI bridge must not invoke a shell")

    workflow = read("ViewModels/MainViewModel.cs")
    for expected in (
        '"convert", SourcePath, "--output", TargetPath, "--dry-run"',
        '"rollback", "--manifest", RollbackManifestPath',
        '"convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath, "--dry-run"',
        '"--write", "--experimental"',
        "_coreAuthorization",
        "_cecAuthorization",
    ):
        require(expected in workflow, f"workflow is missing {expected}")

    core_write = workflow.split("public async Task WriteCoreAsync()", 1)[1].split(
        "public async Task RollbackCoreAsync()", 1
    )[0]
    for expected in (
        '"--expected-source-sha256", authorization.SourceReportHash',
        '"--expected-target-sha256", expectedTargetSha256',
        "var expectedTargetSha256 = authorization.Target.Sha256",
    ):
        require(expected in core_write, f"core write is missing dry-run hash binding {expected}")

    cec_write = workflow.split("public async Task WriteCecAsync()", 1)[1].split(
        "public async Task RollbackCecAsync()", 1
    )[0]
    require('"convert-cec --dry-run verification"' in cec_write, "CEC write must retain its planner recheck")
    require("--expected-source-sha256" not in cec_write, "CEC must not use core slot hash arguments")
    require("--expected-target-sha256" not in cec_write, "CEC must not use core slot hash arguments")

    copy = read("Infrastructure/ConverterCopy.cs")
    for expected in ("Simplified Chinese", "简体中文", "Experimental CEC", "实验性 CEC"):
        require(expected in copy, f"localized copy is missing {expected}")
    require((APP / "README.zh-CN.md").is_file(), "Windows shell must include Chinese usage guidance")

    window = read("MainWindow.xaml")
    for expected in ("StageArtwork", "DryRun_Click", "CecToggle", "RollbackCore_Click"):
        require(expected in window, f"main surface is missing {expected}")
    stage_artwork = read("Controls/StageArtwork.xaml.cs")
    require("SceneImage.Source" in stage_artwork, "stage artwork must change with workflow state")

    print("Windows WinUI source contract checks passed.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, element_tree.ParseError) as error:
        print(f"source contract check failed: {error}", file=sys.stderr)
        raise SystemExit(1)
