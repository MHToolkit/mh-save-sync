import SwiftUI
import ConverterPresentation

struct ExperimentalCECView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @State private var expanded = false
    @State private var running = false
    @State private var manifest: URL?
    @State private var showWriteConfirmation = false

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

                Section {
                    Button(ConverterCopy.text("CEC.DryRun", language: language)) {
                        runDryRun()
                    }
                    .disabled(!hasPaths || running)

                    if workflow.canWriteCEC, let fingerprint = workflow.cecDryRunFingerprint {
                        Label(ConverterCopy.text("CEC.Authorized", language: language), systemImage: "checkmark.shield.fill")
                            .foregroundStyle(.green)
                        Text(fingerprint.targetSHA256Before)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                        Button(ConverterCopy.text("CEC.Write", language: language)) {
                            showWriteConfirmation = true
                        }
                        .disabled(running)
                    }
                }

                Section {
                    SelectedPathRow(
                        title: ConverterCopy.text("CEC.Manifest", language: language),
                        value: manifest,
                        chooseTitle: ConverterCopy.text("Input.Select", language: language)
                    ) {
                        manifest = OpenPanel.selectFile(
                            title: ConverterCopy.text("CEC.Manifest", language: language),
                            message: ConverterCopy.text("Write.SelectManifestMessage", language: language)
                        )
                    }
                    Button(ConverterCopy.text("CEC.Rollback", language: language)) {
                        rollback()
                    }
                    .disabled(manifest == nil || running)
                }

                if let failure = workflow.failure, failure.operation == .convertCEC || failure.operation == .rollbackCEC {
                    Section {
                        FailureDetails(failure: failure, language: language)
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(workflow.activeOperation != nil)
        }
        .sheet(isPresented: $showWriteConfirmation) {
            TransactionConfirmationSheet(
                title: ConverterCopy.text("CEC.ConfirmTitle", language: language),
                targetLabel: ConverterCopy.text("CEC.Target", language: language),
                target: workflow.cecDryRunFingerprint?.target ?? workflow.components.cecTarget,
                files: ConverterCopy.text("CEC.SourceRecords", language: language),
                language: language,
                verificationDetails: cecConfirmationDetails,
                confirmationRole: .destructive,
                onConfirm: confirmWriteCEC,
                onCancel: { showWriteConfirmation = false }
            )
        }
    }

    private var hasPaths: Bool {
        workflow.components.cecSourceDirectory != nil && workflow.components.cecTarget != nil
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

    private var cecConfirmationDetails: [TransactionConfirmationDetail] {
        guard let fingerprint = workflow.cecDryRunFingerprint else { return [] }
        return [
            TransactionConfirmationDetail(
                label: ConverterCopy.text("CEC.SourceRecordSetSHA256", language: language),
                value: fingerprint.sourceRecordSetSHA256
            ),
            TransactionConfirmationDetail(
                label: ConverterCopy.text("CEC.TargetSHA256", language: language),
                value: fingerprint.targetSHA256Before
            ),
        ]
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

    private func runDryRun() {
        running = true
        Task {
            defer { running = false }
            try? await workflow.runCECDryRun()
        }
    }

    private func writeCEC() {
        running = true
        Task {
            defer { running = false }
            try? await workflow.writeCEC()
            if let path = workflow.latestReport?.manifest {
                manifest = URL(fileURLWithPath: path)
            }
        }
    }

    private func confirmWriteCEC() {
        showWriteConfirmation = false
        writeCEC()
    }

    private func rollback() {
        guard let manifest else { return }
        running = true
        Task {
            defer { running = false }
            try? await workflow.rollback(manifest: manifest, cec: true)
        }
    }
}
