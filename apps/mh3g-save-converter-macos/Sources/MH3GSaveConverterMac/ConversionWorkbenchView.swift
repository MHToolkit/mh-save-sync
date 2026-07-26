import SwiftUI
import ConverterPresentation

struct ConversionWorkbenchView: View {
    @Binding var localeOverride: String
    @State private var selectedNavigation: ConverterNavigation? = .input
    @State private var workflow = ConversionWorkflow(executable: ConverterExecutableLocator.locate())
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var language: ConverterLanguage {
        let override = ConverterLanguage(rawValue: localeOverride) ?? .system
        return override == .system
            ? (Locale.current.identifier.lowercased().hasPrefix("zh") ? .zhHans : .english)
            : override
    }

    var body: some View {
        NavigationSplitView {
            List(ConverterNavigation.allCases, selection: $selectedNavigation) { item in
                Label(ConverterCopy.text(item.titleKey, language: language), systemImage: item.systemImage)
                    .tag(item)
                    .accessibilityIdentifier(item.accessibilityIdentifier)
            }
            .navigationTitle(ConverterCopy.text("App.Title", language: language))
            .listStyle(.sidebar)
        } detail: {
            detailView
                .frame(minWidth: 680, minHeight: 560)
        }
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 8) {
                    Image(systemName: "arrow.left.arrow.right.circle.fill")
                        .foregroundStyle(.tint)
                    Text(ConverterCopy.text("App.Title", language: language))
                        .font(.headline)
                }
            }
            ToolbarItem(placement: .automatic) {
                StatusPill(state: workflow.state, language: language)
            }
        }
        .onChange(of: selectedNavigation) { _, _ in
            guard !reduceMotion else { return }
            // Navigation is controlled by native split-view selection.  The
            // content changes are brief and do not disable any other control.
        }
    }

    @ViewBuilder
    private var detailView: some View {
        switch selectedNavigation ?? .input {
        case .input:
            InputInspectionView(workflow: workflow, language: language)
        case .components:
            ComponentSelectionView(workflow: workflow, language: language)
        case .dryRun:
            DryRunView(workflow: workflow, language: language)
        case .writeRollback:
            WriteRollbackView(workflow: workflow, language: language)
        case .history:
            ConversionHistoryView(workflow: workflow, language: language)
        case .experimentalCEC:
            ExperimentalCECView(workflow: workflow, language: language)
        case .settings:
            SettingsView(localeOverride: $localeOverride, workflow: workflow, language: language)
        }
    }
}

enum ConverterExecutableLocator {
    static func locate() -> URL {
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents/MacOS/mh3g-save-convert")
        if FileManager.default.isExecutableFile(atPath: bundled.path) {
            return bundled
        }
        if let configured = ProcessInfo.processInfo.environment["MH3G_CONVERTER_CLI"], !configured.isEmpty {
            return URL(fileURLWithPath: configured)
        }
        return URL(fileURLWithPath: "/usr/local/bin/mh3g-save-convert")
    }
}

private struct StatusPill: View {
    let state: WorkflowState
    let language: ConverterLanguage

    var body: some View {
        Label(title, systemImage: image)
            .labelStyle(.titleAndIcon)
            .font(.caption)
            .foregroundStyle(color)
            .accessibilityLabel(title)
    }

    private var title: String {
        switch state {
        case .writing: ConverterCopy.text("Status.Running", language: language)
        case .failure: ConverterCopy.text("Status.Failed", language: language)
        default: ConverterCopy.text("Status.Ready", language: language)
        }
    }

    private var image: String {
        switch state {
        case .writing: "arrow.triangle.2.circlepath"
        case .failure: "exclamationmark.triangle.fill"
        default: "checkmark.circle.fill"
        }
    }

    private var color: Color {
        switch state {
        case .failure: .red
        case .writing: .orange
        default: .green
        }
    }
}
