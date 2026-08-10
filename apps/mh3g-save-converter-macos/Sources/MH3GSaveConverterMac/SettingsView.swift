import SwiftUI
import ConverterPresentation

struct SettingsView: View {
    @Binding var localeOverride: String
    @Bindable var workflow: ConversionWorkflow
    @ObservedObject var updateChecker: GitHubUpdateChecker
    let language: ConverterLanguage

    var body: some View {
        WorkbenchPage(
            artwork: .componentsWorkshop,
            title: ConverterCopy.text("Navigation.Settings", language: language),
            subtitle: ConverterCopy.text("Settings.Subtitle", language: language)
        ) {
            Form {
                Section {
                    Picker(ConverterCopy.text("Settings.Language", language: language), selection: $localeOverride) {
                        Text(ConverterCopy.text("Settings.Language.System", language: language)).tag(ConverterLanguage.system.rawValue)
                        Text(ConverterCopy.text("Settings.Language.Chinese", language: language)).tag(ConverterLanguage.zhHans.rawValue)
                        Text(ConverterCopy.text("Settings.Language.English", language: language)).tag(ConverterLanguage.english.rawValue)
                    }
                    .pickerStyle(.menu)
                }

                Section(ConverterCopy.text("Settings.Diagnostics", language: language)) {
                    LabeledContent(ConverterCopy.text("Settings.UI", language: language)) {
                        Text(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ConverterCopy.text("Settings.Development", language: language))
                            .font(.caption.monospaced())
                    }
                    LabeledContent(ConverterCopy.text("Settings.CLI", language: language)) {
                        Text(ProcessInfo.processInfo.environment["MH3G_CONVERTER_CLI"] ?? ConverterCopy.text("Settings.BundledSidecar", language: language))
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                    }
                    LabeledContent(
                        ConverterCopy.text("Settings.WorkflowState", language: language),
                        value: ConverterCopy.text(workflow.state.localizationKey, language: language)
                    )
                }

                Section(ConverterCopy.text("Update.About", language: language)) {
                    LabeledContent(ConverterCopy.text("Update.CurrentVersion", language: language)) {
                        Text(updateChecker.currentVersion)
                            .font(.caption.monospaced())
                    }

                    HStack(spacing: 10) {
                        Button(ConverterCopy.text("Update.Check", language: language)) {
                            Task { await updateChecker.checkManually() }
                        }
                        .disabled(updateChecker.isChecking)

                        if updateChecker.isChecking {
                            ProgressView()
                                .controlSize(.small)
                            Text(ConverterCopy.text("Update.Checking", language: language))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }

                    updateStatus

                    Text(ConverterCopy.text("Update.NetworkNote", language: language))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .formStyle(.grouped)
        }
    }

    @ViewBuilder
    private var updateStatus: some View {
        switch updateChecker.status {
        case .idle, .checking:
            EmptyView()
        case let .upToDate(latest):
            Label(
                String(format: ConverterCopy.text("Update.UpToDate", language: language), latest),
                systemImage: "checkmark.circle.fill"
            )
            .foregroundStyle(.green)
        case let .updateAvailable(latest):
            Label(
                String(format: ConverterCopy.text("Update.Available", language: language), latest),
                systemImage: "arrow.down.circle.fill"
            )
            .foregroundStyle(.tint)
        case let .failed(detail):
            VStack(alignment: .leading, spacing: 4) {
                Label(ConverterCopy.text("Update.Failed", language: language), systemImage: "wifi.exclamationmark")
                    .foregroundStyle(.orange)
                Text(detail)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }
    }
}
