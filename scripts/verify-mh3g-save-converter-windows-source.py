#!/usr/bin/env python3
"""Fast source-level contract checks for the unpackaged WinUI shell.

This intentionally runs on macOS/Linux hosts where Windows App SDK compilation
is unavailable. A Windows x64 build remains the release gate.
"""

from __future__ import annotations

import json
import sys
import xml.etree.ElementTree as element_tree
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps" / "mh3g-save-converter-windows"
WINDOWS_WORKFLOW = ROOT / ".github" / "workflows" / "mh3g-converter-windows.yml"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "mh3g-converter-release.yml"
WINDOWS_PACKAGE_SCRIPT = ROOT / "scripts" / "package-mh3g-save-converter-windows.ps1"
GLOBAL_JSON = ROOT / "global.json"


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
    """Keep validation-only PR CI separate from tag-driven release packaging."""
    validation_workflow = WINDOWS_WORKFLOW.read_text(encoding="utf-8")
    release_workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
    sdk = json.loads(GLOBAL_JSON.read_text(encoding="utf-8")).get("sdk", {})
    require(
        sdk.get("version") == "8.0.100" and sdk.get("rollForward") == "latestFeature",
        "global.json must pin WinUI builds to the .NET 8 feature band",
    )

    for expected in (
        "- global.json",
        "- apps/mh3g-save-converter-windows/**",
        "- scripts/package-mh3g-save-converter-windows.ps1",
        "- scripts/verify-mh3g-save-converter-windows-source.py",
        "actions/setup-dotnet@v5",
        "python scripts/verify-mh3g-save-converter-windows-source.py",
        "scripts/package-mh3g-save-converter-windows.ps1",
        "-ValidateOnly",
        "if: failure()",
    ):
        require(
            expected in validation_workflow,
            f"Windows validation workflow is missing {expected}",
        )

    for release_artifact in (
        "mh3g-save-convert-windows-x64.zip",
        "mh3g-save-convert-windows-x64.zip.sha256",
        "MH3GSaveConverter-Setup-x64.exe",
        "MH3GSaveConverter-Setup-x64.exe.sha256",
        "MH3GSaveConverter-Portable-x64.exe",
        "MH3GSaveConverter-Portable-x64.exe.sha256",
    ):
        require(
            release_artifact not in validation_workflow,
            f"Windows validation workflow must not upload release artifact {release_artifact}",
        )
        require(
            release_artifact in release_workflow,
            f"tag release workflow is missing {release_artifact}",
        )

    for expected in (
        "tags:",
        '"v*"',
        "package Windows x64 release artifacts",
        "package-mh3g-save-converter-windows.ps1 -Bootstrap",
    ):
        require(expected in release_workflow, f"tag release workflow is missing {expected}")
    require("-ValidateOnly" not in release_workflow, "tag release workflow must build real Windows artifacts")

    for workflow in (validation_workflow, release_workflow):
        for forbidden in (
            "& $packagedApp",
            "Start-Process $packagedApp",
            'Start-Process "$packagedApp"',
            "dotnet publish apps/mh3g-save-converter-windows/MH3GSaveConverter.Windows.csproj",
        ):
            require(forbidden not in workflow, "workflows must delegate packaging without launching the WinUI GUI")


