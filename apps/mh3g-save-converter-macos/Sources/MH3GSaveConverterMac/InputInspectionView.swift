import SwiftUI
import ConverterPresentation

struct InputInspectionView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @State private var source: URL?
    @State private var target: URL?
    @State private var selectionError: String?
    @State private var isInspecting = false

    var body: some View {
        WorkbenchPage(
            artwork: .inputRoute,
            title: ConverterCopy.text("Navigation.Input", language: language),
            subtitle: ConverterCopy.text("DryRun.NotAuthorized", language: language)
        ) {
            Form {
                Section {
                    SelectedPathRow(
                        title: ConverterCopy.text("Input.Source", language: language),
                        value: source ?? workflow.input?.source,
                        chooseTitle: ConverterCopy.text("Input.Select", language: language)
                    ) {
                        chooseSource()
                    }
                    SelectedPathRow(
                        title: ConverterCopy.text("Input.Target", language: language),
                        value: target ?? workflow.input?.target,
                        chooseTitle: ConverterCopy.text("Input.Select", language: language)
                    ) {
                        chooseTarget()
                    }
                    HStack {
                        Button(ConverterCopy.text("Input.Inspect", language: language)) {
                            inspect()
                        }
                        .keyboardShortcut(.defaultAction)
                        .disabled(!hasInput || isInspecting || workflow.activeOperation != nil)
                        if isInspecting {
                            ProgressView()
                                .controlSize(.small)
                        }
                    }
                }

                if let sourceInspection = workflow.sourceInspection, let targetInspection = workflow.targetInspection {
                    Section(ConverterCopy.text("Input.SHA256", language: language)) {
                        InspectionTable(
                            source: sourceInspection,
                            target: targetInspection,
                            language: language
                        )
                    }
                }

                if let selectionError {
                    Section {
                        Label(selectionError, systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                    }
                }

                if workflow.failure?.operation == .inspect, let failure = workflow.failure {
                    Section {
                        FailureDetails(failure: failure, language: language)
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(workflow.activeOperation != nil)
        }
        .onAppear {
            source = workflow.input?.source
            target = workflow.input?.target
        }
    }

    private var hasInput: Bool { (source ?? workflow.input?.source) != nil && (target ?? workflow.input?.target) != nil }

    private func chooseSource() {
        guard let url = OpenPanel.selectFile(
            title: ConverterCopy.text("Input.Source", language: language),
            message: ConverterCopy.text("Input.SourceMessage", language: language)
        ) else { return }
        guard ["user1", "user2", "user3"].contains(url.lastPathComponent.lowercased()) else {
            selectionError = ConverterCopy.text("Input.InvalidSlot", language: language)
            return
        }
        source = url
        selectionError = nil
        updateInput()
    }

    private func chooseTarget() {
        guard let url = OpenPanel.selectFile(
            title: ConverterCopy.text("Input.Target", language: language),
            message: ConverterCopy.text("Input.TargetMessage", language: language)
        ) else { return }
        guard ["user1", "user2", "user3"].contains(url.lastPathComponent.lowercased()) else {
            selectionError = ConverterCopy.text("Input.InvalidSlot", language: language)
            return
        }
        target = url
        selectionError = nil
        updateInput()
    }

    private func updateInput() {
        guard let source, let target else { return }
        workflow.configure(input: ConversionInput(source: source, target: target))
    }

    private func inspect() {
        updateInput()
        isInspecting = true
        Task {
            defer { isInspecting = false }
            do {
                try await workflow.inspectInputs()
                selectionError = nil
            } catch {
                selectionError = error.localizedDescription
            }
        }
    }
}

struct WorkbenchPage<Content: View>: View {
    let artwork: ConverterArtwork
    let title: String
    let subtitle: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ZStack(alignment: .bottomLeading) {
                    SceneArtworkView(artwork: artwork)
                        .frame(height: 230)
                        .opacity(0.94)
                    VStack(alignment: .leading, spacing: 5) {
                        Text(title)
                            .font(.title2.weight(.semibold))
                        Text(subtitle)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                    .padding(20)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(.regularMaterial)
                }
                content()
                    .padding(24)
                    .frame(maxWidth: 920, alignment: .leading)
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

struct SelectedPathRow: View {
    let title: String
    let value: URL?
    let chooseTitle: String
    let choose: () -> Void

    var body: some View {
        LabeledContent(title) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(value?.path ?? "—")
                    .font(.caption.monospaced())
                    .foregroundStyle(value == nil ? .secondary : .primary)
                    .lineLimit(2)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                Button(chooseTitle, action: choose)
            }
        }
    }
}

struct InspectionTable: View {
    let source: InputInspection
    let target: InputInspection
    let language: ConverterLanguage

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 9) {
            GridRow {
                Text("")
                Text(ConverterCopy.text("Input.Source", language: language)).font(.caption).foregroundStyle(.secondary)
                Text(ConverterCopy.text("Input.Target", language: language)).font(.caption).foregroundStyle(.secondary)
            }
            GridRow {
                Text(ConverterCopy.text("Input.Profile", language: language)).foregroundStyle(.secondary)
                Text(source.profile)
                Text(target.profile)
            }
            GridRow {
                Text(ConverterCopy.text("Input.Bytes", language: language)).foregroundStyle(.secondary)
                Text(source.size, format: .number)
                Text(target.size, format: .number)
            }
            GridRow {
                Text(ConverterCopy.text("Input.SHA256", language: language)).foregroundStyle(.secondary)
                Text(source.sha256).font(.caption.monospaced()).textSelection(.enabled)
                Text(target.sha256).font(.caption.monospaced()).textSelection(.enabled)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct FailureDetails: View {
    let failure: WorkflowFailure
    let language: ConverterLanguage

    var body: some View {
        DisclosureGroup(ConverterCopy.text("Error.Detail", language: language)) {
            VStack(alignment: .leading, spacing: 8) {
                Text(failure.message)
                if !failure.stderr.isEmpty {
                    Text(failure.stderr)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
            }
            .foregroundStyle(.red)
        }
    }
}
