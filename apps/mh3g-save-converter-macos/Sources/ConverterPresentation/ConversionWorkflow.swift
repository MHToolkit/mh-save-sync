import Foundation
import Observation

@MainActor
@Observable
public final class ConversionWorkflow {
    public private(set) var mode: ConversionMode = .newConversion
    public private(set) var state: WorkflowState = .input
    public private(set) var input: ConversionInput?
    public private(set) var sourceInspection: InputInspection?
    public private(set) var currentInspection: InputInspection?
    public private(set) var targetInspection: InputInspection?
    public private(set) var components = ComponentSelection()
    public private(set) var dryRunFingerprint: DryRunFingerprint?
    public private(set) var repairDryRunFingerprint: RepairDryRunFingerprint?
    public private(set) var repairFromVersion: HistoricalConverterRevision?
    public private(set) var repairRevisionCandidates = [HistoricalConverterRevision]()
    public private(set) var repairRevisionSelectionRequired = false
    public private(set) var systemDryRunFingerprint: SystemDryRunFingerprint?
    public private(set) var extrasStageDryRunFingerprint: ExtrasStageDryRunFingerprint?
    public private(set) var extrasInstallDryRunFingerprint: ExtrasInstallDryRunFingerprint?
    public private(set) var cecDryRunFingerprint: CECDryRunFingerprint?
    public private(set) var coreWriteCompleted = false
    public private(set) var systemWriteCompleted = false
    public private(set) var extrasInstallCompleted = false
    public private(set) var failure: WorkflowFailure?
    public private(set) var latestReport: ConverterReport?
    public private(set) var activeOperation: ConverterOperation?

    private let executable: URL
    private let executor: any ConverterCommandExecuting
    private var activeOperationLease: UUID?

    public init(executable: URL, executor: any ConverterCommandExecuting = ConverterCommandClient()) {
        self.executable = executable.standardizedFileURL
        self.executor = executor
    }

    public var canStartDryRun: Bool {
        input != nil
            && coreInspectionComplete
            && selectedOptionalDataIsConfigured
            && activeOperation == nil
    }

    /// Repair mode has two independent read inputs.  The input stage is not
    /// complete until both the original 3DS slot and the current Wii U/Cemu
    /// reference have been inspected; the output may legitimately be absent.
    public var coreInspectionComplete: Bool {
        sourceInspection != nil
            && (mode == .newConversion || currentInspection != nil)
    }

    /// A missing target inspection is expected for an export directory: the
    /// selected `user#` does not exist until the guarded transactional write.
    public var isNewTargetExport: Bool {
        input != nil && coreInspectionComplete && targetInspection == nil
    }

    /// A selected optional domain cannot be treated as ready until every path
    /// required by that domain is explicit. Core Dry Run and write use this
    /// same gate; every optional command also validates its own paths before
    /// argv construction.
    public var selectedOptionalDataIsConfigured: Bool {
        if mode == .repairConverted {
            return !components.includeGuildCards || components.extraSourceDirectory != nil
        }
        let systemConfigured = !components.includeSystem
            || (components.systemSource != nil && components.systemTarget != nil)
        let extrasConfigured = components.selectedGroups.isEmpty
            || (components.extraSourceDirectory != nil
                && components.extraStagingDirectory != nil
                && components.extraTargetDirectory != nil)
        return systemConfigured && extrasConfigured
    }

    /// Completion is tracked per selected standard transaction domain. A
    /// core-slot write must never make an enabled `system` or ExtData choice
    /// look complete in the UI. Experimental CEC remains an independent tool
    /// and does not block the normal conversion route.
    public var hasPendingSelectedOptionalWork: Bool {
        if mode == .repairConverted {
            return false
        }
        return (components.includeSystem && !systemWriteCompleted)
            || (!components.selectedGroups.isEmpty && !extrasInstallCompleted)
    }

    public var hasPendingSelectedConversionWork: Bool {
        !coreWriteCompleted || hasPendingSelectedOptionalWork
    }

    /// This is intentionally stricter than the button's visual disabled state.
    /// Every write path re-checks its own current authorization before
    /// constructing argv, so programmatic UI calls cannot bypass the guard.
    public var canWrite: Bool {
        if mode == .repairConverted {
            guard activeOperation == nil,
                  let authorized = repairDryRunFingerprint,
                  let input
            else { return false }
            return authorized.source == input.source.standardizedFileURL
                && authorized.current == input.current?.standardizedFileURL
                && authorized.output == input.target.standardizedFileURL
                && authorized.extDataSource == components.extraSourceDirectory?.standardizedFileURL
                && authorized.fromVersion == repairFromVersion
                && !repairRevisionSelectionRequired
        }
        guard activeOperation == nil,
              selectedOptionalDataIsConfigured,
              let authorized = dryRunFingerprint,
              let current = currentFingerprint()
        else { return false }
        return authorized.sourceSHA256 == current.sourceSHA256
            && authorized.targetSHA256 == current.targetSHA256
    }

    public var canWriteCEC: Bool {
        guard activeOperation == nil,
              components.includesCEC,
              components.acknowledgeExperimentalCEC,
              let source = components.cecSourceDirectory,
              let target = components.cecTarget,
              let authorized = cecDryRunFingerprint
        else { return false }
        return authorized.sourceDirectory == source.standardizedFileURL
            && authorized.target == target.standardizedFileURL
    }

    public var canWriteSystem: Bool {
        guard activeOperation == nil,
              components.includeSystem,
              let source = components.systemSource,
              let target = components.systemTarget,
              let authorized = systemDryRunFingerprint
        else { return false }
        return authorized.source == source.standardizedFileURL
            && authorized.target == target.standardizedFileURL
    }

    public var canStageExtras: Bool {
        guard activeOperation == nil,
              let authorized = extrasStageDryRunFingerprint,
              let paths = try? requiredExtraPaths()
        else { return false }
        return authorized.sourceDirectory == paths.source.standardizedFileURL
            && authorized.stagingDirectory == paths.staging.standardizedFileURL
            && authorized.groups == components.selectedGroups
    }

    public var canInstallExtras: Bool {
        guard activeOperation == nil,
              let authorized = extrasInstallDryRunFingerprint,
              let paths = try? requiredExtraPaths()
        else { return false }
        return authorized.stagingDirectory == paths.staging.standardizedFileURL
            && authorized.targetDirectory == paths.target.standardizedFileURL
            && authorized.groups == components.selectedGroups
    }

    public func configure(input: ConversionInput) {
        guard activeOperation == nil else { return }
        guard self.input != input else { return }
        self.input = input
        sourceInspection = nil
        currentInspection = nil
        targetInspection = nil
        coreWriteCompleted = false
        invalidateCoreAuthorization(nextState: .input, clearsPresentation: true)
    }

    public func setMode(_ mode: ConversionMode) {
        guard activeOperation == nil, self.mode != mode else { return }
        self.mode = mode
        repairFromVersion = nil
        repairRevisionCandidates = []
        repairRevisionSelectionRequired = false
        components.includeSystem = false
        components.includeQuests = false
        sourceInspection = nil
        currentInspection = nil
        targetInspection = nil
        coreWriteCompleted = false
        invalidateCoreAuthorization(nextState: .input, clearsPresentation: true)
    }

    public func setRepairFromVersion(_ revision: HistoricalConverterRevision?) {
        guard activeOperation == nil, repairFromVersion != revision else { return }
        repairFromVersion = revision
        repairRevisionSelectionRequired = false
        repairDryRunFingerprint = nil
        if state == .dryRun {
            state = .componentSelection
        }
    }