def verify_local_packaging_script() -> None:
    """Keep the documented Windows-local package path complete and deterministic."""
    require(WINDOWS_PACKAGE_SCRIPT.is_file(), "Windows local packaging script is missing")
    script = WINDOWS_PACKAGE_SCRIPT.read_text(encoding="utf-8")

    for expected in (
        "$PSScriptRoot",
        "[Environment]::Is64BitOperatingSystem",
        "[switch]$Bootstrap",
        "winget",
        "vswhere.exe",
        "Get-AnyVisualStudioInstallation",
        "setup.exe",
        "--installPath",
        "3010",
        "1641",
        "VsDevCmd.bat",
        "Add-RustupBinToProcessPath",
        "Get-RustToolCommand",
        "-1978335189",
        "ExpandEnvironmentVariables",
        "UTF8Encoding",
        "Test-RustMinimumVersion",
        "Get-DotnetEightSdkCommand",
        "MH3GSaveConverter\\BuildTools\\dotnet8\\dotnet.exe",
        "1.95.0",
        "Test-WindowsSdk",
        "Microsoft.VisualStudio.Component.Windows10SDK.19041",
        "Get-InnoSetupCompiler",
        "Test-InnoSetup",
        "JRSoftware.InnoSetup",
        "MH3GSaveConverter.iss",
        "dotnet restore",
        "cargo test --locked",
        "cargo build --locked --release",
        "x86_64-pc-windows-msvc",
        "dotnet publish",
        '"--self-contained", "true"',
        "WindowsAppSDKSelfContained=true",
        "PublishSingleFile=true",
        "IncludeAllContentForSelfExtract=true",
        "Publish-PortableExecutable",
        "Build-InstallerExecutable",
        "MH3GSaveConverter-Portable-x64.exe",
        "MH3GSaveConverter-Setup-x64.exe",
        "mh3g-save-convert.exe",
        "Run-Converter.ps1",
        "Zone.Identifier",
        "ReparsePoint",
        "Start-Transcript -LiteralPath $transcript -Force",
        "Get-FileHash",
        "Compress-Archive",
        "mh3g-save-convert-windows-x64.zip",
        "Get-RunningSupportedEmulators",
        "Assert-EmulatorsStoppedForRustTests",
        "$packageDirectoryName",
        "& $FilePath @Arguments | Out-Host",
    ):
        require(expected in script, f"Windows local packaging script is missing {expected}")

    for forbidden in (
        "Invoke-Expression",
        "IEX ",
        ".Source",
        "Start-Transcript -LiteralPath $transcript -Append",
    ):
        require(forbidden not in script, f"Windows local packaging script must not use {forbidden}")

    parameter_block = script.split("Set-StrictMode", 1)[0]
    require(
        "Join-Path $PSScriptRoot" not in parameter_block,
        "Windows PowerShell must not resolve $PSScriptRoot inside a parameter default",
    )
    require(
        "$OutputDirectory = Join-Path $repoRoot" in script,
        "Windows package output must resolve only after $PSScriptRoot is available",
    )
    require(
        "$dotnet = Get-DotnetEightSdkCommand" in script,
        "Windows package build must reuse the same .NET 8 SDK resolver as preflight",
    )
    dotnet_resolver = script.split("function Get-DotnetEightSdkCommand", 1)[1].split(
        "function Test-DotnetEightSdk", 1
    )[0]
    require(
        "BuildTools\\dotnet8\\dotnet.exe" in dotnet_resolver
        and "LocalApplicationData" in dotnet_resolver
        and "--list-sdks" in dotnet_resolver,
        "Windows .NET resolver must reuse a valid private MH3G .NET 8 SDK instead of downloading it again",
    )
    package_layout = script.split("function Test-PackagedLayout", 1)[1].split("try {", 1)[0]
    require(
        "PackageDirectoryName" in package_layout
        and "$packageRoot = Join-Path $verifyRoot $PackageDirectoryName" in script
        and "Test-PackagedLayout -ArchivePath $archive -PackageDirectoryName $packageDirectoryName" in script,
        "Windows package self-check must follow a custom staging directory leaf rather than a hard-coded archive root",
    )
    require(
        "if (-not $SkipTests) {\n        Assert-EmulatorsStoppedForRustTests\n    }\n\n    Write-Host \"=== Toolchain ===\"" in script,
        "Windows package tests must fail before toolchain work with a clear emulator-process explanation instead of leaking a synthetic test failure",
    )
    require(
        "reparse point" in script.lower(),
        "Windows package output cleanup must reject junctions and symlinks",
    )
    require(
        "Assert-NativeConverterSidecar" in script
        and "mh3g-save-convert-core.exe" in script,
        "Windows packaging must reject the legacy compatibility wrapper before publishing",
    )
    require(
        "Test-InnoSetup" in script
        and 'Install-WithWinget -Id "JRSoftware.InnoSetup"' in script
        and "Inno Setup 6" in script,
        "Windows bootstrap must install Inno Setup only when the installer compiler is absent",
    )
    portable_publish = script.split("function Publish-PortableExecutable", 1)[1].split(
        "function Build-InstallerExecutable", 1
    )[0]
    for expected in (
        "-p:PublishSingleFile=true",
        "-p:IncludeAllContentForSelfExtract=true",
        "mh3g-save-convert.exe",
        "finally",
        "Remove-Item -LiteralPath $embeddedSidecar",
    ):
        require(expected in portable_publish, f"portable Windows publish is missing {expected}")
    installer_build = script.split("function Build-InstallerExecutable", 1)[1].split(
        "function Test-PackagedLayout", 1
    )[0]
    for expected in (
        '"/DSourceDir=$SourceDirectory"',
        '"/DOutputDir=$artifactsRoot"',
        '"/DAppVersion=$Version"',
    ):
        require(expected in installer_build, f"Inno Setup build is missing {expected}")
    require(
        '$versionLine[0] -match \'^rustc\\s+(?<version>\\d+\\.\\d+\\.\\d+)\'' in script
        and '$versionText = $Matches["version"]' in script
        and '$rustcExitCode = $LASTEXITCODE' in script,
        "Rust version preflight must capture a successful regex match before parsing it",
    )
    rust_version_body = script.split("function Test-RustMinimumVersion", 1)[1].split(
        "function Get-MissingPrerequisites", 1
    )[0]
    require(
        "Select-Object -First 1" not in rust_version_body,
        "Rust version preflight must not stop the native rustc process through Select-Object -First 1",
    )
    rust_bootstrap = script.split("function Install-MissingPrerequisites", 1)[1].split(
        "function Initialize-MsvcBuildEnvironment", 1
    )[0]
    require(
        'Install-WithWinget -Id "Rustlang.Rustup" -AcceptedExitCodes @(0, -1978335189)' in rust_bootstrap,
        "Rustup bootstrap must treat WinGet's installed/no-update result as non-fatal",
    )
    # WinGet can retain Rustlang.Rustup's installed registration after the
    # current user's rustup/cargo proxy payload has disappeared.  A normal
    # `winget install` then returns the documented no-update code, so the
    # bootstrap must repair that payload in place before giving up.  Keep the
    # recovery ladder deliberately narrow: no uninstall, no deletion of
    # CARGO_HOME/RUSTUP_HOME, and no blind executable download.
    for expected in (
        "function Repair-RustupWithWinget",
        "function Install-RustupFromOfficialSource",
        "function Get-RustupProbePaths",
        "function Get-RustupRecoveryHomes",
        "function Set-RustupHomesForCurrentProcess",
        "[switch]$ForceReinstall",
        'Install-WithWinget -Id "Rustlang.Rustup" -ForceReinstall',
        "Repair-RustupWithWinget",
        "Install-RustupFromOfficialSource",
        "--no-update-default-toolchain",
        "--no-modify-path",
        "rustup-init.exe.sha256",
        "Get-FileHash",
    ):
        require(expected in script, f"Windows Rustup recovery ladder is missing {expected}")
    require(
        "Rustup recovery failed" in rust_bootstrap,
        "Rustup bootstrap must report a failed repair ladder only after the rechecks",
    )
    for forbidden in ("rustup self uninstall", "winget uninstall", "Remove-Item"):
        require(
            forbidden not in rust_bootstrap,
            "Rustup bootstrap must repair in place without deleting an existing toolchain",
        )
    normal_rustup_install = rust_bootstrap.index(
        'Install-WithWinget -Id "Rustlang.Rustup" -AcceptedExitCodes @(0, -1978335189)'
    )
    repair_rustup = rust_bootstrap.index("Repair-RustupWithWinget")
    force_rustup = rust_bootstrap.index(
        'Install-WithWinget -Id "Rustlang.Rustup" -ForceReinstall'
    )
    official_rustup = rust_bootstrap.index("Install-RustupFromOfficialSource")
    recovery_error = rust_bootstrap.index("Rustup recovery failed")
    require(
        normal_rustup_install < repair_rustup < force_rustup < official_rustup < recovery_error,
        "Rustup recovery order must be normal install, repair, force reinstall, verified official fallback, then fail closed",
    )
    first_preflight = script.split("Start-Transcript -LiteralPath $transcript -Force", 1)[1].split(
        "$missing = @(Get-MissingPrerequisites)", 1
    )[0]
    require(
        "Refresh-ProcessPath" in first_preflight
        and "Set-RustupHomesForCurrentProcess" in first_preflight
        and "Add-RustupBinToProcessPath" in first_preflight,
        "initial preflight must restore the current user's Rustup homes and PATH before checking prerequisites",
    )
    visual_studio_modify = script.split("function Install-VisualStudioBuildComponents", 1)[1].split(
        "function Install-MissingPrerequisites", 1
    )[0]
    component_arguments = visual_studio_modify.split("$components = @(", 1)[1].split(
        ")\n    $existingInstallation", 1
    )[0]
    require(
        '"--wait"' not in component_arguments,
        "installed Visual Studio setup.exe modify must not receive bootstrapper-only --wait",
    )


