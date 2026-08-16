#!/usr/bin/env python3
"""Deterministic source contract for the Windows UIOS candidate.

This gate intentionally does not claim native Windows rendering. It verifies
the frozen information architecture, semantic IDs, fixture isolation, motion
preference, and optional/core scope separation before runtime capture.
"""

from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps" / "mh3g-save-converter-windows"
CONTRACT = (ROOT / ".ui-os" / "design" / "FROZEN_CONTRACT.md").read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"Windows UI Quality source contract failed: {message}")


xaml_path = APP / "MainWindow.xaml"
xaml = xaml_path.read_text(encoding="utf-8")
code = (APP / "MainWindow.xaml.cs").read_text(encoding="utf-8")
vm = (APP / "ViewModels" / "MainViewModel.cs").read_text(encoding="utf-8")
app = (APP / "App.xaml.cs").read_text(encoding="utf-8")
motion = (APP / "Services" / "MotionPreferenceService.cs").read_text(encoding="utf-8")
ET.parse(xaml_path)
ET.parse(APP / "App.xaml")

required_ids = {
    "mh3g.converter.windows.navigation.convert",
    "mh3g.converter.windows.navigation.history",
    "mh3g.converter.windows.navigation.experimentalCEC",
    "mh3g.converter.windows.navigation.settings",
    "mh3g.converter.windows.page.input.title",
    "mh3g.converter.windows.page.optionals.title",
    "mh3g.converter.windows.page.dryRun.title",
    "mh3g.converter.windows.page.writeResult.title",
    "mh3g.converter.windows.page.history.title",
    "mh3g.converter.windows.page.experimentalCEC.title",
    "mh3g.converter.windows.page.settings.title",
    "mh3g.converter.windows.action.inspect",
    "mh3g.converter.windows.action.continueOptionals",
    "mh3g.converter.windows.action.runDryRun",
    "mh3g.converter.windows.action.confirmWrite",
    "mh3g.converter.windows.action.startConversion",
    "mh3g.converter.windows.action.skipOptional",
    "mh3g.converter.windows.action.rollback",
    "mh3g.converter.windows.path.source",
    "mh3g.converter.windows.path.current",
    "mh3g.converter.windows.path.output",
    "mh3g.converter.windows.details.dryRun",
    "mh3g.converter.windows.details.dryRun.empty",
    "mh3g.converter.windows.details.dryRun.report",
}
ids = re.findall(r'AutomationProperties\.AutomationId="([^"]+)"', xaml)
require(required_ids.issubset(set(ids)), f"missing IDs: {sorted(required_ids - set(ids))}")
require(len(ids) == len(set(ids)), "AutomationId values must be unique")
require(xaml.count("<controls:NavigationView ") == 1, "exactly one primary NavigationView is required")
require("920×600" in CONTRACT and "1,120×760" in CONTRACT,
        "default/minimum viewport contract is missing")
require("StageArtwork" not in xaml and "HeroTitle" not in xaml, "legacy hero/artwork must be deleted")
require("#FF" not in xaml and "#FFF" not in xaml, "work surfaces cannot hard-code light colors")
history_surface = xaml.split('x:Name="HistoryPage"', 1)[1].split('<!-- Experimental CEC -->', 1)[0]
settings_surface = xaml.split('x:Name="SettingsPage"', 1)[1].split('</controls:NavigationView>', 1)[0]
require("OptionalMissingReason" not in history_surface, "History must not host a conversion blocker")
require("OptionalMissingReason" not in settings_surface, "Settings must not host a conversion blocker")

fixtures = {
    "first-run", "input.empty", "components.optional-missing", "components.optional-skipped",
    "dry-run.ready", "dry-run.blocked", "write.authorized", "write.confirmation",
    "conversion.success", "conversion.failure", "history.empty", "history.result",
}
for fixture in fixtures:
    require(f'"{fixture}"' in vm, f"fixture missing: {fixture}")
require('"--ui-fixture"' in app and "MH3G_UI_FIXTURE" in app, "fixture launch parsing missing")
require("_fixtureId is not null" in code and "FixtureBlocksActions" in code,
        "fixture must block network, picker, and action execution")
require("UISettings().AnimationsEnabled" in motion, "Windows reduce-motion preference is not read")
require("_motionPreferences.AnimationsEnabled" in code, "motion seam must consult Windows preference")

dry_run = vm.split("public async Task RunCoreDryRunAsync()", 1)[1].split("public async Task WriteCoreAsync()", 1)[0]
write = vm.split("public async Task WriteCoreAsync()", 1)[1].split("public async Task RollbackCoreAsync()", 1)[0]
can_write = vm.split("public bool CanWriteCore", 1)[1].split("public bool CanRollbackCore", 1)[0]
require("SelectedOptionalDataIsConfigured" not in dry_run, "optional config still blocks core Dry Run")
require("SelectedOptionalDataIsConfigured" not in write, "optional config still blocks core write")
require("SelectedOptionalDataIsConfigured" not in can_write, "optional config still gates core CTA")
require("CommitRepairOptionalScope" in vm and "_repairGuildCardSource" in vm,
        "repair guild-card scope is not explicit/authorization-bound")
repair_commit = vm.split("public bool CommitRepairOptionalScope(bool skip)", 1)[1].split(
    "public string SourcePath", 1
)[0]
require("InvalidateCoreWriteAuthorizationPreservingInspection();" in repair_commit,
        "repair optional transition must preserve completed core inspection")
require("InvalidateCoreAuthorization();" not in repair_commit,
        "repair optional transition still clears completed core inspection")
require('Text="{Binding Copy.ResultEmpty}"' in xaml
        and 'Text="{Binding LatestReport, Mode=OneWay}"' in xaml,
        "Dry Run technical details need both an empty state and a read-only report")
require("_extrasInstallCompleted = true;" in vm and "_extrasInstallCompleted = false;" in vm,
        "ExtData independent completion lifecycle is incomplete")

print("Windows UI Quality source contract checks passed.")
