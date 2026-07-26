import SwiftUI
import ConverterPresentation

struct WriteRollbackView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @State private var showConfirmation = false
    @State private var manifest: URL?
    @State private var isRunning = false

    private var hasDeferredOptionalWrites: Bool {
        workflow.components.includeSystem || !workflow.components.selectedGroups.isEmpty || workflow.components.includesCEC
    }

    var body: some View {
        WorkbenchPage(
            artwork: .rollbackHarbor,
            title: ConverterCopy.text("Navigation.WriteRollback", language: language),
            subtitle: ConverterCopy.text("Write.Subtitle", language: language)
        ) {
            Form {
                Section {
                    LabeledContent(ConverterCopy.text("Write.Authorization", language: language)) {
                        Label(
                            workflow.canWrite ? ConverterCopy.text("Write.CurrentAuthorized", language: language) : ConverterCopy.text("Write.Unavailable", language: language),
                            systemImage: workflow.canWrite ? "checkmark.shield.fill" : "lock.fill"
                        )
                        .foregroundStyle(workflow.canWrite ? .green : .secondary)
                    }
                    if hasDeferredOptionalWrites {
                        Label(ConverterCopy.text("Write.OptionalDeferred", language: language), systemImage: "info.circle")
                            .foregroundStyle(.secondary)
                    }
                    Button(ConverterCopy.text("Write.Confirm", language: language)) {
                        showConfirmation = true
                    }
                    .disabled(!workflow.canWrite || hasDeferredOptionalWrites || isRunning)
                } footer: {
                    Text(ConverterCopy.text("Write.Footer", language: language))
                }

                Section {
                    SelectedPathRow(
                        title: ConverterCopy.text("Write.Manifest", language: language),
                        value: manifest,
                        chooseTitle: ConverterCopy.text("Input.Select", language: language)
                    ) {
                        manifest = OpenPanel.selectFile(
                            title: ConverterCopy.text("Write.SelectManifest", language: language),
                            message: ConverterCopy.text("Write.SelectManifestMessage", language: language)
                        )
                    }
                    Button(ConverterCopy.text("Write.Rollback", language: language)) {
                        rollback()
                    }
                    .disabled(manifest == nil || isRunning)
                } footer: {
                    Text(ConverterCopy.text("Write.RollbackFooter", language: language))
                }

                if let failure = workflow.failure {
                    Section {
                        FailureDetails(failure: failure, language: language)
                    }
                }
            }
            .formStyle(.grouped)
        }
        .sheet(isPresented: $showConfirmation) {
            TransactionConfirmationSheet(
                workflow: workflow,
                language: language,
                onConfirm: writeCore,
                onCancel: { showConfirmation = false }
            )
        }
    }

    private func writeCore() {
        showConfirmation = false
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.writeCore()
        }
    }

    private func rollback() {
        guard let manifest else { return }
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.rollback(manifest: manifest)
        }
    }
}

private struct TransactionConfirmationSheet: View {
    let workflow: ConversionWorkflow
    let language: ConverterLanguage
    let onConfirm: () -> Void
    let onCancel: () -> Void

    private var target: URL? { workflow.input?.target }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Image(systemName: "externaldrive.badge.checkmark")
                    .font(.title2)
                    .foregroundStyle(.tint)
                Text(ConverterCopy.text("Write.Confirm", language: language))
                    .font(.title3.weight(.semibold))
            }
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 10) {
                GridRow {
                    Text(ConverterCopy.text("Write.Files", language: language))
                        .foregroundStyle(.secondary)
                    Text(ConverterCopy.text("Write.OneTarget", language: language))
                }
                GridRow {
                    Text(ConverterCopy.text("Write.Target", language: language))
                        .foregroundStyle(.secondary)
                    Text(target?.path ?? "—")
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
                GridRow {
                    Text(ConverterCopy.text("Write.Backup", language: language))
                        .foregroundStyle(.secondary)
                    Text(ConverterCopy.text("Write.OneBackup", language: language))
                }
                GridRow {
                    Text(ConverterCopy.text("Write.Manifest", language: language))
                        .foregroundStyle(.secondary)
                    Text(ConverterCopy.text("Write.ManifestCreated", language: language))
                }
                GridRow {
                    Text(ConverterCopy.text("Write.ExperimentalCEC", language: language))
                        .foregroundStyle(.secondary)
                    Text(ConverterCopy.text("Write.NotIncluded", language: language))
                }
            }
            Spacer(minLength: 0)
            HStack {
                Button(ConverterCopy.text("Write.Cancel", language: language), action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button(ConverterCopy.text("Write.SelectedSave", language: language), action: onConfirm)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 570, height: 350)
    }
}
