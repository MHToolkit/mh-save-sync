import SwiftUI
import ConverterPresentation

struct ExperimentalCECView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @State private var expanded = false

    var body: some View {
        WorkbenchPage(
            artwork: .cecMailbox,
            title: ConverterCopy.text("Navigation.ExperimentalCEC", language: language),
            subtitle: ConverterCopy.text("CEC.Hidden", language: language)
        ) {
            Form {
                Section {
                    DisclosureGroup(isExpanded: $expanded) {
                        VStack(alignment: .leading, spacing: 12) {
                            SelectedPathRow(
                                title: ConverterCopy.text("CEC.Source", language: language),
                                value: workflow.components.cecSourceDirectory,
                                chooseTitle: ConverterCopy.text("Input.Select", language: language)
                            ) {
                                chooseSource()
                            }
                            SelectedPathRow(
                                title: ConverterCopy.text("CEC.Target", language: language),
                                value: workflow.components.cecTarget,
                                chooseTitle: ConverterCopy.text("Input.Select", language: language)
                            ) {
                                chooseTarget()
                            }
                            Toggle(
                                ConverterCopy.text("CEC.Acknowledge", language: language),
                                isOn: acknowledgementBinding
                            )
                            .disabled(workflow.components.cecSourceDirectory == nil || workflow.components.cecTarget == nil)
                            Label(ConverterCopy.text("CEC.Warning", language: language), systemImage: "exclamationmark.triangle")
                                .foregroundStyle(.orange)
                                .font(.caption)
                        }
                        .padding(.top, 8)
                    } label: {
                        Label(ConverterCopy.text("CEC.Disclosure", language: language), systemImage: "envelope.badge")
                    }
                } footer: {
                    Text(ConverterCopy.text("CEC.Footer", language: language))
                }
            }
            .formStyle(.grouped)
        }
    }

    private var acknowledgementBinding: Binding<Bool> {
        Binding(
            get: { workflow.components.acknowledgeExperimentalCEC },
            set: { value in
                var next = workflow.components
                next.acknowledgeExperimentalCEC = value
                workflow.setComponents(next)
            }
        )
    }

    private func chooseSource() {
        guard let source = OpenPanel.selectDirectory(
            title: ConverterCopy.text("CEC.Source", language: language),
            message: ConverterCopy.text("CEC.SourceMessage", language: language)
        ) else { return }
        update { $0.cecSourceDirectory = source }
    }

    private func chooseTarget() {
        guard let target = OpenPanel.selectFile(
            title: ConverterCopy.text("CEC.Target", language: language),
            message: ConverterCopy.text("CEC.TargetMessage", language: language)
        ) else { return }
        update { $0.cecTarget = target }
    }

    private func update(_ change: (inout ComponentSelection) -> Void) {
        var next = workflow.components
        change(&next)
        workflow.setComponents(next)
    }
}
