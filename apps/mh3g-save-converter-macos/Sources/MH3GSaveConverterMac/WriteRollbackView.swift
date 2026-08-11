import SwiftUI
import ConverterPresentation

struct WriteRollbackView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage
    @Binding var navigation: ConverterNavigation?
    @State private var showCoreConfirmation = false
    @State private var showSystemConfirmation = false
    @State private var showExtrasConfirmation = false
    @State private var coreManifest: URL?
    @State private var systemManifest: URL?
    @State private var extrasManifest: URL?
    @State private var isRunning = false

    var body: some View {
        WorkbenchPage(
            artwork: .rollbackHarbor,
            title: ConverterCopy.text("Navigation.WriteRollback", language: language),
            subtitle: ConverterCopy.text("Write.Subtitle", language: language)
        ) {
            Form {
                coreSection

                if workflow.components.includeSystem {
                    systemSection
                }

                if !workflow.components.selectedGroups.isEmpty {
                    extrasSection
                }

                if let failure = workflow.failure {
                    Section {
                        FailureDetails(failure: failure, language: language)
                    }
                }

                if !workflow.selectedOptionalDataIsConfigured {
                    WorkflowGuidanceSection(
                        messageKey: "Guide.OptionalDataNeedsConfiguration",
                        actionKey: "Guide.ToComponents",
                        systemImage: "square.stack.3d.up",
                        language: language
                    ) {
                        navigation = .components
                    }
                } else if workflow.state == .success, workflow.hasPendingSelectedConversionWork {
                    WorkflowGuidanceSection(
                        messageKey: "Guide.SelectedWorkPending",
                        systemImage: "checklist",
                        language: language
                    )
                } else if workflow.state == .success {
                    WorkflowGuidanceSection(
                        messageKey: "Guide.WriteComplete",
                        actionKey: "Guide.ToHistory",
                        systemImage: "clock.arrow.circlepath",
                        language: language
                    ) {
                        navigation = .history
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(workflow.activeOperation != nil)
        }
        .sheet(isPresented: $showCoreConfirmation) {
            TransactionConfirmationSheet(
                title: ConverterCopy.text("Write.SelectedSave", language: language),
                targetLabel: ConverterCopy.text(
                    workflow.mode == .repairConverted ? "Input.RepairOutput" : "Write.Target",
                    language: language
                ),
                target: workflow.input?.target,
                files: workflow.mode == .repairConverted && workflow.components.includeGuildCards
                    ? ConverterCopy.text("Write.RepairTargets", language: language)
                    : ConverterCopy.text("Write.OneTarget", language: language),
                language: language,
                verificationDetails: coreConfirmationDetails,
                onConfirm: writeCore,
                onCancel: { showCoreConfirmation = false }
            )
        }
        .sheet(isPresented: $showSystemConfirmation) {
            TransactionConfirmationSheet(
                title: ConverterCopy.text("Write.WriteSystem", language: language),
                targetLabel: ConverterCopy.text("Write.Target", language: language),
                target: workflow.components.systemTarget,
                files: ConverterCopy.text("Write.OneTarget", language: language),
                language: language,
                verificationDetails: systemConfirmationDetails,
                onConfirm: writeSystem,
                onCancel: { showSystemConfirmation = false }
            )
        }
        .sheet(isPresented: $showExtrasConfirmation) {
            TransactionConfirmationSheet(
                title: ConverterCopy.text("Write.ExtrasInstall", language: language),
                targetLabel: ConverterCopy.text("Write.TargetDirectory", language: language),
                target: workflow.components.extraTargetDirectory,
                files: selectedExtraGroups,
                language: language,
                verificationDetails: extrasConfirmationDetails,
                onConfirm: installExtras,
                onCancel: { showExtrasConfirmation = false }
            )
        }
    }

    private var coreSection: some View {
        Section {
            LabeledContent(ConverterCopy.text("Write.Authorization", language: language)) {
                authorizationLabel(workflow.canWrite)
            }
            Button(ConverterCopy.text("Write.Confirm", language: language)) {
                showCoreConfirmation = true
            }
            .disabled(!workflow.canWrite || isRunning)

            SelectedPathRow(
                title: ConverterCopy.text("Write.Manifest", language: language),
                value: coreManifest,
                chooseTitle: ConverterCopy.text("Input.Select", language: language)
            ) {
                coreManifest = selectManifest(
                    title: ConverterCopy.text("Write.SelectManifest", language: language)
                )
            }
            Button(ConverterCopy.text("Write.Rollback", language: language)) {
                rollbackCore()
            }
            .disabled(coreManifest == nil || isRunning)
        } header: {
            Text(ConverterCopy.text("Write.Core", language: language))
        } footer: {
            Text(ConverterCopy.text("Write.Footer", language: language))
        }
    }

    private var systemSection: some View {
        Section {
            LabeledContent(ConverterCopy.text("Write.Authorization", language: language)) {
                authorizationLabel(workflow.canWriteSystem)
            }
            Button(ConverterCopy.text("Write.SystemDryRun", language: language)) {
                runSystemDryRun()
            }
            .disabled(!hasSystemPaths || isRunning)
            Button(ConverterCopy.text("Write.WriteSystem", language: language)) {
                showSystemConfirmation = true
            }
            .disabled(!workflow.canWriteSystem || isRunning)

            SelectedPathRow(
                title: ConverterCopy.text("Write.Manifest", language: language),
                value: systemManifest,
                chooseTitle: ConverterCopy.text("Input.Select", language: language)
            ) {
                systemManifest = selectManifest(
                    title: ConverterCopy.text("Write.SelectManifest", language: language)
                )
            }
            Button(ConverterCopy.text("Write.Rollback", language: language)) {
                rollbackSystem()
            }
            .disabled(systemManifest == nil || isRunning)
        } header: {
            Text(ConverterCopy.text("Write.System", language: language))
        } footer: {
            Text(ConverterCopy.text("Write.SystemFooter", language: language))
        }
    }

    private var extrasSection: some View {
        Section {
            LabeledContent(ConverterCopy.text("Components.GroupScope", language: language)) {
                Text(selectedExtraGroups)
                    .foregroundStyle(.secondary)
            }
            LabeledContent(ConverterCopy.text("Write.Authorization", language: language)) {
                authorizationLabel(workflow.canStageExtras, readyKey: "Write.StageReady")
            }
            Button(ConverterCopy.text("Write.ExtrasStageDryRun", language: language)) {
                runExtrasStageDryRun()
            }
            .disabled(!hasExtraPaths || isRunning)
            Button(ConverterCopy.text("Write.ExtrasStage", language: language)) {
                stageExtras()
            }
            .disabled(!workflow.canStageExtras || isRunning)

            LabeledContent(ConverterCopy.text("Write.Authorization", language: language)) {
                authorizationLabel(workflow.canInstallExtras, readyKey: "Write.InstallReady")
            }
            Button(ConverterCopy.text("Write.ExtrasInstallDryRun", language: language)) {
                runExtrasInstallDryRun()
            }
            .disabled(!hasExtraPaths || isRunning)
            Button(ConverterCopy.text("Write.ExtrasInstall", language: language)) {
                showExtrasConfirmation = true
            }
            .disabled(!workflow.canInstallExtras || isRunning)

            SelectedPathRow(
                title: ConverterCopy.text("Write.ExtrasManifest", language: language),
                value: extrasManifest,
                chooseTitle: ConverterCopy.text("Input.Select", language: language)
            ) {
                extrasManifest = selectManifest(
                    title: ConverterCopy.text("Write.ExtrasManifest", language: language)
                )
            }
            Button(ConverterCopy.text("Write.Rollback", language: language)) {
                rollbackExtras()
            }
            .disabled(extrasManifest == nil || isRunning)
        } header: {
            Text(ConverterCopy.text("Write.Extras", language: language))
        } footer: {
            Text(ConverterCopy.text("Write.ExtrasFooter", language: language))
        }
    }

    private var hasSystemPaths: Bool {
        workflow.components.systemSource != nil && workflow.components.systemTarget != nil
    }

    private var hasExtraPaths: Bool {
        !workflow.components.selectedGroups.isEmpty
            && workflow.components.extraSourceDirectory != nil
            && workflow.components.extraStagingDirectory != nil
            && workflow.components.extraTargetDirectory != nil
    }

    private var selectedExtraGroups: String {
        workflow.components.selectedGroups
            .sorted { $0.rawValue < $1.rawValue }
            .map { group in
                switch group {
                case .guildCards:
                    ConverterCopy.text("Components.GuildCards", language: language)
                case .quests:
                    ConverterCopy.text("Components.Quests", language: language)
                }
            }
            .joined(separator: " · ")
    }

    private var coreConfirmationDetails: [TransactionConfirmationDetail] {
        if let fingerprint = workflow.repairDryRunFingerprint {
            return [
                TransactionConfirmationDetail(
                    label: ConverterCopy.text("Write.SourceSHA256", language: language),
                    value: fingerprint.sourceSetSHA256
                ),
                TransactionConfirmationDetail(
                    label: ConverterCopy.text("Write.CurrentSetSHA256", language: language),
                    value: fingerprint.currentSetSHA256
                ),
                TransactionConfirmationDetail(
                    label: ConverterCopy.text("Write.OutputSetSHA256", language: language),
                    value: fingerprint.outputSetSHA256
                ),
                TransactionConfirmationDetail(
                    label: ConverterCopy.text("Repair.PreviewSHA256", language: language),
                    value: fingerprint.previewSHA256
                ),
            ]
        }
        guard let fingerprint = workflow.dryRunFingerprint else { return [] }
        return [
            TransactionConfirmationDetail(
                label: ConverterCopy.text("Write.SourceSHA256", language: language),
                value: fingerprint.sourceSHA256
            ),
            TransactionConfirmationDetail(
                label: fingerprint.exportsNewTarget
                    ? ConverterCopy.text("Write.Target", language: language)
                    : ConverterCopy.text("Write.TargetSHA256", language: language),
                value: fingerprint.targetSHA256 ?? ConverterCopy.text("Write.NewExport", language: language)
            ),
        ]
    }

    private var systemConfirmationDetails: [TransactionConfirmationDetail] {
        guard let fingerprint = workflow.systemDryRunFingerprint else { return [] }
        return [
            TransactionConfirmationDetail(
                label: ConverterCopy.text("Write.SourceSHA256", language: language),
                value: fingerprint.sourceSHA256
            ),
            TransactionConfirmationDetail(
                label: ConverterCopy.text("Write.TargetSHA256", language: language),
                value: fingerprint.targetSHA256
            ),
        ]
    }

    private var extrasConfirmationDetails: [TransactionConfirmationDetail] {
        guard let fingerprint = workflow.extrasInstallDryRunFingerprint else { return [] }
        return [
            TransactionConfirmationDetail(
                label: ConverterCopy.text("Write.StagingSetSHA256", language: language),
                value: fingerprint.stagingSetSHA256
            ),
            TransactionConfirmationDetail(
                label: ConverterCopy.text("Write.TargetSetSHA256", language: language),
                value: fingerprint.targetSetSHA256
            ),
        ]
    }

    @ViewBuilder
    private func authorizationLabel(_ authorized: Bool, readyKey: String = "Write.CurrentAuthorized") -> some View {
        Label(
            authorized ? ConverterCopy.text(readyKey, language: language) : ConverterCopy.text("Write.Unavailable", language: language),
            systemImage: authorized ? "checkmark.shield.fill" : "lock.fill"
        )
        .foregroundStyle(authorized ? .green : .secondary)
    }

    private func selectManifest(title: String) -> URL? {
        OpenPanel.selectFile(
            title: title,
            message: ConverterCopy.text("Write.SelectManifestMessage", language: language)
        )
    }

    private func writeCore() {
        showCoreConfirmation = false
        isRunning = true
        Task {
            defer { isRunning = false }
            do {
                try await workflow.writeCore()
                captureManifest(into: &coreManifest)
            } catch {}
        }
    }

    private func rollbackCore() {
        guard let coreManifest else { return }
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.rollback(manifest: coreManifest)
        }
    }

    private func runSystemDryRun() {
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.runSystemDryRun()
        }
    }

    private func writeSystem() {
        showSystemConfirmation = false
        isRunning = true
        Task {
            defer { isRunning = false }
            do {
                try await workflow.writeSystem()
                captureManifest(into: &systemManifest)
            } catch {}
        }
    }

    private func rollbackSystem() {
        guard let systemManifest else { return }
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.rollback(manifest: systemManifest, system: true)
        }
    }

    private func runExtrasStageDryRun() {
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.runExtrasStageDryRun()
        }
    }

    private func stageExtras() {
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.stageExtras()
        }
    }

    private func runExtrasInstallDryRun() {
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.runExtrasInstallDryRun()
        }
    }

    private func installExtras() {
        showExtrasConfirmation = false
        isRunning = true
        Task {
            defer { isRunning = false }
            do {
                try await workflow.installExtraGroups()
                captureManifest(into: &extrasManifest)
            } catch {}
        }
    }

    private func rollbackExtras() {
        guard let extrasManifest else { return }
        isRunning = true
        Task {
            defer { isRunning = false }
            try? await workflow.rollback(manifest: extrasManifest, extraGroup: true)
        }
    }

    private func captureManifest(into destination: inout URL?) {
        guard let report = workflow.latestReport,
              let path = report.compatibilityManifest ?? report.manifest
        else { return }
        destination = URL(fileURLWithPath: path)
    }
}