    public func applyInspections(
        source: InputInspection,
        current: InputInspection? = nil,
        target: InputInspection?
    ) {
        guard activeOperation == nil else { return }
        sourceInspection = source
        currentInspection = current
        targetInspection = target
        coreWriteCompleted = false
        invalidateCoreAuthorization(nextState: .componentSelection, clearsPresentation: true)
    }

    public func setComponents(_ selection: ComponentSelection) {
        guard activeOperation == nil else { return }
        guard components != selection else { return }
        let systemChanged = components.includeSystem != selection.includeSystem
            || components.systemSource != selection.systemSource
            || components.systemTarget != selection.systemTarget
        let extrasChanged = components.includeGuildCards != selection.includeGuildCards
            || components.includeQuests != selection.includeQuests
            || components.extraSourceDirectory != selection.extraSourceDirectory
            || components.extraStagingDirectory != selection.extraStagingDirectory
            || components.extraTargetDirectory != selection.extraTargetDirectory
        let cecChanged = components.cecSourceDirectory != selection.cecSourceDirectory
            || components.cecTarget != selection.cecTarget
            || components.acknowledgeExperimentalCEC != selection.acknowledgeExperimentalCEC
        components = selection
        if systemChanged {
            systemDryRunFingerprint = nil
            systemWriteCompleted = false
        }
        if extrasChanged {
            extrasStageDryRunFingerprint = nil
            extrasInstallDryRunFingerprint = nil
            extrasInstallCompleted = false
        }
        if cecChanged {
            cecDryRunFingerprint = nil
        }
        failure = nil
        state = input == nil ? .input : .componentSelection
    }

    /// Calls the Rust `inspect` command for the exact source file and, if it
    /// already exists, the exact target file. A missing target is a supported
    /// explicit export destination; directory discovery still never occurs.
    public func inspectInputs() async throws {
        try requireIdleForIndependentOperation()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        try await withOperation(.inspect) { lease in
            let sourceReport = try await self.execute(
                .inspect,
                arguments: [ConverterOperation.inspect.rawValue, input.source.path],
                lease: lease
            )
            guard let source = self.inspection(from: sourceReport) else {
                throw self.failureAndRethrow(
                    .inspect,
                    ConversionWorkflowError.invalidReport("inspect requires profile, size, and source SHA-256"),
                    stderr: ""
                )
            }
            let current: InputInspection?
            if self.mode == .repairConverted {
                guard let currentURL = input.current,
                      FileManager.default.fileExists(atPath: currentURL.path)
                else { throw ConversionWorkflowError.inputNotInspected }
                let currentReport = try await self.execute(
                    .inspect,
                    arguments: [ConverterOperation.inspect.rawValue, currentURL.path],
                    lease: lease
                )
                guard let inspectedCurrent = self.inspection(from: currentReport) else {
                    throw self.failureAndRethrow(
                        .inspect,
                        ConversionWorkflowError.invalidReport("current Wii U inspect requires profile, size, and source SHA-256"),
                        stderr: ""
                    )
                }
                current = inspectedCurrent
            } else {
                current = nil
            }
            let target: InputInspection?
            if FileManager.default.fileExists(atPath: input.target.path) {
                let targetReport = try await self.execute(
                    .inspect,
                    arguments: [ConverterOperation.inspect.rawValue, input.target.path],
                    lease: lease
                )
                guard let inspectedTarget = self.inspection(from: targetReport) else {
                    throw self.failureAndRethrow(
                        .inspect,
                        ConversionWorkflowError.invalidReport("target inspect requires profile, size, and source SHA-256"),
                        stderr: ""
                    )
                }
                target = inspectedTarget
            } else {
                target = nil
            }
            self.sourceInspection = source
            self.currentInspection = current
            self.targetInspection = target
            self.invalidateCoreAuthorization(nextState: .componentSelection, clearsPresentation: false)
        }
    }

