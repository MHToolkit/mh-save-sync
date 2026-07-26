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
WINDOWS_WORKFLOW = ROOT / ".github" / "workflows" / "mh3g-converter-windows.yml"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def read(relative: str) -> str:
    return (APP / relative).read_text(encoding="utf-8")


def public_method_body(source: str, method: str) -> str:
    """Return one public async ViewModel method without scanning later methods."""
    marker = f"public async Task {method}()"
    start = source.index(marker)
    next_public = source.find("\n    public async Task ", start + len(marker))
    next_private = source.find("\n    private ", start + len(marker))
    endings = [index for index in (next_public, next_private) if index != -1]
    end = min(endings) if endings else len(source)
    return source[start:end]


def verify_release_workflow() -> None:
    """Keep the shipped WinUI application and its Rust sidecar inseparable."""
    workflow = WINDOWS_WORKFLOW.read_text(encoding="utf-8")

    for expected in (
        "- apps/mh3g-save-converter-windows/**",
        "- scripts/verify-mh3g-save-converter-windows-source.py",
        "actions/setup-dotnet@v5",
        "python scripts/verify-mh3g-save-converter-windows-source.py",
        "dotnet publish apps/mh3g-save-converter-windows/MH3GSaveConverter.Windows.csproj",
        "-p:Platform=x64",
        "-p:WindowsAppSDKSelfContained=true",
        '$stage = "artifacts/mh3g-save-convert-windows-x64"',
        'Copy-Item "target/release/mh3g-save-convert.exe" "$stage/tools/mh3g-save-convert.exe"',
        'Test-Path "$verifyDir/mh3g-save-convert-windows-x64/MH3GSaveConverter.exe"',
        'Test-Path "$verifyDir/mh3g-save-convert-windows-x64/tools/mh3g-save-convert.exe"',
        "GUI executable is missing",
        "GUI sidecar is missing",
    ):
        require(expected in workflow, f"Windows release workflow is missing {expected}")

    for forbidden in (
        "& $packagedApp",
        "Start-Process $packagedApp",
        'Start-Process "$packagedApp"',
    ):
        require(forbidden not in workflow, "release workflow must not launch the WinUI GUI during smoke verification")