struct TransactionConfirmationDetail: Identifiable {
    let label: String
    let value: String
    let monospaced: Bool

    init(label: String, value: String, monospaced: Bool = true) {
        self.label = label
        self.value = value
        self.monospaced = monospaced
    }

    var id: String { label }
}

struct TransactionConfirmationSheet: View {
    let title: String
    let targetLabel: String
    let target: URL?
    let files: String
    let language: ConverterLanguage
    let verificationDetails: [TransactionConfirmationDetail]
    let confirmationRole: ButtonRole?
    let onConfirm: () -> Void
    let onCancel: () -> Void

    init(
        title: String,
        targetLabel: String,
        target: URL?,
        files: String,
        language: ConverterLanguage,
        verificationDetails: [TransactionConfirmationDetail],
        confirmationRole: ButtonRole? = nil,
        onConfirm: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.title = title
        self.targetLabel = targetLabel
        self.target = target
        self.files = files
        self.language = language
        self.verificationDetails = verificationDetails
        self.confirmationRole = confirmationRole
        self.onConfirm = onConfirm
        self.onCancel = onCancel
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Image(systemName: "externaldrive.badge.checkmark")
                    .font(.title2)
                    .foregroundStyle(.tint)
                Text(title)
                    .font(.title3.weight(.semibold))
            }
            ScrollView {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 10) {
                    GridRow {
                        Text(ConverterCopy.text("Write.Files", language: language))
                            .foregroundStyle(.secondary)
                        Text(files)
                    }
                    GridRow {
                        Text(targetLabel)
                            .foregroundStyle(.secondary)
                        Text(target?.path ?? "—")
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(verificationDetails) { detail in
                        GridRow {
                            Text(detail.label)
                                .foregroundStyle(.secondary)
                            Text(detail.value)
                                .font(detail.monospaced ? .caption.monospaced() : .body)
                                .textSelection(.enabled)
                                .fixedSize(horizontal: false, vertical: true)
                        }
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
                }
            }
            .frame(maxHeight: 290)
            Spacer(minLength: 0)
            HStack {
                Button(ConverterCopy.text("Write.Cancel", language: language), action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button(title, role: confirmationRole, action: onConfirm)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(minWidth: 570, idealWidth: 620, maxWidth: 680, minHeight: 330, idealHeight: 430, maxHeight: 500)
    }
}
