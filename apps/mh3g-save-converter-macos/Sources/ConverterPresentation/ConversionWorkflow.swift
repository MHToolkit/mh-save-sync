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
    public private(set) var failure: WorkflowFailure?
    public private(set) var latestReport: ConverterReport?
    public private(set) var activeOperation: ConverterOperation?

    private let executable: URL
    private let executor: any ConverterCommandExecuting

    public init(executable: URL, executor: any ConverterCommandExecuting = ConverterCommandClient()) {
        self.executable = executable.standardizedFileURL
        self.executor = executor
    }

    public var canStartDryRun: Bool {
        input != nil && sourceInspection != nil && targetInspection != nil && activeOperation == nil
    }

    /// This is intentionally stricter than the button's visual disabled state.
    /// Every write method calls `currentAuthorizedFingerprint()` again before
    /// constructing argv, so programmatic UI calls cannot bypass the guard.
    public var canWrite: Bool {
        guard activeOperation == nil,
              let authorized = dryRunFingerprint,
              let current = currentFingerprint()
        else { return false }
        return authorized == current
    }

    public func configure(input: ConversionInput) {
        guard self.input != input else { return }
        self.input = input
        sourceInspection = nil
        targetInspection = nil
        invalidateAuthorization(nextState: .input)
    }

    public func applyInspections(source: InputInspection, target: InputInspection) {
        sourceInspection = source
        targetInspection = target
        invalidateAuthorization(nextState: .componentSelection)
    }

    public func setComponents(_ selection: ComponentSelection) {
        guard components != selection else { return }
        components = selection
        invalidateAuthorization(nextState: input == nil ? .input : .componentSelection)
    }

    /// Calls the Rust `inspect` command for the two exact files selected in the
    /// open panel.  It deliberately has no directory discovery mode.
    public func inspectInputs() async throws {
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        let sourceReport = try await execute(
            .inspect,
            arguments: [ConverterOperation.inspect.rawValue, input.source.path]
        )
        let targetReport = try await execute(
            .inspect,
            arguments: [ConverterOperation.inspect.rawValue, input.target.path]
        )
        guard let source = inspection(from: sourceReport), let target = inspection(from: targetReport) else {
            throw failureAndRethrow(
                .inspect,
                ConversionWorkflowError.invalidReport("inspect requires profile, size, and source SHA-256"),
                stderr: ""
            )
        }
        applyInspections(source: source, target: target)
    }

    /// Core slot Dry Run is the authorization boundary for `writeCore` only.
    /// Optional sources use distinct file or directory inputs and must never
    /// inherit the user-slot fingerprint as their hash precondition.
    public func runCoreDryRun() async throws {
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        guard sourceInspection != nil, targetInspection != nil else { throw ConversionWorkflowError.inputNotInspected }
        let report = try await execute(
            .convert,
            arguments: [
                ConverterOperation.convert.rawValue,
                input.source.path,
                "--output", input.target.path,
                "--dry-run",
            ]
        )
        guard report.status == "dry-run" else {
            throw failureAndRethrow(
                .convert,
                ConversionWorkflowError.invalidReport("expected dry-run status"),
                stderr: report.stderr ?? ""
            )
        }
        let current = try requireCurrentFingerprint()
        if let reportedSource = report.hash(named: "source"), reportedSource != current.sourceSHA256 {
            throw failureAndRethrow(
                .convert,
                ConversionWorkflowError.invalidReport("source SHA-256 changed during Dry Run"),
                stderr: report.stderr ?? ""
            )
        }
        if let reportedTarget = report.hash(named: "target_before"), reportedTarget != current.targetSHA256 {
            throw failureAndRethrow(
                .convert,
                ConversionWorkflowError.invalidReport("target SHA-256 changed during Dry Run"),
                stderr: report.stderr ?? ""
            )
        }
        dryRunFingerprint = current
        latestReport = report
        state = .dryRun
    }

    public func writeCore() async throws {
        let fingerprint = try currentAuthorizedFingerprint()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        let report = try await execute(
            .convert,
            arguments: [
                ConverterOperation.convert.rawValue,
                input.source.path,
                "--output", input.target.path,
                "--write",
                "--expected-source-sha256", fingerprint.sourceSHA256,
                "--expected-target-sha256", fingerprint.targetSHA256,
            ]
        )
        complete(with: report)
    }

    public func writeSystem() async throws {
        try requireIdleForOptionalWrite()
        guard components.includeSystem,
              let source = components.systemSource,
              let target = components.systemTarget
        else { throw ConversionWorkflowError.missingSystemPaths }
        let report = try await execute(
            .convertSystem,
            arguments: [
                ConverterOperation.convertSystem.rawValue,
                source.path,
                "--output", target.path,
                "--write",
            ]
        )
        complete(with: report)
    }

    /// Stage selected ExtData groups.  The Rust CLI validates that all eight
    /// components are valid and only allows installation as a full named group.
    public func stageExtras() async throws {
        _ = try currentAuthorizedFingerprint()
        let paths = try requiredExtraPaths()
        let report = try await execute(
            .convertExtras,
            arguments: [
                ConverterOperation.convertExtras.rawValue,
                "--source-dir", paths.source.path,
                "--output-dir", paths.staging.path,
                "--write",
            ]
        )
        complete(with: report)
    }

    /// Installs only the currently selected *whole* groups.  The backend owns
    /// target-set hashes and will reject a staging or target change after its
    /// own Dry Run; this presentation layer never supplies card#/quest# paths.
    public func installExtraGroups(stagingSetSHA256: String, targetSetSHA256: String) async throws {
        _ = try currentAuthorizedFingerprint()
        let paths = try requiredExtraPaths()
        let groups = components.selectedGroups.sorted { $0.rawValue < $1.rawValue }.map(\.rawValue).joined(separator: ",")
        let report = try await execute(
            .installExtras,
            arguments: [
                ConverterOperation.installExtras.rawValue,
                "--staging-dir", paths.staging.path,
                "--target-dir", paths.target.path,
                "--groups", groups,
                "--write",
                "--expected-staging-set-sha256", stagingSetSHA256,
                "--expected-target-set-sha256", targetSetSHA256,
            ]
        )
        complete(with: report)
    }

    public func writeCEC() async throws {
        try requireIdleForOptionalWrite()
        guard components.includesCEC else { throw ConversionWorkflowError.missingCECDirectories }
        guard components.acknowledgeExperimentalCEC else { throw ConversionWorkflowError.experimentalCECAcknowledgementRequired }
        guard let source = components.cecSourceDirectory, let target = components.cecTarget else {
            throw ConversionWorkflowError.missingCECDirectories
        }
        let report = try await execute(
            .convertCEC,
            arguments: [
                ConverterOperation.convertCEC.rawValue,
                "--source-dir", source.path,
                "--target", target.path,
                "--write",
                "--experimental",
            ]
        )
        complete(with: report)
    }

    public func rollback(manifest: URL, extraGroup: Bool = false, cec: Bool = false) async throws {
        let operation: ConverterOperation = cec ? .rollbackCEC : (extraGroup ? .rollbackExtras : .rollback)
        let report = try await execute(operation, arguments: [operation.rawValue, "--manifest", manifest.path])
        complete(with: report)
    }

    /// A non-executing preview used by the write confirmation sheet.  It lets
    /// the sheet enumerate scope without ever copying a directory or invoking
    /// a broad catch-all command.
    public func writePlan() throws -> [PlannedConverterCommand] {
        let fingerprint = try currentAuthorizedFingerprint()
        guard let input else { throw ConversionWorkflowError.inputNotInspected }
        var plan = [
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
        ]
        if components.includeSystem {
            guard let source = components.systemSource, let target = components.systemTarget else {
                throw ConversionWorkflowError.missingSystemPaths
            }
            plan.append(
                PlannedConverterCommand(
                    operation: .convertSystem,
                    arguments: [
                        ConverterOperation.convertSystem.rawValue,
                        source.path,
                        "--output", target.path,
                        "--write",
                    ]
                )
            )
        }
        if !components.selectedGroups.isEmpty {
            let paths = try requiredExtraPaths()
            let groups = components.selectedGroups.sorted { $0.rawValue < $1.rawValue }.map(\.rawValue).joined(separator: ",")
            plan.append(
                PlannedConverterCommand(
                    operation: .convertExtras,
                    arguments: [
                        ConverterOperation.convertExtras.rawValue,
                        "--source-dir", paths.source.path,
                        "--output-dir", paths.staging.path,
                        "--write",
                    ]
                )
            )
            plan.append(
                PlannedConverterCommand(
                    operation: .installExtras,
                    arguments: [
                        ConverterOperation.installExtras.rawValue,
                        "--staging-dir", paths.staging.path,
                        "--target-dir", paths.target.path,
                        "--groups", groups,
                        "--write",
                    ]
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
            plan.append(
                PlannedConverterCommand(
                    operation: .convertCEC,
                    arguments: [
                        ConverterOperation.convertCEC.rawValue,
                        "--source-dir", source.path,
                        "--target", target.path,
                        "--write",
                        "--experimental",
                    ]
                )
            )
        }
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
        guard let current = currentFingerprint(), let authorized = dryRunFingerprint else {
            throw ConversionWorkflowError.dryRunRequired
        }
        guard current == authorized else { throw ConversionWorkflowError.staleDryRun }
        guard activeOperation == nil else { throw ConversionWorkflowError.dryRunRequired }
        return current
    }

    private func requireIdleForOptionalWrite() throws {
        guard activeOperation == nil else { throw ConversionWorkflowError.dryRunRequired }
    }

    private func currentFingerprint() -> DryRunFingerprint? {
        guard let sourceInspection, let targetInspection else { return nil }
        return DryRunFingerprint(
            sourceSHA256: sourceInspection.sha256,
            targetSHA256: targetInspection.sha256,
            includeSystem: components.includeSystem,
            selectedGroups: components.selectedGroups,
            cecAcknowledged: components.acknowledgeExperimentalCEC
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

    private func execute(_ operation: ConverterOperation, arguments: [String]) async throws -> ConverterReport {
        activeOperation = operation
        failure = nil
        state = .writing
        defer { activeOperation = nil }
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

    private func complete(with report: ConverterReport) {
        latestReport = report
        state = .success
        dryRunFingerprint = nil
    }

    private func invalidateAuthorization(nextState: WorkflowState) {
        dryRunFingerprint = nil
        latestReport = nil
        failure = nil
        state = nextState
    }

    private func failureAndRethrow(_ operation: ConverterOperation, _ error: Error, stderr: String) -> Error {
        failure = WorkflowFailure(operation: operation, message: error.localizedDescription, stderr: stderr)
        state = .failure
        return error
    }

    private func stderr(from error: ConversionWorkflowError) -> String {
        if case let .commandFailed(_, stderr) = error { return stderr }
        return ""
    }
}