    /// Core slot Dry Run is the authorization boundary for `writeCore` only.
    /// `system` is a separate source/target pair with its own authorization.
    public func runCoreDryRun() async throws {
        try requireIdleForIndependentOperation()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        guard sourceInspection != nil else { throw ConversionWorkflowError.inputNotInspected }
        try requireSelectedOptionalDataConfiguration()
        let operation: ConverterOperation = mode == .repairConverted ? .repairConverted : .convert
        try await withOperation(operation) { lease in
            self.dryRunFingerprint = nil
            self.repairDryRunFingerprint = nil
            self.repairRevisionCandidates = []
            self.repairRevisionSelectionRequired = false
            if self.mode == .repairConverted {
                guard let current = input.current,
                      self.currentInspection != nil
                else { throw ConversionWorkflowError.inputNotInspected }
                var arguments = [
                    ConverterOperation.repairConverted.rawValue,
                    input.source.path,
                    "--current", current.path,
                    "--output", input.target.path,
                ]
                if self.components.includeGuildCards,
                   let extData = self.components.extraSourceDirectory {
                    arguments += ["--source-extdata-dir", extData.path]
                }
                if let revision = self.repairFromVersion {
                    arguments += ["--from-version", revision.rawValue]
                }
                arguments.append("--dry-run")
                let report = try await self.execute(
                    .repairConverted,
                    arguments: arguments,
                    lease: lease
                )
                guard report.status == "dry-run",
                      let sourceSetSHA256 = report.sourceSetSHA256,
                      ConverterEvidence.isValidSHA256(sourceSetSHA256),
                      let currentSetSHA256 = report.currentSetSHA256,
                      ConverterEvidence.isValidSHA256(currentSetSHA256),
                      let outputSetSHA256 = report.outputSetSHA256,
                      ConverterEvidence.isValidSHA256(outputSetSHA256),
                      let previewSHA256 = report.previewSHA256,
                      ConverterEvidence.isValidSHA256(previewSHA256),
                      let detection = report.detection,
                      let repairComponents = report.components,
                      !repairComponents.isEmpty,
                      repairComponents.allSatisfy({ $0.repairFingerprint() != nil })
                else {
                    throw self.failureAndRethrow(
                        .repairConverted,
                        ConversionWorkflowError.invalidReport("repair Dry Run requires source/current/output set and preview SHA-256"),
                        stderr: report.stderr ?? ""
                    )
                }
                self.repairRevisionCandidates = detection.candidates
                self.repairRevisionSelectionRequired =
                    detection.confidence == "ambiguous"
                    && self.repairFromVersion == nil
                if self.repairRevisionSelectionRequired {
                    self.latestReport = report
                    self.state = .dryRun
                    return
                }
                self.repairDryRunFingerprint = RepairDryRunFingerprint(
                    source: input.source,
                    current: current,
                    output: input.target,
                    extDataSource: self.components.includeGuildCards ? self.components.extraSourceDirectory : nil,
                    fromVersion: self.repairFromVersion,
                    sourceSetSHA256: sourceSetSHA256,
                    currentSetSHA256: currentSetSHA256,
                    outputSetSHA256: outputSetSHA256,
                    previewSHA256: previewSHA256,
                    components: repairComponents.compactMap { $0.repairFingerprint() }
                )
                self.latestReport = report
                self.state = .dryRun
                return
            }
            let report = try await self.execute(
                .convert,
                arguments: [
                    ConverterOperation.convert.rawValue,
                    input.source.path,
                    "--output", input.target.path,
                    "--dry-run",
                ],
                lease: lease
            )
            guard report.status == "dry-run" else {
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("expected dry-run status"),
                    stderr: report.stderr ?? ""
                )
            }
            let current = try self.requireCurrentFingerprint()
            guard let reportedSource = report.hash(named: "source"),
                  ConverterEvidence.isValidSHA256(reportedSource),
                  let reportedOutput = report.hash(named: "output"),
                  ConverterEvidence.isValidSHA256(reportedOutput)
            else {
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("Dry Run requires valid source and output SHA-256"),
                    stderr: report.stderr ?? ""
                )
            }
            if reportedSource != current.sourceSHA256 {
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("source SHA-256 changed during Dry Run"),
                    stderr: report.stderr ?? ""
                )
            }
            let reportedTarget = report.hash(named: "target_before")
            switch (current.targetSHA256, reportedTarget) {
            case let (.some(expected), .some(reported))
                where ConverterEvidence.isValidSHA256(reported) && expected == reported:
                break
            case (nil, nil):
                break
            case (nil, .some):
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("target appeared during Dry Run; refuse export"),
                    stderr: report.stderr ?? ""
                )
            case (.some, nil):
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("Dry Run requires source and target_before SHA-256"),
                    stderr: report.stderr ?? ""
                )
            case (.some, .some):
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("target SHA-256 changed during Dry Run"),
                    stderr: report.stderr ?? ""
                )
            }
            self.dryRunFingerprint = DryRunFingerprint(
                sourceSHA256: current.sourceSHA256,
                targetSHA256: current.targetSHA256,
                outputSHA256: reportedOutput
            )
            self.latestReport = report
            self.state = .dryRun
        }
    }

    public func writeCore() async throws {
        try requireIdleForIndependentOperation()
        try requireSelectedOptionalDataConfiguration()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        if mode == .repairConverted {
            guard let fingerprint = repairDryRunFingerprint,
                  let current = input.current,
                  fingerprint.source == input.source.standardizedFileURL,
                  fingerprint.current == current.standardizedFileURL,
                  fingerprint.output == input.target.standardizedFileURL,
                  fingerprint.extDataSource == components.extraSourceDirectory?.standardizedFileURL,
                  fingerprint.fromVersion == repairFromVersion,
                  !repairRevisionSelectionRequired
            else { throw ConversionWorkflowError.dryRunRequired }
            try await withOperation(.repairConverted) { lease in
                var arguments = [
                    ConverterOperation.repairConverted.rawValue,
                    input.source.path,
                    "--current", current.path,
                    "--output", input.target.path,
                ]
                if self.components.includeGuildCards,
                   let extData = self.components.extraSourceDirectory {
                    arguments += ["--source-extdata-dir", extData.path]
                }
                if let revision = fingerprint.fromVersion {
                    arguments += ["--from-version", revision.rawValue]
                }
                arguments += [
                    "--write",
                    "--expected-source-set-sha256", fingerprint.sourceSetSHA256,
                    "--expected-current-set-sha256", fingerprint.currentSetSHA256,
                    "--expected-output-set-sha256", fingerprint.outputSetSHA256,
                    "--expected-preview-sha256", fingerprint.previewSHA256,
                ]
                let report = try await self.execute(
                    .repairConverted,
                    arguments: arguments,
                    lease: lease
                )
                try self.completeRepair(with: report, fingerprint: fingerprint, input: input)
            }
            return
        }
        let fingerprint = try currentAuthorizedFingerprint()
        try await withOperation(.convert) { lease in
            let report = try await self.execute(
                .convert,
                arguments: self.coreWriteArguments(input: input, fingerprint: fingerprint),
                lease: lease
            )
            try self.complete(with: report, expectedStatus: "written", operation: .convert)
        }
    }

    /// `system` does not share the selected `user#` slot's source or target.
    /// Its Dry Run must therefore establish the two preconditions used by its
    /// subsequent write independently.
    public func runSystemDryRun() async throws {
        try requireIdleForIndependentOperation()
        guard components.includeSystem,
              let source = components.systemSource,
              let target = components.systemTarget
        else { throw ConversionWorkflowError.missingSystemPaths }
        try await withOperation(.convertSystem) { lease in
            self.systemDryRunFingerprint = nil
            let report = try await self.execute(
                .convertSystem,
                arguments: [
                    ConverterOperation.convertSystem.rawValue,
                    source.path,
                    "--output", target.path,
                    "--dry-run",
                ],
                lease: lease
            )
            guard report.status == "dry-run",
                  let sourceSHA256 = report.hash(named: "source"),
                  ConverterEvidence.isValidSHA256(sourceSHA256),
                  let targetSHA256 = report.hash(named: "target_before"),
                  ConverterEvidence.isValidSHA256(targetSHA256),
                  let outputSHA256 = report.hash(named: "output"),
                  ConverterEvidence.isValidSHA256(outputSHA256)
            else {
                throw self.failureAndRethrow(
                    .convertSystem,
                    ConversionWorkflowError.invalidReport("system Dry Run requires source and target_before SHA-256"),
                    stderr: report.stderr ?? ""
                )
            }
            self.systemDryRunFingerprint = SystemDryRunFingerprint(
                source: source,
                target: target,
                sourceSHA256: sourceSHA256,
                targetSHA256: targetSHA256,
                outputSHA256: outputSHA256
            )
            self.latestReport = report
            self.state = .dryRun
        }
    }

    public func writeSystem() async throws {
        try requireIdleForIndependentOperation()
        guard components.includeSystem,
              let source = components.systemSource,
              let target = components.systemTarget
        else { throw ConversionWorkflowError.missingSystemPaths }
        let fingerprint = try currentAuthorizedSystemFingerprint(source: source, target: target)
        try await withOperation(.convertSystem) { lease in
            let report = try await self.execute(
                .convertSystem,
                arguments: [
                    ConverterOperation.convertSystem.rawValue,
                    source.path,
                    "--output", target.path,
                    "--write",
                    "--expected-source-sha256", fingerprint.sourceSHA256,
                    "--expected-target-sha256", fingerprint.targetSHA256,
                ],
                lease: lease
            )
            try self.complete(with: report, expectedStatus: "written", operation: .convertSystem)
        }
    }

    /// Preview the full ExtData staging set. This does not authorize a Cemu
    /// write; it only authorizes creating the explicitly selected staging
    /// directory after the same read-only plan still matches.
    public func runExtrasStageDryRun() async throws {
        try requireIdleForIndependentOperation()
        let paths = try requiredExtraPaths()
        try await withOperation(.convertExtras) { lease in
            self.extrasStageDryRunFingerprint = nil
            let report = try await self.execute(
                .convertExtras,
                arguments: self.extrasStageArguments(paths: paths, write: false),
                lease: lease
            )
            guard report.status == "dry-run" else {
                throw self.failureAndRethrow(
                    .convertExtras,
                    ConversionWorkflowError.invalidReport("expected ExtData stage dry-run status"),
                    stderr: report.stderr ?? ""
                )
            }
            self.extrasStageDryRunFingerprint = try self.extrasStageFingerprint(from: report, paths: paths)
            self.latestReport = report
            self.state = .dryRun
        }
    }

    /// Stage selected ExtData groups after re-running the read-only planner.
    /// The backend refuses pre-existing component files in the staging output,
    /// so this operation never silently replaces an existing staged set.
    public func stageExtras() async throws {
        try requireIdleForIndependentOperation()
        let fingerprint = try currentAuthorizedExtrasStageFingerprint()
        let paths = try requiredExtraPaths()
        try await withOperation(.convertExtras) { lease in
            let verification = try await self.execute(
                .convertExtras,
                arguments: self.extrasStageArguments(paths: paths, write: false),
                lease: lease
            )
            let verifiedFingerprint = try self.extrasStageFingerprint(from: verification, paths: paths)
            guard verification.status == "dry-run", verifiedFingerprint == fingerprint else {
                self.extrasStageDryRunFingerprint = nil
                throw self.failureAndRethrow(
                    .convertExtras,
                    ConversionWorkflowError.invalidReport("ExtData source changed after Dry Run"),
                    stderr: verification.stderr ?? ""
                )
            }
            let report = try await self.execute(
                .convertExtras,
                arguments: self.extrasStageArguments(paths: paths, write: true),
                lease: lease
            )
            try self.complete(with: report, expectedStatus: "written", operation: .convertExtras)
        }
    }

    /// Preview a replacement of the selected complete groups after staging.
    /// This is the authorization boundary for `installExtraGroups()` because
    /// it captures the exact staged and target group sets.
    public func runExtrasInstallDryRun() async throws {
        try requireIdleForIndependentOperation()
        let paths = try requiredExtraPaths()
        try await withOperation(.installExtras) { lease in
            self.extrasInstallDryRunFingerprint = nil
            let report = try await self.execute(
                .installExtras,
                arguments: self.extrasInstallArguments(paths: paths, write: false, fingerprint: nil),
                lease: lease
            )
            guard report.status == "dry-run",
                  let reportedGroups = report.groups,
                  Set(reportedGroups) == self.components.selectedGroups,
                  let stagingSetSHA256 = report.stagingSetSHA256,
                  ConverterEvidence.isValidSHA256(stagingSetSHA256),
                  let targetSetSHA256 = report.targetSetSHA256Before,
                  ConverterEvidence.isValidSHA256(targetSetSHA256),
                  ConverterEvidence.path(report.stagingDirectory, equals: paths.staging),
                  ConverterEvidence.path(report.targetDirectory, equals: paths.target),
                  let entries = report.entries,
                  self.validExtraInstallEntries(
                      entries,
                      groups: self.components.selectedGroups,
                      targetDirectory: paths.target
                  )
            else {
                throw self.failureAndRethrow(
                    .installExtras,
                    ConversionWorkflowError.invalidReport("ExtData install Dry Run requires selected groups and staging/target set SHA-256"),
                    stderr: report.stderr ?? ""
                )
            }
            self.extrasInstallDryRunFingerprint = ExtrasInstallDryRunFingerprint(
                stagingDirectory: paths.staging,
                targetDirectory: paths.target,
                groups: self.components.selectedGroups,
                stagingSetSHA256: stagingSetSHA256,
                targetSetSHA256: targetSetSHA256,
                entries: entries.map { $0.fingerprint() }
            )
            self.latestReport = report
            self.state = .dryRun
        }
    }

    /// Installs only the currently selected *whole* groups. The Rust
    /// transaction checks both set hashes while holding its target lock.
    public func installExtraGroups() async throws {
        try requireIdleForIndependentOperation()
        let fingerprint = try currentAuthorizedExtrasInstallFingerprint()
        let paths = try requiredExtraPaths()
        try await withOperation(.installExtras) { lease in
            let report = try await self.execute(
                .installExtras,
                arguments: self.extrasInstallArguments(paths: paths, write: true, fingerprint: fingerprint),
                lease: lease
            )
            try self.complete(with: report, expectedStatus: "written", operation: .installExtras)
        }
    }

    public func writeCEC() async throws {
        try requireIdleForIndependentOperation()
        guard components.includesCEC else { throw ConversionWorkflowError.missingCECDirectories }
        guard components.acknowledgeExperimentalCEC else { throw ConversionWorkflowError.experimentalCECAcknowledgementRequired }
        guard let source = components.cecSourceDirectory, let target = components.cecTarget else {
            throw ConversionWorkflowError.missingCECDirectories
        }
        let fingerprint = try currentAuthorizedCECFingerprint(source: source, target: target)
        try await withOperation(.convertCEC) { lease in
            let verification = try await self.execute(
                .convertCEC,
                arguments: [
                    ConverterOperation.convertCEC.rawValue,
                    "--source-dir", source.path,
                    "--target", target.path,
                    "--dry-run",
                ],
                lease: lease
            )
            guard verification.status == "dry-run",
                  verification.sourceRecordSetSHA256 == fingerprint.sourceRecordSetSHA256,
                  verification.targetSHA256Before == fingerprint.targetSHA256Before,
                  verification.targetSHA256After == fingerprint.targetSHA256After,
                  ConverterEvidence.path(verification.sourceDirectory, equals: source),
                  ConverterEvidence.path(verification.target, equals: target)
            else {
                self.cecDryRunFingerprint = nil
                throw self.failureAndRethrow(
                    .convertCEC,
                    ConversionWorkflowError.invalidReport("CEC mailbox or target changed after Dry Run"),
                    stderr: verification.stderr ?? ""
                )
            }
            let report = try await self.execute(
                .convertCEC,
                arguments: [
                    ConverterOperation.convertCEC.rawValue,
                    "--source-dir", source.path,
                    "--target", target.path,
                    "--expected-source-record-set-sha256", fingerprint.sourceRecordSetSHA256,
                    "--expected-target-sha256", fingerprint.targetSHA256Before,
                    "--write",
                    "--experimental",
                ],
                lease: lease
            )
            try self.complete(with: report, expectedStatus: "written", operation: .convertCEC)
        }
    }

    public func runCECDryRun() async throws {
        try requireIdleForIndependentOperation()
        guard components.includesCEC,
              let source = components.cecSourceDirectory,
              let target = components.cecTarget
        else { throw ConversionWorkflowError.missingCECDirectories }
        try await withOperation(.convertCEC) { lease in
            self.cecDryRunFingerprint = nil
            let report = try await self.execute(
                .convertCEC,
                arguments: [
                    ConverterOperation.convertCEC.rawValue,
                    "--source-dir", source.path,
                    "--target", target.path,
                    "--dry-run",
                ],
                lease: lease
            )
            guard report.status == "dry-run",
                  let sourceRecordSetSHA256 = report.sourceRecordSetSHA256,
                  ConverterEvidence.isValidSHA256(sourceRecordSetSHA256),
                  let targetSHA256Before = report.targetSHA256Before,
                  ConverterEvidence.isValidSHA256(targetSHA256Before),
                  let targetSHA256After = report.targetSHA256After,
                  ConverterEvidence.isValidSHA256(targetSHA256After),
                  let recordHashes = report.sourceRecordSHA256,
                  !recordHashes.isEmpty,
                  recordHashes.allSatisfy(ConverterEvidence.isValidSHA256),
                  ConverterEvidence.path(report.sourceDirectory, equals: source),
                  ConverterEvidence.path(report.target, equals: target)
            else {
                throw self.failureAndRethrow(
                    .convertCEC,
                    ConversionWorkflowError.invalidReport("CEC Dry Run requires source record-set and target SHA-256"),
                    stderr: report.stderr ?? ""
                )
            }
            self.cecDryRunFingerprint = CECDryRunFingerprint(
                sourceDirectory: source,
                target: target,
                sourceRecordSetSHA256: sourceRecordSetSHA256,
                targetSHA256Before: targetSHA256Before,
                targetSHA256After: targetSHA256After,
                targetExisted: FileManager.default.fileExists(atPath: target.path)
            )
            self.latestReport = report
            self.state = .dryRun
        }
    }

    public func rollback(
        manifest: URL,
        system: Bool = false,
        extraGroup: Bool = false,
        cec: Bool = false
    ) async throws {
        try requireIdleForIndependentOperation()
        let compatibilityRepair = !system
            && !extraGroup
            && !cec
            && manifest.lastPathComponent.hasPrefix(".mh3g-compatibility-repair-")
        let operation: ConverterOperation = cec
            ? .rollbackCEC
            : (extraGroup ? .rollbackExtras : (compatibilityRepair ? .rollbackRepair : .rollback))
        let authorizationScope: AuthorizationScope = cec ? .cec : (extraGroup ? .extras : (system ? .system : .core))
        try await withOperation(operation) { lease in
            let report = try await self.execute(
                operation,
                arguments: [operation.rawValue, "--manifest", manifest.path],
                lease: lease
            )
            try self.complete(
                with: report,
                expectedStatus: "rolled-back",
                operation: operation,
                scope: authorizationScope,
                rollbackManifest: manifest
            )
        }
    }

    /// A non-executing list of only the independently authorized writes. A
    /// staged ExtData install intentionally appears only after its install Dry
    /// Run; stage creation and target replacement cannot be authorized by one
    /// stale, catch-all plan.
    public func writePlan() throws -> [PlannedConverterCommand] {
        try requireIdleForIndependentOperation()
        var plan = [PlannedConverterCommand]()
        if let input, dryRunFingerprint != nil {
            let fingerprint = try currentAuthorizedFingerprint()
            plan.append(
                PlannedConverterCommand(
                    operation: .convert,
                    arguments: coreWriteArguments(input: input, fingerprint: fingerprint)
                )
            )
        }
        if components.includeSystem {
            guard let source = components.systemSource, let target = components.systemTarget else {
                throw ConversionWorkflowError.missingSystemPaths
            }
            let fingerprint = try currentAuthorizedSystemFingerprint(source: source, target: target)
            plan.append(
                PlannedConverterCommand(
                    operation: .convertSystem,
                    arguments: [
                        ConverterOperation.convertSystem.rawValue,
                        source.path,
                        "--output", target.path,
                        "--write",
                        "--expected-source-sha256", fingerprint.sourceSHA256,
                        "--expected-target-sha256", fingerprint.targetSHA256,
                    ]
                )
            )
        }
        if !components.selectedGroups.isEmpty {
            let paths = try requiredExtraPaths()
            let fingerprint = try currentAuthorizedExtrasInstallFingerprint()
            plan.append(
                PlannedConverterCommand(
                    operation: .installExtras,
                    arguments: extrasInstallArguments(paths: paths, write: true, fingerprint: fingerprint)
                )
            )
        }
        if components.includesCEC {
            guard components.acknowledgeExperimentalCEC else {
                throw ConversionWorkflowError.experimentalCECAcknowledgementRequired
            }
            guard let source = components.cecSourceDirectory, let target = components.cecTarget else {
                throw ConversionWorkflowError.missingCECDirectories
            }
            let fingerprint = try currentAuthorizedCECFingerprint(source: source, target: target)
            plan.append(
                PlannedConverterCommand(
                    operation: .convertCEC,
                    arguments: [
                        ConverterOperation.convertCEC.rawValue,
                        "--source-dir", source.path,
                        "--target", target.path,
                        "--expected-source-record-set-sha256", fingerprint.sourceRecordSetSHA256,
                        "--expected-target-sha256", fingerprint.targetSHA256Before,
                        "--write",
                        "--experimental",
                    ]
                )
            )
        }
        guard !plan.isEmpty else { throw ConversionWorkflowError.dryRunRequired }
        return plan
    }

    /// Kept internal and available to the unit target only.  It creates a
    /// deterministic authorization state without replacing the production
    /// Dry Run path with a fake UI-only bypass.
    func authorizeDryRunForTesting() throws {
        let current = try requireCurrentFingerprint()
        dryRunFingerprint = DryRunFingerprint(
            sourceSHA256: current.sourceSHA256,
            targetSHA256: current.targetSHA256,
            outputSHA256: String(repeating: "d", count: 64)
        )
        state = .dryRun
    }

    private func inspection(from report: ConverterReport) -> InputInspection? {
        guard let profile = report.profile,
              let size = report.size,
              let sha256 = report.hash(named: "source")
        else { return nil }
        return InputInspection(profile: profile, size: size, sha256: sha256)
    }

    private func requireCurrentFingerprint() throws -> DryRunFingerprint {
        guard let current = currentFingerprint() else { throw ConversionWorkflowError.inputNotInspected }
        return current
    }

    private func currentAuthorizedFingerprint() throws -> DryRunFingerprint {
        try requireIdleForIndependentOperation()
        guard let current = currentFingerprint(), let authorized = dryRunFingerprint else {
            throw ConversionWorkflowError.dryRunRequired
        }
        guard current.sourceSHA256 == authorized.sourceSHA256,
              current.targetSHA256 == authorized.targetSHA256
        else { throw ConversionWorkflowError.staleDryRun }
        return authorized
    }

    private func currentAuthorizedSystemFingerprint(source: URL, target: URL) throws -> SystemDryRunFingerprint {
        try requireIdleForIndependentOperation()
        guard let authorized = systemDryRunFingerprint else {
            throw ConversionWorkflowError.dryRunRequired
        }
        guard authorized.source == source.standardizedFileURL,
              authorized.target == target.standardizedFileURL
        else { throw ConversionWorkflowError.staleDryRun }
        return authorized
    }

    private func currentAuthorizedExtrasStageFingerprint() throws -> ExtrasStageDryRunFingerprint {
        try requireIdleForIndependentOperation()
        guard let authorized = extrasStageDryRunFingerprint else {
            throw ConversionWorkflowError.dryRunRequired
        }
        let paths = try requiredExtraPaths()
        guard authorized.sourceDirectory == paths.source.standardizedFileURL,
              authorized.stagingDirectory == paths.staging.standardizedFileURL,
              authorized.groups == components.selectedGroups
        else { throw ConversionWorkflowError.staleDryRun }
        return authorized
    }

    private func currentAuthorizedExtrasInstallFingerprint() throws -> ExtrasInstallDryRunFingerprint {
        try requireIdleForIndependentOperation()
        guard let authorized = extrasInstallDryRunFingerprint else {
            throw ConversionWorkflowError.dryRunRequired
        }
        let paths = try requiredExtraPaths()
        guard authorized.stagingDirectory == paths.staging.standardizedFileURL,
              authorized.targetDirectory == paths.target.standardizedFileURL,
              authorized.groups == components.selectedGroups
        else { throw ConversionWorkflowError.staleDryRun }
        return authorized
    }

    private func currentAuthorizedCECFingerprint(source: URL, target: URL) throws -> CECDryRunFingerprint {
        try requireIdleForIndependentOperation()
        guard let authorized = cecDryRunFingerprint,
              authorized.sourceDirectory == source.standardizedFileURL,
              authorized.target == target.standardizedFileURL
        else { throw ConversionWorkflowError.dryRunRequired }
        return authorized
    }

    private func requireIdleForIndependentOperation() throws {
        guard let activeOperation else { return }
        throw ConversionWorkflowError.operationInProgress(activeOperation)
    }

    private func currentFingerprint() -> DryRunFingerprint? {
        guard let sourceInspection else { return nil }
        return DryRunFingerprint(
            sourceSHA256: sourceInspection.sha256,
            targetSHA256: targetInspection?.sha256,
            outputSHA256: ""
        )
    }

    private func coreWriteArguments(input: ConversionInput, fingerprint: DryRunFingerprint) -> [String] {
        var arguments = [
            ConverterOperation.convert.rawValue,
            input.source.path,
            "--output", input.target.path,
            "--write",
            "--expected-source-sha256", fingerprint.sourceSHA256,
        ]
        if let targetSHA256 = fingerprint.targetSHA256 {
            arguments += ["--expected-target-sha256", targetSHA256]
        } else {
            arguments.append("--expected-target-absent")
        }
        return arguments
    }

    private func requiredExtraPaths() throws -> (source: URL, staging: URL, target: URL) {
        guard !components.selectedGroups.isEmpty,
              let source = components.extraSourceDirectory,
              let staging = components.extraStagingDirectory,
              let target = components.extraTargetDirectory
        else { throw ConversionWorkflowError.missingExtraDirectories }
        return (source, staging, target)
    }

    private func requireSelectedOptionalDataConfiguration() throws {
        guard !components.includeSystem
                || (components.systemSource != nil && components.systemTarget != nil)
        else { throw ConversionWorkflowError.missingSystemPaths }
        guard selectedOptionalDataIsConfigured else {
            throw ConversionWorkflowError.missingExtraDirectories
        }
    }

    private func extrasStageArguments(
        paths: (source: URL, staging: URL, target: URL),
        write: Bool
    ) -> [String] {
        [
            ConverterOperation.convertExtras.rawValue,
            "--source-dir", paths.source.path,
            "--output-dir", paths.staging.path,
            write ? "--write" : "--dry-run",
        ]
    }

    private func extrasInstallArguments(
        paths: (source: URL, staging: URL, target: URL),
        write: Bool,
        fingerprint: ExtrasInstallDryRunFingerprint?
    ) -> [String] {
        let groups = components.selectedGroups
            .sorted { $0.rawValue < $1.rawValue }
            .map(\.rawValue)
            .joined(separator: ",")
        var arguments = [
            ConverterOperation.installExtras.rawValue,
            "--staging-dir", paths.staging.path,
            "--target-dir", paths.target.path,
            "--groups", groups,
        ]
        if let fingerprint {
            arguments += [
                "--expected-staging-set-sha256", fingerprint.stagingSetSHA256,
                "--expected-target-set-sha256", fingerprint.targetSetSHA256,
            ]
        }
        arguments.append(write ? "--write" : "--dry-run")
        return arguments
    }

    private func extrasStageFingerprint(
        from report: ConverterReport,
        paths: (source: URL, staging: URL, target: URL)
    ) throws -> ExtrasStageDryRunFingerprint {
        guard let components = report.components, !components.isEmpty else {
            throw failureAndRethrow(
                .convertExtras,
                ConversionWorkflowError.invalidReport("ExtData stage Dry Run requires converted component fingerprints"),
                stderr: report.stderr ?? ""
            )
        }
        let fingerprints = components.compactMap { $0.fingerprint() }
        guard fingerprints.count == components.count,
              components.allSatisfy({ component in
                  ConverterEvidence.isValidSHA256(component.sourceSHA256)
                      && ConverterEvidence.isValidSHA256(component.outputSHA256)
                      && ConverterEvidence.hasPath(component.output)
                      && (component.size ?? 0) > 0
              }),
              ConverterEvidence.path(report.sourceDirectory, equals: paths.source),
              ConverterEvidence.path(report.outputDirectory, equals: paths.staging)
        else {
            throw failureAndRethrow(
                .convertExtras,
                ConversionWorkflowError.invalidReport("ExtData stage components are missing source/output fingerprints"),
                stderr: report.stderr ?? ""
            )
        }
        return ExtrasStageDryRunFingerprint(
            sourceDirectory: paths.source,
            stagingDirectory: paths.staging,
            groups: self.components.selectedGroups,
            components: fingerprints
        )
    }

    private func withOperation<T>(
        _ operation: ConverterOperation,
        _ body: (UUID) async throws -> T
    ) async throws -> T {
        try requireIdleForIndependentOperation()
        let lease = UUID()
        activeOperation = operation
        activeOperationLease = lease
        failure = nil
        state = .writing
        defer {
            if activeOperationLease == lease {
                activeOperation = nil
                activeOperationLease = nil
            }
        }
        return try await body(lease)
    }

    private func execute(
        _ operation: ConverterOperation,
        arguments: [String],
        lease: UUID
    ) async throws -> ConverterReport {
        guard let activeOperation else {
            throw ConversionWorkflowError.invalidReport("operation lock missing")
        }
        guard activeOperation == operation, activeOperationLease == lease else {
            throw ConversionWorkflowError.operationInProgress(activeOperation)
        }
        do {
            let result = try await executor.run(ConverterCommand(executable: executable, arguments: arguments))
            guard result.exitCode == 0 else {
                throw ConversionWorkflowError.commandFailed(
                    operation: operation,
                    stderr: String(decoding: result.stderr, as: UTF8.self)
                )
            }
            do {
                return try JSONDecoder().decode(ConverterReport.self, from: result.stdout)
            } catch {
                throw ConversionWorkflowError.invalidReport(error.localizedDescription)
            }
        } catch let error as ConversionWorkflowError {
            throw failureAndRethrow(operation, error, stderr: stderr(from: error))
        } catch let error as ConverterCommandError {
            let stderr: String
            if case let .failed(_, value) = error { stderr = value } else { stderr = error.localizedDescription }
            throw failureAndRethrow(operation, error, stderr: stderr)
        } catch {
            throw failureAndRethrow(operation, error, stderr: error.localizedDescription)
        }
    }

    private func complete(
        with report: ConverterReport,
        expectedStatus: String,
        operation: ConverterOperation,
        scope: AuthorizationScope? = nil,
        rollbackManifest: URL? = nil
    ) throws {
        guard report.status == expectedStatus else {
            throw failureAndRethrow(
                operation,
                ConversionWorkflowError.invalidReport("expected \(expectedStatus) status"),
                stderr: report.stderr ?? ""
            )
        }
        do {
            try validateCompletionEvidence(
                report,
                operation: operation,
                rollbackManifest: rollbackManifest
            )
        } catch {
            throw failureAndRethrow(
                operation,
                error,
                stderr: report.stderr ?? "",
                scope: scope
            )
        }
        latestReport = report
        state = .success
        let completionScope = scope ?? authorizationScope(for: operation)
        recordCompletion(expectedStatus: expectedStatus, operation: operation, scope: completionScope)
        invalidateAuthorization(in: completionScope)
    }

    private func completeRepair(
        with report: ConverterReport,
        fingerprint: RepairDryRunFingerprint,
        input: ConversionInput
    ) throws {
        guard report.status == "written" || report.status == "no-changes" else {
            throw failureAndRethrow(
                .repairConverted,
                ConversionWorkflowError.invalidReport("expected written or no-changes status"),
                stderr: report.stderr ?? ""
            )
        }
        do {
            guard let inputCurrent = input.current,
                  report.operation == ConverterOperation.repairConverted.rawValue,
                  ConverterEvidence.path(report.source, equals: input.source),
                  ConverterEvidence.path(report.current, equals: inputCurrent),
                  ConverterEvidence.path(report.output, equals: input.target),
                  report.sourceSetSHA256 == fingerprint.sourceSetSHA256,
                  report.currentSetSHA256 == fingerprint.currentSetSHA256,
                  report.outputSetSHA256 == fingerprint.outputSetSHA256,
                  report.previewSHA256 == fingerprint.previewSHA256,
                  ConverterEvidence.isValidSHA256(report.sourceSetSHA256),
                  ConverterEvidence.isValidSHA256(report.currentSetSHA256),
                  ConverterEvidence.isValidSHA256(report.outputSetSHA256),
                  ConverterEvidence.isValidSHA256(report.previewSHA256),
                  let components = report.components,
                  !components.isEmpty,
                  components.compactMap({ $0.repairFingerprint() }) == fingerprint.components
            else {
                throw ConversionWorkflowError.invalidReport(
                    "repair completion requires exact source/current/output paths, set hashes, preview hash, and components"
                )
            }
            if report.status == "written" {
                guard let manifests = report.manifests,
                      !manifests.isEmpty,
                      manifests.allSatisfy(ConverterEvidence.hasPath),
                      ConverterEvidence.hasPath(report.compatibilityManifest)
                else {
                    throw ConversionWorkflowError.invalidReport(
                        "written repair requires component manifests and a compatibility manifest"
                    )
                }
            } else {
                guard report.manifests?.isEmpty != false,
                      !ConverterEvidence.hasPath(report.compatibilityManifest)
                else {
                    throw ConversionWorkflowError.invalidReport(
                        "no-changes repair must not claim write manifests"
                    )
                }
            }
        } catch {
            throw failureAndRethrow(
                .repairConverted,
                error,
                stderr: report.stderr ?? "",
                scope: .core
            )
        }
        latestReport = report
        state = .success
        coreWriteCompleted = true
        invalidateAuthorization(in: .core)
    }

    private func validateCompletionEvidence(
        _ report: ConverterReport,
        operation: ConverterOperation,
        rollbackManifest: URL?
    ) throws {
        switch operation {
        case .convert:
            guard let input, let fingerprint = dryRunFingerprint else {
                throw ConversionWorkflowError.invalidReport("core write authorization is missing")
            }
            try validateFileWriteReport(
                report,
                output: input.target,
                sourceSHA256: fingerprint.sourceSHA256,
                targetSHA256Before: fingerprint.targetSHA256,
                outputSHA256: fingerprint.outputSHA256
            )
        case .convertSystem:
            guard let fingerprint = systemDryRunFingerprint else {
                throw ConversionWorkflowError.invalidReport("system write authorization is missing")
            }
            try validateFileWriteReport(
                report,
                output: fingerprint.target,
                sourceSHA256: fingerprint.sourceSHA256,
                targetSHA256Before: fingerprint.targetSHA256,
                outputSHA256: fingerprint.outputSHA256
            )
        case .convertExtras:
            try validateExtrasStageWrite(report)
        case .installExtras:
            try validateExtrasInstallWrite(report)
        case .convertCEC:
            try validateCECWrite(report)
        case .rollback, .rollbackRepair, .rollbackExtras, .rollbackCEC:
            try validateRollbackReport(report, operation: operation, manifest: rollbackManifest)
        case .inspect, .repairConverted:
            throw ConversionWorkflowError.invalidReport("unsupported completion operation")
        }
    }

    private func validateFileWriteReport(
        _ report: ConverterReport,
        output: URL,
        sourceSHA256: String,
        targetSHA256Before: String?,
        outputSHA256: String
    ) throws {
        guard ConverterEvidence.path(report.output, equals: output),
              ConverterEvidence.hasPath(report.manifest),
              report.hash(named: "source") == sourceSHA256,
              report.hash(named: "output") == outputSHA256,
              ConverterEvidence.isValidSHA256(report.hash(named: "source")),
              ConverterEvidence.isValidSHA256(report.hash(named: "output"))
        else {
            throw ConversionWorkflowError.invalidReport(
                "written file requires exact output path, manifest, source hash, and output hash"
            )
        }
        if let targetSHA256Before {
            guard report.hash(named: "target_before") == targetSHA256Before,
                  ConverterEvidence.isValidSHA256(report.hash(named: "target_before")),
                  ConverterEvidence.hasPath(report.backup)
            else {
                throw ConversionWorkflowError.invalidReport(
                    "replacing an existing target requires its exact before hash and backup"
                )
            }
        } else {
            guard report.hash(named: "target_before") == nil,
                  !ConverterEvidence.hasPath(report.backup)
            else {
                throw ConversionWorkflowError.invalidReport(
                    "new export must not claim an existing-target hash or backup"
                )
            }
        }
    }

    private func validateExtrasStageWrite(_ report: ConverterReport) throws {
        guard let fingerprint = extrasStageDryRunFingerprint,
              ConverterEvidence.path(report.sourceDirectory, equals: fingerprint.sourceDirectory),
              ConverterEvidence.path(report.outputDirectory, equals: fingerprint.stagingDirectory),
              let components = report.components,
              !components.isEmpty,
              components.allSatisfy({ component in
                  ConverterEvidence.isValidSHA256(component.sourceSHA256)
                      && ConverterEvidence.isValidSHA256(component.outputSHA256)
                      && ConverterEvidence.hasPath(component.output)
                      && (component.size ?? 0) > 0
              }),
              components.compactMap({ $0.fingerprint() }) == fingerprint.components
        else {
            throw ConversionWorkflowError.invalidReport(
                "ExtData staging requires exact source/output directories and component output evidence"
            )
        }
    }

    private func validateExtrasInstallWrite(_ report: ConverterReport) throws {
        guard let fingerprint = extrasInstallDryRunFingerprint,
              report.operation == ConverterOperation.installExtras.rawValue,
              Set(report.groups ?? []) == fingerprint.groups,
              ConverterEvidence.hasPath(report.manifest),
              ConverterEvidence.path(report.stagingDirectory, equals: fingerprint.stagingDirectory),
              ConverterEvidence.path(report.targetDirectory, equals: fingerprint.targetDirectory),
              report.stagingSetSHA256 == fingerprint.stagingSetSHA256,
              report.targetSetSHA256Before == fingerprint.targetSetSHA256,
              ConverterEvidence.isValidSHA256(report.stagingSetSHA256),
              ConverterEvidence.isValidSHA256(report.targetSetSHA256Before),
              let entries = report.entries,
              !entries.isEmpty
        else {
            throw ConversionWorkflowError.invalidReport(
                "ExtData install requires exact groups, directories, set hashes, manifest, and entries"
            )
        }
        let expectedComponents = Set(fingerprint.groups.flatMap(\.componentNames))
        guard Set(entries.map(\.component)) == expectedComponents,
              entries.count == expectedComponents.count,
              Set(entries.map(\.group)) == fingerprint.groups,
              validExtraInstallEntries(
                  entries,
                  groups: fingerprint.groups,
                  targetDirectory: fingerprint.targetDirectory
              ),
              entries.map({ $0.fingerprint() }) == fingerprint.entries
        else {
            throw ConversionWorkflowError.invalidReport(
                "ExtData install entries do not prove every selected component replacement"
            )
        }
        let reportedBackups = Set((report.backupPaths ?? []).filter(ConverterEvidence.hasPath))
        let entryBackups = Set(entries.compactMap(\.backup).filter(ConverterEvidence.hasPath))
        guard report.backupPaths?.count == entryBackups.count,
              reportedBackups.count == entryBackups.count,
              reportedBackups == entryBackups
        else {
            throw ConversionWorkflowError.invalidReport("ExtData backup list does not match entry evidence")
        }
    }

    private func validExtraInstallEntries(
        _ entries: [ConverterExtraInstallEntry],
        groups: Set<ExtraGroup>,
        targetDirectory: URL
    ) -> Bool {
        let expectedPairs = Set(groups.flatMap { group in
            group.componentNames.map { "\(group.rawValue):\($0)" }
        })
        let reportedPairs = entries.map { "\($0.group.rawValue):\($0.component)" }
        guard entries.count == expectedPairs.count,
              Set(reportedPairs) == expectedPairs,
              Set(reportedPairs).count == reportedPairs.count
        else { return false }
        return entries.allSatisfy { entry in
            let target = URL(fileURLWithPath: entry.target).standardizedFileURL
            // URL equality includes a directory/file resource hint. For a
            // target directory that does not exist yet, Foundation may omit
            // the trailing directory marker while `deletingLastPathComponent`
            // always includes it. Compare normalized filesystem paths so the
            // fail-closed evidence check does not depend on host filesystem
            // state.
            let targetMatches = target.deletingLastPathComponent().path
                == targetDirectory.standardizedFileURL.path
                && target.lastPathComponent == entry.component
            return targetMatches
                && ConverterEvidence.isValidSHA256(entry.afterSHA256)
                && entry.targetPreviouslyExisted
                && ConverterEvidence.isValidSHA256(entry.beforeSHA256)
                && ConverterEvidence.hasPath(entry.backup)
        }
    }

    private func validateCECWrite(_ report: ConverterReport) throws {
        guard let fingerprint = cecDryRunFingerprint,
              ConverterEvidence.path(report.sourceDirectory, equals: fingerprint.sourceDirectory),
              ConverterEvidence.path(report.target, equals: fingerprint.target),
              report.sourceRecordSetSHA256 == fingerprint.sourceRecordSetSHA256,
              report.targetSHA256Before == fingerprint.targetSHA256Before,
              report.targetSHA256After == fingerprint.targetSHA256After,
              ConverterEvidence.isValidSHA256(report.sourceRecordSetSHA256),
              ConverterEvidence.isValidSHA256(report.targetSHA256Before),
              ConverterEvidence.isValidSHA256(report.targetSHA256After),
              let recordHashes = report.sourceRecordSHA256,
              !recordHashes.isEmpty,
              recordHashes.allSatisfy(ConverterEvidence.isValidSHA256),
              ConverterEvidence.hasPath(report.manifest),
              !fingerprint.targetExisted || ConverterEvidence.hasPath(report.backup)
        else {
            throw ConversionWorkflowError.invalidReport(
                "CEC write requires exact mailbox/target hashes, manifest, and conditional backup evidence"
            )
        }
    }

    private func validateRollbackReport(
        _ report: ConverterReport,
        operation: ConverterOperation,
        manifest: URL?
    ) throws {
        guard let manifest,
              ConverterEvidence.path(report.manifest, equals: manifest)
        else {
            throw ConversionWorkflowError.invalidReport("rollback must echo the exact recovery manifest")
        }
        switch operation {
        case .rollbackRepair:
            guard report.operation == operation.rawValue else {
                throw ConversionWorkflowError.invalidReport("repair rollback operation evidence is missing")
            }
        case .rollbackExtras:
            guard report.operation == operation.rawValue,
                  let groups = report.groups, !groups.isEmpty,
                  let entries = report.entries, !entries.isEmpty,
                  validExtraRollbackEntries(entries, groups: Set(groups))
            else {
                throw ConversionWorkflowError.invalidReport(
                    "ExtData rollback requires operation, groups, and restored entry evidence"
                )
            }
        case .rollback, .rollbackCEC:
            break
        case .inspect, .convert, .repairConverted, .convertSystem, .convertExtras, .installExtras, .convertCEC:
            throw ConversionWorkflowError.invalidReport("unsupported rollback operation")
        }
    }

    private func validExtraRollbackEntries(
        _ entries: [ConverterExtraInstallEntry],
        groups: Set<ExtraGroup>
    ) -> Bool {
        let expectedPairs = Set(groups.flatMap { group in
            group.componentNames.map { "\(group.rawValue):\($0)" }
        })
        let reportedPairs = entries.map { "\($0.group.rawValue):\($0.component)" }
        guard entries.count == expectedPairs.count,
              Set(reportedPairs) == expectedPairs,
              Set(reportedPairs).count == reportedPairs.count
        else { return false }
        let targetParents = Set(entries.map {
            URL(fileURLWithPath: $0.target).standardizedFileURL.deletingLastPathComponent()
        })
        return targetParents.count == 1 && entries.allSatisfy { entry in
            let target = URL(fileURLWithPath: entry.target).standardizedFileURL
            return target.lastPathComponent == entry.component
                && ConverterEvidence.isValidSHA256(entry.afterSHA256)
                && entry.targetPreviouslyExisted
                && ConverterEvidence.isValidSHA256(entry.beforeSHA256)
                && ConverterEvidence.hasPath(entry.backup)
        }
    }

    private enum AuthorizationScope {
        case core
        case system
        case extras
        case cec
    }

    private func invalidateCoreAuthorization(nextState: WorkflowState, clearsPresentation: Bool) {
        dryRunFingerprint = nil
        repairDryRunFingerprint = nil
        repairRevisionCandidates = []
        repairRevisionSelectionRequired = false
        if clearsPresentation {
            latestReport = nil
            failure = nil
        }
        state = nextState
    }

    private func invalidateAuthorization(in scope: AuthorizationScope) {
        switch scope {
        case .core:
            dryRunFingerprint = nil
            repairDryRunFingerprint = nil
        case .system:
            systemDryRunFingerprint = nil
        case .extras:
            extrasStageDryRunFingerprint = nil
            extrasInstallDryRunFingerprint = nil
        case .cec:
            cecDryRunFingerprint = nil
        }
    }

    private func recordCompletion(
        expectedStatus: String,
        operation: ConverterOperation,
        scope: AuthorizationScope
    ) {
        switch expectedStatus {
        case "written":
            switch operation {
            case .convert, .repairConverted:
                coreWriteCompleted = true
            case .convertSystem:
                systemWriteCompleted = true
            case .convertExtras:
                break
            case .installExtras:
                extrasInstallCompleted = true
            case .convertCEC:
                break
            case .inspect, .rollback, .rollbackRepair, .rollbackExtras, .rollbackCEC:
                break
            }
        case "rolled-back":
            switch scope {
            case .core:
                coreWriteCompleted = false
            case .system:
                systemWriteCompleted = false
            case .extras:
                extrasInstallCompleted = false
            case .cec:
                break
            }
        default:
            break
        }
    }

    private func authorizationScope(for operation: ConverterOperation) -> AuthorizationScope {
        switch operation {
        case .inspect, .convert, .repairConverted, .rollback, .rollbackRepair:
            .core
        case .convertSystem:
            .system
        case .convertExtras, .installExtras, .rollbackExtras:
            .extras
        case .convertCEC, .rollbackCEC:
            .cec
        }
    }

    private func failureAndRethrow(
        _ operation: ConverterOperation,
        _ error: Error,
        stderr: String,
        scope: AuthorizationScope? = nil
    ) -> Error {
        // A failed operation may have raced a file-system change or failed in
        // the converter after a partial transaction. Revoke only that
        // operation's authorization so independent data domains stay intact.
        invalidateAuthorization(in: scope ?? authorizationScope(for: operation))
        failure = WorkflowFailure(operation: operation, message: error.localizedDescription, stderr: stderr)
        state = .failure
        return error
    }

    private func stderr(from error: ConversionWorkflowError) -> String {
        if case let .commandFailed(_, stderr) = error { return stderr }
        return ""
    }
}
