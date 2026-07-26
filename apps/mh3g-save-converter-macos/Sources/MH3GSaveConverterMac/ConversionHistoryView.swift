import SwiftUI
import ConverterPresentation

struct ConversionHistoryView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage

    var body: some View {
        WorkbenchPage(
            artwork: .rollbackHarbor,
            title: ConverterCopy.text("Navigation.History", language: language),
            subtitle: ConverterCopy.text("History.Subtitle", language: language)
        ) {
            if let report = workflow.latestReport {
                Form {
                    Section {
                        LabeledContent(ConverterCopy.text("History.Operation", language: language), value: report.operation ?? "—")
                        LabeledContent(ConverterCopy.text("History.Status", language: language), value: report.status ?? "—")
                        if let backup = report.backup {
                            LabeledContent(ConverterCopy.text("History.Backup", language: language), value: backup)
                        }
                        if let manifest = report.manifest {
                            LabeledContent(ConverterCopy.text("History.Manifest", language: language), value: manifest)
                        }
                    }
                }
                .formStyle(.grouped)
            } else {
                ContentUnavailableView(
                    ConverterCopy.text("History.Empty", language: language),
                    systemImage: "clock.arrow.circlepath",
                    description: Text(ConverterCopy.text("History.EmptyDescription", language: language))
                )
                .frame(maxWidth: .infinity, minHeight: 260)
            }
        }
    }
}
