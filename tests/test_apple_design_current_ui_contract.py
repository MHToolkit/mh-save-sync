from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_macos_converter_stage_rail_has_dynamic_type_fallbacks_and_state_icons():
    view = read("apps/mh3g-save-converter-macos/Sources/MH3GSaveConverterMac/ConversionWorkbenchView.swift")
    presentation = read(
        "apps/mh3g-save-converter-macos/Sources/ConverterPresentation/WorkflowStageRailPresentation.swift"
    )

    assert "ViewThatFits(in: .horizontal)" in view
    assert "twoColumnRail" in view
    assert "verticalRail" in view
    assert ".lineLimit(1)" not in view
    assert ".minimumScaleFactor" not in view
    assert "accessibilityIdentifier(\"mh3g.converter.stageRail." in view
    assert 'return "exclamationmark.triangle.fill"' in presentation
    assert 'return "checkmark.circle.fill"' in presentation
    assert "fallbackOrder: [.horizontal, .twoColumnGrid, .vertical]" in presentation


def test_android_save_sync_status_rail_uses_full_width_steps_and_reduced_motion_token():
    activity = read("apps/android/app/src/main/java/org/mhtoolkit/savesync/MainActivity.kt")
    design = read("apps/android/app/src/main/java/org/mhtoolkit/savesync/SaveSyncDesignSystem.kt")

    assert "rememberSaveSyncMotionDurationMillis" in design
    assert "statusRailLayoutFallbacks = listOf(\"full-width-column\", \"live-region-summary\")" in design
    assert "SaveSyncStatusRailPresentation.from(" in activity
    assert "railPresentation.steps.forEach" in activity
    assert ".fillMaxWidth()" in activity
    assert "contentDescription = \"${step.label}，$state\"" in activity
    status_step = activity.split("private fun StatusRailStep(", 1)[1].split(
        "    @Composable",
        1,
    )[0]
    assert "Modifier.weight(1f)" not in status_step


def test_windows_converter_keeps_status_visible_and_accessible():
    xaml = read("apps/mh3g-save-converter-windows/MainWindow.xaml")

    assert "StatusText" in xaml
    assert "AutomationProperties.LiveSetting=\"Polite\"" in xaml
    assert "Copy.StageInspect" in xaml
    assert "Copy.StageDryRun" in xaml
    assert "Copy.StageWrite" in xaml