def main() -> int:
    verify_release_workflow()
    verify_local_packaging_script()

    for relative in ("App.xaml", "MainWindow.xaml", "Controls/StageArtwork.xaml", "app.manifest"):
        element_tree.parse(APP / relative)
    app_xaml = read("App.xaml")
    require(
        "<ResourceDictionary.MergedDictionaries>" in app_xaml
        and "<controls:XamlControlsResources />" in app_xaml,
        "App resources must merge WinUI control resources into one ResourceDictionary",
    )

    installer_definition = ROOT / "packaging" / "mh3g-save-convert" / "MH3GSaveConverter.iss"
    require(installer_definition.is_file(), "Inno Setup installer definition is missing")
    installer_text = installer_definition.read_text(encoding="utf-8")
    for expected in (
        "PrivilegesRequired=lowest",
        "ArchitecturesAllowed=x64compatible",
        "MH3GSaveConverter-Setup-x64",
        "tools",
    ):
        require(expected in installer_text, f"Inno Setup definition is missing {expected}")

    project = read("MH3GSaveConverter.Windows.csproj")
    for expected in (
        "net8.0-windows10.0.19041.0",
        "<UseWinUI>true</UseWinUI>",
        "Microsoft.WindowsAppSDK",
        "<WindowsPackageType>None</WindowsPackageType>",
        "<PlatformTarget>x64</PlatformTarget>",
        "<IncludeInSingleFile>true</IncludeInSingleFile>",
    ):
        require(expected in project, f"project is missing {expected}")
    require(
        "assets\\" not in project
        and '<Content Update="Assets\\Artwork\\*.png">' in project
        and '<Content Update="Assets\\MH3GSaveConverter.ico">' in project
        and '<None Update="Assets\\Artwork\\*.png">' not in project,
        "WinUI package assets must use one canonical Windows-relative casing",
    )
    for artwork in (
        "input-route.png",
        "components-workshop.png",
        "dry-run-flow.png",
        "rollback-harbor.png",
        "cec-mailbox.png",
    ):
        require((APP / "Assets" / "Artwork" / artwork).is_file(), f"missing packaged artwork {artwork}")

    bridge = read("Services/ConverterCliClient.cs")
    for expected in (
        "UseShellExecute = false",
        "startInfo.ArgumentList.Add(argument)",
        "JsonDocument.Parse(candidate)",
        "mh3g-save-convert-core.exe",
        "Legacy compatibility wrapper",
    ):
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
        'Path.GetFileName(RollbackManifestPath)',
        '".mh3g-compatibility-repair-"',
        'isCompatibilityRollback ? "rollback-repair" : "rollback"',
        'new[] { operation, "--manifest", RollbackManifestPath }',
        '"convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath, "--dry-run"',
        '"--write", "--experimental"',
        "_coreAuthorization",
        "_cecAuthorization",
        'Path.Combine(AppContext.BaseDirectory, "tools", "mh3g-save-convert.exe")',
    ):
        require(expected in workflow, f"workflow is missing {expected}")
    for expected in (
        "ConversionMode.RepairConverted",
        "RepairDryRunAuthorization",
        '"repair-converted", paths.Source, "--current", paths.Target',
        'arguments.Add("--source-extdata-dir");',
        'arguments.Add("--from-version");',
        '"--expected-source-set-sha256"',
        '"--expected-current-set-sha256"',
        '"--expected-preview-sha256"',
        'TryGetProperty("detection", out var detection)',
        'TryGetString("compatibility_manifest")',
        '"rollback-repair"',
    ):
        require(expected in workflow, f"Windows compatibility-repair workflow is missing {expected}")

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
        "_coreAuthorization = new DryRunAuthorization(",
        "sourceAfter, targetAfter, reportSourceHash, DateTimeOffset.UtcNow",
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
        "var conversionAuthorization = authorization",
        '"--expected-source-sha256", conversionAuthorization.SourceReportHash',
        "var expectedTargetSha256 = conversionAuthorization.Target.Sha256",
        "if (conversionAuthorization.Target.Exists)",
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
    require("_systemWriteCompleted = true;" in system_write, "system completion must be tracked independently")
    require(
        "_extrasInstallCompleted" not in workflow,
        "Windows must not track a completed ExtData install while that capability is unavailable",
    )

    window = read("MainWindow.xaml")
    require(
        'Symbol="ProtectedDocument"' in window
        and 'ScrollViewer.VerticalScrollBarVisibility="Auto"' in window,
        "WinUI XAML must use valid Symbol and TextBox scrollbar members",
    )
    app_xaml = read("App.xaml")
    require(
        "BooleanNegationConverter" not in app_xaml
        and 'Visibility="{Binding WriteUnavailableVisibility}"' in window
        and 'Visibility="{Binding LatestReportEmptyVisibility}"' in window,
        "WinUI must avoid an App-resource converter that dotnet publish cannot resolve",
    )
    require('Click="GoToOptionalConfiguration_Click"' in window, "post-Inspect guidance must lead to optional setup")
    require('x:Name="OptionalConfigurationAnchor"' in window, "optional configuration requires a stable destination")
    require('Message="{Binding PostWriteGuidanceMessage}"' in window, "post-write guidance must account for selected optional data")
    require('Click="GoToPostWriteDestination_Click"' in window, "post-write CTA must choose its actual next destination")
    code_behind = read("MainWindow.xaml.cs")
    require(
        "RootGrid.DataContext = ViewModel;" in code_behind
        and "DataContext = ViewModel;" not in code_behind.replace("RootGrid.DataContext = ViewModel;", ""),
        "WinUI Window must set DataContext on its root FrameworkElement",
    )
    require(
        "using Microsoft.UI.Xaml.Media;" in code_behind
        and "SystemBackdrop = new MicaBackdrop();" in code_behind,
        "MicaBackdrop must resolve from Microsoft.UI.Xaml.Media",
    )
    require(
        "sender == SourcePathBox" not in code_behind
        and "ReferenceEquals(sender, SourcePathBox)" in code_behind,
        "WinUI TextChanged routing must use explicit reference equality",
    )
    write_core = public_method_body(workflow, "WriteCoreAsync")
    require(
        "var repairArguments = new List<string>" in write_core
        and "ExecuteAsync(operation, repairArguments, cancellationToken)" in write_core,
        "repair write arguments must not shadow the normal conversion arguments",
    )
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

    # The Windows backend now uses ReplaceFileW, a manifest-bound backup, and a
    # durable recovery journal. The native shell must expose the same guarded
    # ExtData write and rollback flow as the sidecar instead of retaining the
    # former platform capability block.
    for expected in (
        "private static bool SupportsSafeExtrasInstall => true;",
        "!SupportsSafeExtrasInstall || !HasSelectedExtraGroups() || HasExtrasInstallPaths()",
        "public bool CanRunExtrasStageDryRun => !IsBusy && HasExtrasStagePaths();",
        "public bool CanStageExtras => !IsBusy && _extrasStageAuthorization is not null && HasExtrasStagePaths();",
        "public bool CanRunExtrasInstallDryRun => !IsBusy && HasExtrasInstallPaths();",
        "SupportsSafeExtrasInstall && !IsBusy && _extrasInstallAuthorization is not null && HasExtrasInstallPaths()",
        "SupportsSafeExtrasInstall && !IsBusy && HasSelectedExtraGroups()",
        "private bool HasExtrasStagePaths()",
        "private bool HasExtrasInstallPaths()",
        "private bool TryRequireExtrasStagePaths()",
    ):
        require(expected in workflow, f"Windows ExtData safety capability is missing {expected}")
    for method in ("InstallExtrasAsync", "RollbackExtrasAsync"):
        body = public_method_body(workflow, method)
        require(
            "if (!SupportsSafeExtrasInstall)" not in body,
            f"{method} must not retain the obsolete Windows ExtData capability block",
        )
    extras_install_preview = public_method_body(workflow, "RunExtrasInstallDryRunAsync")
    require(
        "if (!SupportsSafeExtrasInstall)" not in extras_install_preview
        and "TryRequireExtrasInstallPaths()" in extras_install_preview
        and '"install-extras --dry-run"' in extras_install_preview,
        "Windows may offer only the read-only ExtData install preview; it must not be blocked by the write capability gate",
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
        "ConversionModePicker",
        "RepairVersionPicker",
        "RepairDetectionSummary",
        "SystemDryRun_Click",
        "WriteSystem_Click",
        "ExtrasStageDryRun_Click",
        "InstallExtras_Click",
        "GuildCardsCheckBox",
        "QuestsCheckBox",
    ):
        require(expected in window, f"WinUI optional transaction controls are missing {expected}")
    require(
        "Copy.ExtDataInstallUnavailable" in window,
        "Windows ExtData controls must explain why automatic install is unavailable",
    )
    require(
        "<InfoBar " not in window
        and '<controls:InfoBar IsOpen="True" IsClosable="False" Severity="Warning" Message="{Binding Copy.ExtDataInstallUnavailable}" />' in window,
        "every WinUI InfoBar must use the Microsoft.UI.Xaml.Controls namespace prefix",
    )

    code_behind = read("MainWindow.xaml.cs")
    for expected in (
        "ConversionModePicker_SelectionChanged",
        "RepairVersionPicker_SelectionChanged",
        "ChooseSystemSource_Click",
        "ChooseExtrasSource_Click",
        "SystemDryRun_Click",
        "InstallExtras_Click",
    ):
        require(expected in code_behind, f"WinUI optional transaction handler is missing {expected}")

    copy = read("Infrastructure/ConverterCopy.cs")
    for expected in (
        "Repair an already converted save",
        "修复已转换存档",
        "Original converter version",
        "原转换器版本",
        "Simplified Chinese",
        "简体中文",
        "Experimental CEC",
        "实验性 CEC",
        "Shared system",
        "共享 system",
        "Optional ExtData",
        "可选 ExtData",
        "ExtDataInstallUnavailable",
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
