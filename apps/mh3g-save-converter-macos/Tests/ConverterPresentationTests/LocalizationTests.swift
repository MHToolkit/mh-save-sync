import Foundation
import XCTest
@testable import ConverterPresentation

final class LocalizationTests: XCTestCase {
    func testStringCatalogContainsChineseAndEnglish() throws {
        let data = try Data(contentsOf: stringCatalogURL)
        let root = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        let strings = try XCTUnwrap(root["strings"] as? [String: Any])
        for key in [
            "Navigation.Input",
            "Navigation.Components",
            "Navigation.DryRun",
            "Navigation.WriteRollback",
            "Navigation.History",
            "Navigation.ExperimentalCEC",
            "Navigation.Settings",
            "Status.NotReady",
            "Status.Authorized",
            "Status.Running",
            "Status.Succeeded",
            "Status.Failed",
            "Write.SourceSHA256",
            "Write.TargetSHA256",
            "Write.StagingSetSHA256",
            "Write.TargetSetSHA256",
            "CEC.SourceRecords",
            "CEC.SourceRecordSetSHA256",
            "CEC.TargetSHA256",
            "WorkflowState.Input",
            "WorkflowState.ComponentSelection",
            "WorkflowState.DryRun",
            "WorkflowState.Writing",
            "WorkflowState.Success",
            "WorkflowState.Failure",
            "Guide.InputComplete",
            "Guide.ComponentsReady",
            "Guide.OptionalDataNeedsConfiguration",
            "Guide.OptionalDataReadyForTransaction",
            "Guide.DryRunComplete",
            "Guide.CoreDryRunCompleteWithOptionals",
            "Guide.SelectedWorkPending",
            "Guide.WriteComplete",
            "Guide.ToComponents",
            "Guide.ToDryRun",
            "Guide.ToWrite",
            "Guide.ToWriteAndOptionals",
            "Guide.ToHistory",
            "Guide.NextStep",
            "Repair.Version",
            "Repair.Version.Auto",
            "Repair.Version.Hint",
            "Repair.Version.Required",
            "Repair.PreviewSHA256",
        ] {
            let entry = try XCTUnwrap(strings[key] as? [String: Any], "missing \(key)")
            let localizations = try XCTUnwrap(entry["localizations"] as? [String: Any])
            XCTAssertNotNil(localizations["zh-Hans"], "\(key) needs zh-Hans")
            XCTAssertNotNil(localizations["en"], "\(key) needs English")
        }
    }

    func testWorkflowPhaseLabelsAreAccessible() {
        for phase in ConverterNavigation.allCases {
            XCTAssertFalse(ConverterCopy.text(phase.titleKey, language: .zhHans).isEmpty)
            XCTAssertFalse(ConverterCopy.text(phase.titleKey, language: .english).isEmpty)
            XCTAssertFalse(phase.accessibilityIdentifier.isEmpty)
        }
    }

    func testGuidedRouteKeepsThePrimaryConversionOrderVisible() {
        XCTAssertEqual(ConverterNavigation.guidedSuccessor(after: .componentSelection), .components)
        XCTAssertEqual(ConverterNavigation.guidedSuccessor(after: .dryRun), .writeRollback)
        XCTAssertEqual(ConverterNavigation.guidedSuccessor(after: .success), .history)
        XCTAssertNil(ConverterNavigation.guidedSuccessor(after: .failure))
    }

    func testAllVisibleCopyHasChineseAndEnglishValues() {
        XCTAssertFalse(ConverterCopy.visibleKeys.isEmpty)
        for key in ConverterCopy.visibleKeys {
            XCTAssertNotEqual(ConverterCopy.text(key, language: .zhHans), key, "missing zh-Hans value for \(key)")
            XCTAssertNotEqual(ConverterCopy.text(key, language: .english), key, "missing English value for \(key)")
        }
    }

    func testSafetyStatusAndConfirmationCopyHasBothLanguages() {
        for key in [
            "Status.NotReady",
            "Status.Authorized",
            "Status.Running",
            "Status.Succeeded",
            "Status.Failed",
            "Write.SourceSHA256",
            "Write.TargetSHA256",
            "Write.StagingSetSHA256",
            "Write.TargetSetSHA256",
            "CEC.SourceRecordSetSHA256",
            "CEC.TargetSHA256",
            "CEC.SourceRecords",
        ] {
            XCTAssertNotEqual(ConverterCopy.text(key, language: .zhHans), key, "missing zh-Hans value for \(key)")
            XCTAssertNotEqual(ConverterCopy.text(key, language: .english), key, "missing English value for \(key)")
        }
    }

    func testWorkflowStatesUseLocalizedDisplayKeysInsteadOfRawValues() {
        let expectedChinese: [(WorkflowState, String)] = [
            (.input, "等待选择存档"),
            (.componentSelection, "已完成检查"),
            (.dryRun, "Dry Run 已完成"),
            (.writing, "正在执行"),
            (.success, "操作完成"),
            (.failure, "操作失败"),
        ]

        for (state, chinese) in expectedChinese {
            XCTAssertNotEqual(state.localizationKey, state.rawValue)
            XCTAssertEqual(
                ConverterCopy.text(state.localizationKey, language: .zhHans),
                chinese
            )
            XCTAssertNotEqual(
                ConverterCopy.text(state.localizationKey, language: .english),
                state.rawValue
            )
        }
    }

    @MainActor
    func testLocaleOverrideChangesDisplayedLocale() {
        let suite = "MH3GSaveConverterMacTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = ConverterLocaleSettings(defaults: defaults)

        XCTAssertEqual(settings.resolvedLanguage(systemIdentifier: "zh-Hans-CN"), .zhHans)
        settings.override = .english
        XCTAssertEqual(settings.resolvedLanguage(systemIdentifier: "zh-Hans-CN"), .english)
        XCTAssertEqual(ConverterCopy.text("Navigation.Input", language: settings.resolvedLanguage(systemIdentifier: "zh-Hans-CN")), "Input & Inspect")
    }
}

private var stringCatalogURL: URL {
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .appendingPathComponent("Sources/MH3GSaveConverterMac/Resources/Localizable.xcstrings")
}
