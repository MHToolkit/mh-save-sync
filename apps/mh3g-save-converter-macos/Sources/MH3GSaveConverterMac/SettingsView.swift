import SwiftUI
import ConverterPresentation

struct SettingsView: View {
    @Binding var localeOverride: String
    @Bindable var workflow: ConversionWorkflow
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
                    LabeledContent(ConverterCopy.text("Settings.WorkflowState", language: language), value: workflow.state.rawValue)
                }
            }
            .formStyle(.grouped)
        }
    }
}