def main() -> int:
    verify_release_workflow()

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

    launcher = (ROOT / "scripts" / "mh3g-windows-launcher.ps1").read_text(encoding="utf-8")
    require(
        'Join-Path $PSScriptRoot "tools/mh3g-save-convert.exe"' in launcher,
        "packaged launcher must resolve the WinUI tools sidecar first",
    )

    workflow = read("ViewModels/MainViewModel.cs")
    for expected in (
        '"convert", paths.Source, "--output", paths.Target, "--dry-run"',
        '"rollback", "--manifest", RollbackManifestPath',
        '"convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath, "--dry-run"',
        '"--write", "--experimental"',
        "_coreAuthorization",
        "_cecAuthorization",
        'Path.Combine(AppContext.BaseDirectory, "tools", "mh3g-save-convert.exe")',
    ):
        require(expected in workflow, f"workflow is missing {expected}")

    resolver = read("Models/SavePathResolution.cs")
    for expected in (
        "TryResolveSource",
        "TryResolveTarget",
        "TryResolveExtDataUserDirectory",
        "Path.Combine(fullPath, slot)",
        "Path.GetFileName(fullPath)",
    ):
        require(expected in resolver, f"Windows path resolver is missing {expected}")

    core_dry_run = workflow.split("public async Task RunCoreDryRunAsync()", 1)[1].split(
        "public async Task WriteCoreAsync()", 1
    )[0]
    for expected in (
        "_coreAuthorization = null;",
        'var reportSourceHash = result.TryGetHash("source");',
        'var reportTargetHash = result.TryGetHash("target_before");',
        "var targetMatchesDryRun = targetAfter.Exists",
        "? !string.IsNullOrWhiteSpace(targetAfter.Sha256)",
        "string.IsNullOrWhiteSpace(targetAfter.Sha256)",
        "string.IsNullOrWhiteSpace(reportTargetHash)",
        "string.Equals(targetAfter.Sha256, reportTargetHash, StringComparison.OrdinalIgnoreCase)",
        "new DryRunAuthorization(sourceAfter, targetAfter, reportSourceHash",
    ):
        require(expected in core_dry_run, f"core Dry Run is missing target hash validation {expected}")

    # A directory export is deliberately a new-file transaction. The target
    # state observed by Inspect is part of that intent: an absent target must
    # not turn into an in-place overwrite merely because another process
    # creates the selected user# before Dry Run starts.
    inspect_core = public_method_body(workflow, "InspectCoreAsync")
    for expected in (
        "var sourceAtInspection = await _fingerprints.CaptureAsync(paths.Source, cancellationToken);",
        "var targetAtInspection = await _fingerprints.CaptureAsync(paths.Target, cancellationToken);",
        "if (!sourceAtInspection.Matches(sourceAfterInspection) || !targetAtInspection.Matches(targetAfterInspection))",
        "_inspectedSource = sourceAfterInspection;",
        "_inspectedTarget = targetAfterInspection;",
    ):
        require(expected in inspect_core, f"core Inspect is missing stable target intent validation {expected}")
    for expected in (
        "var inspectedSource = _inspectedSource",
        "var inspectedTarget = _inspectedTarget",
        "if (!inspectedSource.Matches(sourceBefore) || !inspectedTarget.Matches(targetBefore))",
        "!sourceBefore.Matches(sourceAfter) || !targetBefore.Matches(targetAfter)",
    ):
        require(expected in core_dry_run, f"core Dry Run must preserve inspected target intent {expected}")

    core_write = workflow.split("public async Task WriteCoreAsync()", 1)[1].split(
        "public async Task RollbackCoreAsync()", 1
    )[0]
    for expected in (
        '"--expected-source-sha256", authorization.SourceReportHash',
        "var expectedTargetSha256 = authorization.Target.Sha256",
        "if (authorization.Target.Exists)",
        'arguments.Add("--expected-target-sha256");',
        "arguments.Add(expectedTargetSha256);",
        'arguments.Add("--expected-target-absent");',
    ):
        require(expected in core_write, f"core write is missing dry-run hash binding {expected}")

    require("public bool SelectedOptionalDataIsConfigured" in workflow, "Windows core workflow must gate selected optional setup")
    require("public bool HasPendingSelectedOptionalWork" in workflow, "Windows core workflow must retain selected optional work")
    require("!SelectedOptionalDataIsConfigured" in core_dry_run, "core Dry Run must not bypass incomplete optional setup")
    require(
        "SelectedOptionalDataIsConfigured" in workflow.split("public bool CanWriteCore", 1)[1].split("public bool CanRollbackCore", 1)[0],
        "core write availability must not bypass incomplete optional setup",
    )
    require(
        "!SelectedOptionalDataIsConfigured" in core_write,
        "core write entry point must reject incomplete optional setup",
    )
    optional_availability = workflow.split("private void RaiseOptionalConfigurationAvailability()", 1)[1].split(
        "private void SetWorkflowGuidance", 1
    )[0]
    require(
        "OnPropertyChanged(nameof(CanWriteCore));" in optional_availability,
        "changing optional setup must refresh core write availability",
    )
    system_write = public_method_body(workflow, "WriteSystemAsync")
    extras_install = public_method_body(workflow, "InstallExtrasAsync")
    require("_systemWriteCompleted = true;" in system_write, "system completion must be tracked independently")
    require("_extrasInstallCompleted = true;" in extras_install, "ExtData completion must be tracked independently")

    window = read("MainWindow.xaml")
    require('Click="GoToOptionalConfiguration_Click"' in window, "post-Inspect guidance must lead to optional setup")
    require('x:Name="OptionalConfigurationAnchor"' in window, "optional configuration requires a stable destination")
    require('Message="{Binding PostWriteGuidanceMessage}"' in window, "post-write guidance must account for selected optional data")
    require('Click="GoToPostWriteDestination_Click"' in window, "post-write CTA must choose its actual next destination")
    code_behind = read("MainWindow.xaml.cs")
    require("private void GoToOptionalConfiguration_Click" in code_behind, "optional configuration CTA handler is missing")
    require("OptionalConfigurationAnchor.StartBringIntoView();" in code_behind, "optional configuration CTA must scroll to its controls")
    require("private void GoToPostWriteDestination_Click" in code_behind, "post-write destination handler is missing")

    cec_write = workflow.split("public async Task WriteCecAsync()", 1)[1].split(
        "public async Task RollbackCecAsync()", 1
    )[0]
    require('"convert-cec --dry-run verification"' in cec_write, "CEC write must retain its planner recheck")
    require("--expected-source-sha256" not in cec_write, "CEC must not use core slot hash arguments")
    for expected in (
        '"--expected-source-record-set-sha256", authorization.SourceRecordSetSha256',
        '"--expected-target-sha256", authorization.TargetSha256Before',
    ):
        require(expected in cec_write, f"CEC write is missing Dry Run hash binding {expected}")

    cec_dry_run = workflow.split("public async Task RunCecDryRunAsync()", 1)[1].split(
        "public async Task WriteCecAsync()", 1
    )[0]
    require("_cecAuthorization = null;" in cec_dry_run, "CEC Dry Run must clear stale authorization before planning")
    require(
        'result.TryGetString("source_record_set_sha256")' in cec_dry_run,
        "CEC Dry Run must require the aggregate source record-set hash",
    )
    require(
        "Stage = WorkflowStage.DryRunAuthorized;" in cec_dry_run,
        "CEC Dry Run must update the visible workflow stage",
    )

    cec_inspect = workflow.split("public async Task InspectCecAsync()", 1)[1].split(
        "public async Task RunCecDryRunAsync()", 1
    )[0]
    require(
        "Stage = WorkflowStage.Inspected;" in cec_inspect,
        "CEC inspection must update the visible workflow stage",
    )
    for expected in (
        "Stage = WorkflowStage.Writing;",
        "Stage = WorkflowStage.Written;",
    ):
        require(expected in cec_write, f"CEC write must update its visible workflow stage {expected}")

    cec_rollback = workflow.split("public async Task RollbackCecAsync()", 1)[1].split(
        "private async Task RunOperationAsync", 1
    )[0]
    for expected in (
        "Stage = WorkflowStage.Writing;",
        "Stage = WorkflowStage.RolledBack;",
    ):
        require(expected in cec_rollback, f"CEC rollback must update its visible workflow stage {expected}")

    for expected in (
        "public async Task RunSystemDryRunAsync()",
        "public async Task WriteSystemAsync()",
        "public async Task RollbackSystemAsync()",
        "public async Task RunExtrasStageDryRunAsync()",
        "public async Task StageExtrasAsync()",
        "public async Task RunExtrasInstallDryRunAsync()",
        "public async Task InstallExtrasAsync()",
        "public async Task RollbackExtrasAsync()",
        "--expected-staging-set-sha256",
        "--expected-target-set-sha256",
        "_systemAuthorization",
        "_extrasStageAuthorization",
        "_extrasInstallAuthorization",
    ):
        require(expected in workflow, f"Windows optional transaction workflow is missing {expected}")
    for expected in (
        "private enum AuthorizationDomain",
        "private void ClearWriteAuthorization(AuthorizationDomain domain)",
        "ClearWriteAuthorization(failureDomain);",
        "AuthorizationDomain.Core",
        "AuthorizationDomain.System",
        "AuthorizationDomain.Extras",
        "AuthorizationDomain.Cec",
    ):
        require(expected in workflow, f"Windows workflow is missing isolated authorization handling {expected}")
    require(
        "ClearWriteAuthorizations();" not in workflow,
        "a failed operation must not revoke unrelated authorization domains",
    )

    # Every failed CLI operation must revoke only the authorization which
    # supplied that operation's guarded write. This keeps the other domains
    # fail-closed without forcing users to repeat unrelated Dry Runs.
    for method, domain in {
        "InspectCoreAsync": "Core",
        "InspectProgressAsync": "Core",
        "InspectEventsAsync": "Core",
        "RunCoreDryRunAsync": "Core",
        "WriteCoreAsync": "Core",
        "RollbackCoreAsync": "Core",
        "RunSystemDryRunAsync": "System",
        "WriteSystemAsync": "System",
        "RollbackSystemAsync": "System",
        "RunExtrasStageDryRunAsync": "Extras",
        "StageExtrasAsync": "Extras",
        "RunExtrasInstallDryRunAsync": "Extras",
        "InstallExtrasAsync": "Extras",
        "RollbackExtrasAsync": "Extras",
        "InspectCecAsync": "Cec",
        "RunCecDryRunAsync": "Cec",
        "WriteCecAsync": "Cec",
        "RollbackCecAsync": "Cec",
    }.items():
        body = public_method_body(workflow, method)
        require("await RunOperationAsync(" in body, f"{method} must run through the guarded operation wrapper")
        require(
            f"}}, AuthorizationDomain.{domain});" in body,
            f"{method} must revoke only the {domain} authorization on failure",
        )

    clear_authorization = workflow.split("private void ClearWriteAuthorization", 1)[1].split(
        "private void RaiseCoreActionAvailability", 1
    )[0]
    core_case = clear_authorization.split("case AuthorizationDomain.Core:", 1)[1].split(
        "case AuthorizationDomain.System:", 1
    )[0]
    system_case = clear_authorization.split("case AuthorizationDomain.System:", 1)[1].split(
        "case AuthorizationDomain.Extras:", 1
    )[0]
    extras_case = clear_authorization.split("case AuthorizationDomain.Extras:", 1)[1].split(
        "case AuthorizationDomain.Cec:", 1
    )[0]
    cec_case = clear_authorization.split("case AuthorizationDomain.Cec:", 1)[1].split("default:", 1)[0]
    for case, expected, forbidden in (
        (core_case, "_coreAuthorization = null;", ("_systemAuthorization", "_extras", "_cecAuthorization")),
        (system_case, "_systemAuthorization = null;", ("_coreAuthorization", "_extras", "_cecAuthorization")),
        (extras_case, "_extrasStageAuthorization = null;", ("_coreAuthorization", "_systemAuthorization", "_cecAuthorization")),
        (cec_case, "_cecAuthorization = null;", ("_coreAuthorization", "_systemAuthorization", "_extras")),
    ):
        require(expected in case, f"authorization clearing case is missing {expected}")
        for forbidden_name in forbidden:
            require(
                forbidden_name not in case,
                f"authorization clearing case must not revoke unrelated {forbidden_name}",
            )

    window = read("MainWindow.xaml")
    for expected in (
        "SystemDryRun_Click",
        "WriteSystem_Click",
        "ExtrasStageDryRun_Click",
        "InstallExtras_Click",
        "GuildCardsCheckBox",
        "QuestsCheckBox",
    ):
        require(expected in window, f"WinUI optional transaction controls are missing {expected}")

    code_behind = read("MainWindow.xaml.cs")
    for expected in (
        "ChooseSystemSource_Click",
        "ChooseExtrasSource_Click",
        "SystemDryRun_Click",
        "InstallExtras_Click",
    ):
        require(expected in code_behind, f"WinUI optional transaction handler is missing {expected}")

    copy = read("Infrastructure/ConverterCopy.cs")
    for expected in (
        "Simplified Chinese",
        "简体中文",
        "Experimental CEC",
        "实验性 CEC",
        "Shared system",
        "共享 system",
        "Optional ExtData",
        "可选 ExtData",
    ):
        require(expected in copy, f"localized copy is missing {expected}")
    require((APP / "README.zh-CN.md").is_file(), "Windows shell must include Chinese usage guidance")

    window = read("MainWindow.xaml")
    for expected in ("StageArtwork", "DryRun_Click", "CecToggle", "RollbackCore_Click"):
        require(expected in window, f"main surface is missing {expected}")
    for expected in (
        "ShowPostInspectGuidance",
        "ShowPostDryRunGuidance",
        "ShowPostWriteGuidance",
        "ShowPostOptionalGuidance",
        "ShowPostRollbackGuidance",
        "GoToPostWriteDestination_Click",
        "GoToCoreWorkflow_Click",
        "GoToOptionalConfiguration_Click",
    ):
        require(expected in window, f"main surface is missing guided continuation {expected}")
    for expected in (
        "WorkflowGuidance.CoreInspected",
        "WorkflowGuidance.CoreDryRunAuthorized",
        "WorkflowGuidance.CoreWritten",
        "WorkflowGuidance.OptionalStepComplete",
        "WorkflowGuidance.RolledBack",
    ):
        require(expected in workflow, f"workflow is missing guided continuation {expected}")
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
