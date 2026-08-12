import SwiftUI
import ConverterPresentation

struct InputInspectionView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @Binding var navigation: ConverterNavigation?
    @State private var slot: SaveSlot = .user2
    @State private var sourceSelection: URL?
    @State private var currentSelection: URL?
    @State private var targetSelection: URL?
    @State private var source: URL?
    @State private var current: URL?
    @State private var target: URL?
    @State private var selectionError: String?
    @State private var isInspecting = false

    var body: some View {
        WorkbenchPage(
            artwork: .inputRoute,
            title: ConverterCopy.text("Navigation.Input", language: language),
            subtitle: ConverterCopy.text("Input.Subtitle", language: language)
        ) {
            Form {
                Section {
                    Picker(ConverterCopy.text("Input.Mode", language: language), selection: Binding(
                        get: { workflow.mode },
                        set: {
                            workflow.setMode($0)
                            updateInput()
                        }
                    )) {
                        Text(ConverterCopy.text("Input.Mode.New", language: language))
                            .tag(ConversionMode.newConversion)
                        Text(ConverterCopy.text("Input.Mode.Repair", language: language))
                            .tag(ConversionMode.repairConverted)
                    }
                    .pickerStyle(.segmented)
                    if workflow.mode == .repairConverted {
                        Label(
                            ConverterCopy.text("Input.RepairHint", language: language),
                            systemImage: "wrench.and.screwdriver"
                        )
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                    Picker(ConverterCopy.text("Input.Slot", language: language), selection: $slot) {
                        ForEach(SaveSlot.allCases) { slot in
                            Text(slot.rawValue).tag(slot)
                        }
                    }
                    .pickerStyle(.segmented)
                    .onChange(of: slot) { _, _ in
                        resolveSelections()
                    }
                    CorePathSelectionGuide(language: language)
                    SelectedPathRow(
                        title: ConverterCopy.text("Input.Source", language: language),
                        value: source,
                        chooseTitle: ConverterCopy.text("Input.SelectFileOrDirectory", language: language),
                        selectionHint: ConverterCopy.text("Input.SourceSelectionHint", language: language)
                    ) {
                        chooseSource()
                    }
                    if workflow.mode == .repairConverted {
                        SelectedPathRow(
                            title: ConverterCopy.text("Input.Current", language: language),
                            value: current ?? workflow.input?.current,
                            chooseTitle: ConverterCopy.text("Input.SelectFileOrDirectory", language: language),
                            selectionHint: ConverterCopy.text("Input.CurrentSelectionHint", language: language)
                        ) {
                            chooseCurrent()
                        }
                        Label(
                            ConverterCopy.text("Input.CurrentReadOnly", language: language),
                            systemImage: "lock.shield"
                        )
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                    SelectedPathRow(
                        title: ConverterCopy.text(
                            workflow.mode == .repairConverted ? "Input.RepairOutput" : "Input.Target",
                            language: language
                        ),
                        value: target ?? workflow.input?.target,
                        chooseTitle: ConverterCopy.text("Input.SelectFileOrDirectory", language: language),
                        selectionHint: ConverterCopy.text("Input.OutputSelectionHint", language: language)
                    ) {
                        chooseTarget()
                    }
                    if let target {
                        LabeledContent(ConverterCopy.text("Input.FinalOutput", language: language)) {
                            HStack(spacing: 6) {
                                Image(systemName: FileManager.default.fileExists(atPath: target.path) ? "externaldrive.fill" : "arrow.down.doc")
                                    .foregroundStyle(.secondary)
                                Text(target.path)
                                    .font(.caption.monospaced())
                                    .lineLimit(2)
                                    .textSelection(.enabled)
                            }
                        }
                        if !FileManager.default.fileExists(atPath: target.path) {
                            Label(ConverterCopy.text("Input.NewOutput", language: language), systemImage: "checkmark.shield")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
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

                if let sourceInspection = workflow.sourceInspection {
                    Section(ConverterCopy.text("Input.SHA256", language: language)) {
                        InspectionTable(
                            source: sourceInspection,
                            current: workflow.currentInspection,
                            target: workflow.targetInspection,
                            mode: workflow.mode,
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

                if workflow.coreInspectionComplete, workflow.state == .componentSelection {
                    WorkflowGuidanceSection(
                        messageKey: "Guide.InputComplete",
                        actionKey: "Guide.ToComponents",
                        systemImage: "square.stack.3d.up",
                        language: language
                    ) {
                        navigation = .components
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(workflow.activeOperation != nil)
        }
        .onAppear {
            source = workflow.input?.source
            current = workflow.input?.current
            target = workflow.input?.target
            sourceSelection = source
            currentSelection = current
            targetSelection = target
            if let source, let resolvedSlot = SavePathResolver.slot(for: source) {
                slot = resolvedSlot
            }
        }
    }

    private var hasInput: Bool {
        (source ?? workflow.input?.source) != nil
            && (target ?? workflow.input?.target) != nil
            && (workflow.mode == .newConversion || (current ?? workflow.input?.current) != nil)
    }

    private func chooseSource() {
        guard let url = OpenPanel.selectFileOrDirectory(
            title: ConverterCopy.text("Input.Source", language: language),
            message: ConverterCopy.text("Input.SourceMessage", language: language)
        ) else { return }
        sourceSelection = url
        if let selectedSlot = SavePathResolver.slot(for: url) {
            slot = selectedSlot
        }
        resolveSelections()
    }

    private func chooseTarget() {
        guard let url = OpenPanel.selectFileOrDirectory(
            title: ConverterCopy.text(
                workflow.mode == .repairConverted ? "Input.RepairOutput" : "Input.Target",
                language: language
            ),
            message: ConverterCopy.text(
                workflow.mode == .repairConverted ? "Input.RepairOutputMessage" : "Input.TargetMessage",
                language: language
            )
        ) else { return }
        targetSelection = url
        resolveSelections()
    }

    private func chooseCurrent() {
        guard let url = OpenPanel.selectFileOrDirectory(
            title: ConverterCopy.text("Input.Current", language: language),
            message: ConverterCopy.text("Input.CurrentMessage", language: language)
        ) else { return }
        currentSelection = url
        resolveSelections()
    }

    private func resolveSelections() {
        do {
            if let sourceSelection {
                source = try SavePathResolver.resolveSource(selection: sourceSelection, slot: slot)
            }
            if let currentSelection {
                current = try SavePathResolver.resolveSource(selection: currentSelection, slot: slot)
            }
            if let targetSelection {
                target = try SavePathResolver.resolveTarget(selection: targetSelection, slot: slot)
            }
            selectionError = nil
            updateInput()
        } catch {
            selectionError = error.localizedDescription
        }
    }

    private func updateInput() {
        guard let source, let target else { return }
        if workflow.mode == .repairConverted {
            guard let current else { return }
            workflow.configure(input: ConversionInput(source: source, target: target, current: current))
        } else {
            workflow.configure(input: ConversionInput(source: source, target: target))
        }
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

struct WorkflowGuidanceSection: View {
    let messageKey: String
    let actionKey: String?
    let systemImage: String
    let language: ConverterLanguage
    let action: (() -> Void)?

    init(
        messageKey: String,
        actionKey: String? = nil,
        systemImage: String,
        language: ConverterLanguage,
        action: (() -> Void)? = nil
    ) {
        self.messageKey = messageKey
        self.actionKey = actionKey
        self.systemImage = systemImage
        self.language = language
        self.action = action
    }

    var body: some View {
        Section {
            Label(ConverterCopy.text(messageKey, language: language), systemImage: systemImage)
                .foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)
            if let actionKey, let action {
                Button(action: action) {
                    Label(ConverterCopy.text(actionKey, language: language), systemImage: "arrow.right.circle.fill")
                }
                .buttonStyle(.borderedProminent)
            }
        } header: {
            Label(ConverterCopy.text("Guide.NextStep", language: language), systemImage: "signpost.right")
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
    let selectionHint: String?
    let choose: () -> Void

    init(
        title: String,
        value: URL?,
        chooseTitle: String,
        selectionHint: String? = nil,
        choose: @escaping () -> Void
    ) {
        self.title = title
        self.value = value
        self.chooseTitle = chooseTitle
        self.selectionHint = selectionHint
        self.choose = choose
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            LabeledContent(title) {
                HStack(alignment: .center, spacing: 8) {
                    if let value {
                        Image(systemName: icon(for: value))
                            .foregroundStyle(.secondary)
                        VStack(alignment: .trailing, spacing: 2) {
                            Text(value.lastPathComponent)
                                .lineLimit(1)
                            Text(value.deletingLastPathComponent().path)
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .textSelection(.enabled)
                        }
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .help(value.path)
                    } else {
                        Text("—")
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                    }
                    Button(action: choose) {
                        Label(chooseTitle, systemImage: "folder.badge.plus")
                    }
                    .buttonStyle(.borderless)
                    .help(chooseTitle)
                    .accessibilityLabel(chooseTitle)
                }
            }
            if let selectionHint {
                Label(selectionHint, systemImage: "info.circle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func icon(for url: URL) -> String {
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory)
        return isDirectory.boolValue ? "folder.fill" : "doc.fill"
    }
}

private struct CorePathSelectionGuide: View {
    let language: ConverterLanguage

    var body: some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                Text(ConverterCopy.text("Input.PathGuideSummary", language: language))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(ConverterCopy.text("Input.PathGuideExample", language: language))
                    .font(.caption.monospaced())
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.top, 6)
        } label: {
            Label(
                ConverterCopy.text("Input.PathGuideTitle", language: language),
                systemImage: "folder.badge.questionmark"
            )
        }
    }
}

struct InspectionTable: View {
    let source: InputInspection
    let current: InputInspection?
    let target: InputInspection?
    let mode: ConversionMode
    let language: ConverterLanguage

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 9) {
            GridRow {
                Text("")
                Text(ConverterCopy.text("Input.Source", language: language)).font(.caption).foregroundStyle(.secondary)
                if mode == .repairConverted {
                    Text(ConverterCopy.text("Input.Current", language: language)).font(.caption).foregroundStyle(.secondary)
                }
                Text(
                    ConverterCopy.text(
                        mode == .repairConverted ? "Input.RepairOutput" : "Input.Target",
                        language: language
                    )
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            GridRow {
                Text(ConverterCopy.text("Input.Profile", language: language)).foregroundStyle(.secondary)
                Text(source.profile)
                if mode == .repairConverted {
                    inspectionValue(current?.profile)
                }
                targetValue(target?.profile)
            }
            GridRow {
                Text(ConverterCopy.text("Input.Bytes", language: language)).foregroundStyle(.secondary)
                Text(source.size, format: .number)
                if mode == .repairConverted {
                    inspectionValue(current.map { String($0.size) })
                }
                targetValue(target.map { String($0.size) })
            }
            GridRow {
                Text(ConverterCopy.text("Input.SHA256", language: language)).foregroundStyle(.secondary)
                Text(source.sha256).font(.caption.monospaced()).textSelection(.enabled)
                if mode == .repairConverted {
                    inspectionValue(current?.sha256, monospaced: true)
                }
                targetValue(target?.sha256, monospaced: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func inspectionValue(_ value: String?, monospaced: Bool = false) -> some View {
        if let value {
            Text(value)
                .font(monospaced ? .caption.monospaced() : .body)
                .textSelection(.enabled)
        } else {
            Label(ConverterCopy.text("Input.Required", language: language), systemImage: "exclamationmark.circle")
                .font(.caption)
                .foregroundStyle(.orange)
        }
    }

    @ViewBuilder
    private func targetValue(_ value: String?, monospaced: Bool = false) -> some View {
        if let value {
            Text(value)
                .font(monospaced ? .caption.monospaced() : .body)
                .textSelection(.enabled)
        } else {
            Label(ConverterCopy.text("Input.NewOutput", language: language), systemImage: "arrow.down.doc")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
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
