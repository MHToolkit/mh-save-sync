import SwiftUI
import ConverterPresentation

struct DryRunView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @State private var running = false

    var body: some View {
        WorkbenchPage(
            artwork: .dryRunFlow,
            title: ConverterCopy.text("Navigation.DryRun", language: language),
            subtitle: ConverterCopy.text("DryRun.NotAuthorized", language: language)
        ) {
            Form {
                Section {
                    DryRunFlowRow(
                        title: ConverterCopy.text("Input.Source", language: language),
                        path: workflow.input?.source.path,
                        state: workflow.sourceInspection == nil ? .pending : .ready
                    )
                    DryRunFlowRow(
                        title: ConverterCopy.text("Input.Target", language: language),
                        path: workflow.input?.target.path,
                        state: workflow.targetInspection == nil ? .pending : .ready
                    )
                    DryRunFlowRow(
                        title: ConverterCopy.text("Navigation.Components", language: language),
                        path: selectedComponentSummary,
                        state: workflow.components.selectedGroups.isEmpty && !workflow.components.includeSystem ? .notSelected : .ready
                    )
                    DryRunFlowRow(
                        title: ConverterCopy.text("DryRun.BackupManifest", language: language),
                        path: workflow.canWrite
                            ? ConverterCopy.text("DryRun.BackupPending", language: language)
                            : ConverterCopy.text("DryRun.BackupAvailable", language: language),
                        state: workflow.canWrite ? .ready : .pending
                    )
                } header: {
                    Text(ConverterCopy.text("DryRun.ReadOnly", language: language))
                }

                Section {
                    HStack {
                        Button(ConverterCopy.text("DryRun.Start", language: language)) {
                            runDryRun()
                        }
                        .keyboardShortcut("r", modifiers: [.command])
                        .disabled(!workflow.canStartDryRun || running)
                        if running {
                            ProgressView()
                                .controlSize(.small)
                        }
                    }
                    if workflow.canWrite, let fingerprint = workflow.dryRunFingerprint {
                        Label(ConverterCopy.text("DryRun.Authorized", language: language), systemImage: "checkmark.shield.fill")
                            .foregroundStyle(.green)
                        HashRows(fingerprint: fingerprint, language: language)
                    } else {
                        Text(ConverterCopy.text("DryRun.NotAuthorized", language: language))
                            .foregroundStyle(.secondary)
                    }
                }

                if workflow.failure?.operation == .convert, let failure = workflow.failure {
                    Section {
                        FailureDetails(failure: failure, language: language)
                    }
                }
            }
            .formStyle(.grouped)
        }
    }

    private var selectedComponentSummary: String {
        var values = [String]()
        if workflow.components.includeSystem { values.append(ConverterCopy.text("Components.System", language: language)) }
        values.append(contentsOf: workflow.components.selectedGroups.sorted { $0.rawValue < $1.rawValue }.map { group in
            switch group {
            case .guildCards:
                ConverterCopy.text("Components.GuildCards", language: language)
            case .quests:
                ConverterCopy.text("Components.Quests", language: language)
            }
        })
        if workflow.components.includesCEC { values.append("CEC") }
        return values.isEmpty ? ConverterCopy.text("DryRun.CoreOnly", language: language) : values.joined(separator: " · ")
    }

    private func runDryRun() {
        running = true
        Task {
            defer { running = false }
            try? await workflow.runCoreDryRun()
        }
    }
}

private struct HashRows: View {
    let fingerprint: DryRunFingerprint
    let language: ConverterLanguage

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
            GridRow {
                Text(ConverterCopy.text("DryRun.Source", language: language)).foregroundStyle(.secondary)
                Text(fingerprint.sourceSHA256).font(.caption.monospaced()).textSelection(.enabled)
            }
            GridRow {
                Text(ConverterCopy.text("DryRun.Target", language: language)).foregroundStyle(.secondary)
                Text(fingerprint.targetSHA256).font(.caption.monospaced()).textSelection(.enabled)
            }
        }
    }
}

private struct DryRunFlowRow: View {
    enum State {
        case pending
        case ready
        case notSelected
    }

    let title: String
    let path: String?
    let state: State

    var body: some View {
        LabeledContent(title) {
            HStack(spacing: 7) {
                Image(systemName: symbol)
                    .foregroundStyle(color)
                Text(path ?? "—")
                    .font(.caption.monospaced())
                    .foregroundStyle(state == .notSelected ? .secondary : .primary)
                    .lineLimit(2)
                    .textSelection(.enabled)
            }
        }
    }

    private var symbol: String {
        switch state {
        case .pending: "circle.dotted"
        case .ready: "checkmark.circle.fill"
        case .notSelected: "minus.circle"
        }
    }

    private var color: Color {
        switch state {
        case .pending: .secondary
        case .ready: .green
        case .notSelected: .secondary
        }
    }
}
