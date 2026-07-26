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

    func testAllVisibleCopyHasChineseAndEnglishValues() {
        XCTAssertFalse(ConverterCopy.visibleKeys.isEmpty)
        for key in ConverterCopy.visibleKeys {
            XCTAssertNotEqual(ConverterCopy.text(key, language: .zhHans), key, "missing zh-Hans value for \(key)")
            XCTAssertNotEqual(ConverterCopy.text(key, language: .english), key, "missing English value for \(key)")
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
