import Foundation
import Observation

@MainActor
@Observable
public final class ConversionWorkflow {
    public private(set) var state: WorkflowState = .input
    public private(set) var input: ConversionInput?
    public private(set) var sourceInspection: InputInspection?
    public private(set) var targetInspection: InputInspection?
    public private(set) var components = ComponentSelection()
    public private(set) var dryRunFingerprint: DryRunFingerprint?
    public private(set) var systemDryRunFingerprint: SystemDryRunFingerprint?
    public private(set) var extrasStageDryRunFingerprint: ExtrasStageDryRunFingerprint?
    public private(set) var extrasInstallDryRunFingerprint: ExtrasInstallDryRunFingerprint?
    public private(set) var cecDryRunFingerprint: CECDryRunFingerprint?
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
        input != nil && sourceInspection != nil && targetInspection != nil && activeOperation == nil
    }

    /// This is intentionally stricter than the button's visual disabled state.
    /// Every write path re-checks its own current authorization before
    /// constructing argv, so programmatic UI calls cannot bypass the guard.
    public var canWrite: Bool {
        guard activeOperation == nil,
              let authorized = dryRunFingerprint,
              let current = currentFingerprint()
        else { return false }
        return authorized == current
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
        targetInspection = nil
        invalidateCoreAuthorization(nextState: .input, clearsPresentation: true)
    }

    public func applyInspections(source: InputInspection, target: InputInspection) {
        guard activeOperation == nil else { return }
        sourceInspection = source
        targetInspection = target
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
        if systemChanged { systemDryRunFingerprint = nil }
        if extrasChanged {
            extrasStageDryRunFingerprint = nil
            extrasInstallDryRunFingerprint = nil
        }
        if cecChanged { cecDryRunFingerprint = nil }
        failure = nil
        state = input == nil ? .input : .componentSelection
    }

    /// Calls the Rust `inspect` command for the two exact files selected in the
    /// open panel.  It deliberately has no directory discovery mode.
    public func inspectInputs() async throws {
        try requireIdleForIndependentOperation()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        try await withOperation(.inspect) { lease in
            let sourceReport = try await self.execute(
                .inspect,
                arguments: [ConverterOperation.inspect.rawValue, input.source.path],
                lease: lease
            )
            let targetReport = try await self.execute(
                .inspect,
                arguments: [ConverterOperation.inspect.rawValue, input.target.path],
                lease: lease
            )
            guard let source = self.inspection(from: sourceReport), let target = self.inspection(from: targetReport) else {
                throw self.failureAndRethrow(
                    .inspect,
                    ConversionWorkflowError.invalidReport("inspect requires profile, size, and source SHA-256"),
                    stderr: ""
                )
            }
            self.sourceInspection = source
            self.targetInspection = target
            self.invalidateCoreAuthorization(nextState: .componentSelection, clearsPresentation: false)
        }
    }

    /// Core slot Dry Run is the authorization boundary for `writeCore` only.
    /// `system` is a separate source/target pair with its own authorization.
    public func runCoreDryRun() async throws {
        try requireIdleForIndependentOperation()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        guard sourceInspection != nil, targetInspection != nil else { throw ConversionWorkflowError.inputNotInspected }
        try await withOperation(.convert) { lease in
            self.dryRunFingerprint = nil
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
                  let reportedTarget = report.hash(named: "target_before")
            else {
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("Dry Run requires source and target_before SHA-256"),
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
            if reportedTarget != current.targetSHA256 {
                throw self.failureAndRethrow(
                    .convert,
                    ConversionWorkflowError.invalidReport("target SHA-256 changed during Dry Run"),
                    stderr: report.stderr ?? ""
                )
            }
            self.dryRunFingerprint = current
            self.latestReport = report
            self.state = .dryRun
        }
    }

    public func writeCore() async throws {
        try requireIdleForIndependentOperation()
        let fingerprint = try currentAuthorizedFingerprint()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        try await withOperation(.convert) { lease in
            let report = try await self.execute(
                .convert,
                arguments: [
                    ConverterOperation.convert.rawValue,
                    input.source.path,
                    "--output", input.target.path,
                    "--write",
                    "--expected-source-sha256", fingerprint.sourceSHA256,
                    "--expected-target-sha256", fingerprint.targetSHA256,
                ],
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
                  let targetSHA256 = report.hash(named: "target_before")
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
                targetSHA256: targetSHA256
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
                  let targetSetSHA256 = report.targetSetSHA256Before
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
                targetSetSHA256: targetSetSHA256
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
                  verification.targetSHA256Before == fingerprint.targetSHA256Before
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
                  !sourceRecordSetSHA256.isEmpty,
                  let targetSHA256Before = report.targetSHA256Before
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
                targetSHA256Before: targetSHA256Before
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
        let operation: ConverterOperation = cec ? .rollbackCEC : (extraGroup ? .rollbackExtras : .rollback)
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
                scope: authorizationScope
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
                    arguments: [
                        ConverterOperation.convert.rawValue,
                        input.source.path,
                        "--output", input.target.path,
                        "--write",
                        "--expected-source-sha256", fingerprint.sourceSHA256,
                        "--expected-target-sha256", fingerprint.targetSHA256,
                    ]
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
        dryRunFingerprint = try requireCurrentFingerprint()
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
        guard current == authorized else { throw ConversionWorkflowError.staleDryRun }
        return current
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
        guard let sourceInspection, let targetInspection else { return nil }
        return DryRunFingerprint(
            sourceSHA256: sourceInspection.sha256,
            targetSHA256: targetInspection.sha256
        )
    }

    private func requiredExtraPaths() throws -> (source: URL, staging: URL, target: URL) {
        guard !components.selectedGroups.isEmpty,
              let source = components.extraSourceDirectory,
              let staging = components.extraStagingDirectory,
              let target = components.extraTargetDirectory
        else { throw ConversionWorkflowError.missingExtraDirectories }
        return (source, staging, target)
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
        return ExtrasStageDryRunFingerprint(
            sourceDirectory: paths.source,
            stagingDirectory: paths.staging,
            groups: self.components.selectedGroups,
            components: components.map { $0.fingerprint() }
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
        scope: AuthorizationScope? = nil
    ) throws {
        guard report.status == expectedStatus else {
            throw failureAndRethrow(
                operation,
                ConversionWorkflowError.invalidReport("expected \(expectedStatus) status"),
                stderr: report.stderr ?? ""
            )
        }
        latestReport = report
        state = .success
        invalidateAuthorization(in: scope ?? authorizationScope(for: operation))
    }

    private enum AuthorizationScope {
        case core
        case system
        case extras
        case cec
    }

    private func invalidateCoreAuthorization(nextState: WorkflowState, clearsPresentation: Bool) {
        dryRunFingerprint = nil
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
        case .system:
            systemDryRunFingerprint = nil
        case .extras:
            extrasStageDryRunFingerprint = nil
            extrasInstallDryRunFingerprint = nil
        case .cec:
            cecDryRunFingerprint = nil
        }
    }

    private func authorizationScope(for operation: ConverterOperation) -> AuthorizationScope {
        switch operation {
        case .inspect, .convert, .rollback:
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
