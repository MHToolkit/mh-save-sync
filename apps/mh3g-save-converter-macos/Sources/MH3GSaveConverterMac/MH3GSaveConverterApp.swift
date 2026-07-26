import SwiftUI
import ConverterPresentation
import Foundation
import Darwin
import AppKit

@main
struct MH3GSaveConverterMac: App {
    @AppStorage(ConverterLocaleSettings.storageKey) private var localeOverride = ConverterLanguage.system.rawValue

    init() {
        if CommandLine.arguments.contains("--diagnostics") {
            print(AppDiagnostics.json())
            exit(0)
        }
        if CommandLine.arguments.contains("--window-smoke") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) {
                let visibleWindowCount = NSApp.windows.filter { $0.isVisible }.count
                let payload = ["visible_window_count": visibleWindowCount]
                let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
                print(data.flatMap { String(data: $0, encoding: .utf8) } ?? "{}")
                NSApp.terminate(nil)
            }
        }
    }

    private var selectedLanguage: ConverterLanguage {
        ConverterLanguage(rawValue: localeOverride) ?? .system
    }

    private var displayLocale: Locale {
        switch selectedLanguage {
        case .system:
            .current
        case .zhHans:
            Locale(identifier: "zh-Hans")
        case .english:
            Locale(identifier: "en")
        }
    }

    var body: some Scene {
        WindowGroup {
            ConversionWorkbenchView(localeOverride: $localeOverride)
                .environment(\.locale, displayLocale)
        }
        .defaultSize(width: 1160, height: 780)
        .commands {
            CommandGroup(replacing: .newItem) { }
        }
    }
}

private enum AppDiagnostics {
    static func json() -> String {
        let cli = ConverterExecutableLocator.locate()
        let payload: [String: String] = [
            "ui_version": Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "development",
            "bundled_cli": cli.path,
            "cli_version": cliVersion(at: cli) ?? "unavailable",
        ]
        let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        return data.flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }

    private static func cliVersion(at url: URL) -> String? {
        guard FileManager.default.isExecutableFile(atPath: url.path) else { return nil }
        let process = Process()
        let output = Pipe()
        process.executableURL = url
        process.arguments = ["--version"]
        process.standardOutput = output
        guard (try? process.run()) != nil else { return nil }
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        return String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
