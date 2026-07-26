import Foundation
import XCTest
@testable import ConverterPresentation

@MainActor
final class ConversionWorkflowTests: XCTestCase {
    func testWriteIsDisabledBeforeDryRun() async {
        let executor = FakeConverterCommandExecutor()
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        XCTAssertFalse(workflow.canWrite)
    }

    func testDryRunAuthorizesOnlyTheExactInputFingerprint() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()

        XCTAssertTrue(workflow.canWrite)
        XCTAssertEqual(workflow.dryRunFingerprint?.sourceSHA256, fixtureSourceInspection.sha256)
        XCTAssertEqual(workflow.dryRunFingerprint?.targetSHA256, fixtureTargetInspection.sha256)
    }

    func testCoreDryRunRequiresBothReportedHashesAndClearsPriorAuthorization() async throws {
        let incompleteDryRun = ConverterCommandResult(
            exitCode: 0,
            stdout: Data("{\"operation\":\"convert\",\"status\":\"dry-run\",\"hashes\":{\"source\":\"\(fixtureSourceInspection.sha256)\"}}".utf8),
            stderr: Data()
        )
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult()), .success(incompleteDryRun)])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        XCTAssertTrue(workflow.canWrite)

        do {
            try await workflow.runCoreDryRun()
            XCTFail("a Dry Run without both hashes must not authorize a write")
        } catch {
            XCTAssertEqual(
                error as? ConversionWorkflowError,
                .invalidReport("Dry Run requires source and target_before SHA-256")
            )
        }

        XCTAssertNil(workflow.dryRunFingerprint)
        XCTAssertFalse(workflow.canWrite)
        XCTAssertEqual(workflow.state, .failure)
    }

    func testDryRunPublishesWorkingStateWhileConverterIsRunning() async throws {
        let executor = BlockingConverterCommandExecutor()
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        let task = Task { try await workflow.runCoreDryRun() }
        await executor.waitUntilStarted()
        XCTAssertEqual(workflow.state, .writing)

        await executor.complete(with: dryRunResult())
        try await task.value
        XCTAssertEqual(workflow.state, .dryRun)
    }

    func testBusyOperationRejectsChangesAndSecondOperationWithoutOverwritingTheFirstResult() async throws {
        let executor = BlockingConverterCommandExecutor(followUpResults: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget
            )
        )
        try workflow.authorizeDryRunForTesting()

        let originalInput = workflow.input
        let originalSourceInspection = workflow.sourceInspection
        let originalTargetInspection = workflow.targetInspection
        let originalComponents = workflow.components
        let originalCoreAuthorization = workflow.dryRunFingerprint

        let firstOperation = Task { try await workflow.runSystemDryRun() }
        await executor.waitUntilStarted()
        XCTAssertEqual(workflow.activeOperation, .convertSystem)

        workflow.configure(
            input: ConversionInput(
                source: URL(fileURLWithPath: "/tmp/changed/user2"),
                target: URL(fileURLWithPath: "/tmp/changed-cemu/user2")
            )
        )
        workflow.applyInspections(
            source: InputInspection(profile: "Changed3DS", size: 1, sha256: "c".repeated(64)),
            target: InputInspection(profile: "ChangedCemu", size: 2, sha256: "d".repeated(64))
        )
        workflow.setComponents(ComponentSelection(includeGuildCards: true))

        XCTAssertEqual(workflow.input, originalInput)
        XCTAssertEqual(workflow.sourceInspection, originalSourceInspection)
        XCTAssertEqual(workflow.targetInspection, originalTargetInspection)
        XCTAssertEqual(workflow.components, originalComponents)
        XCTAssertEqual(workflow.dryRunFingerprint, originalCoreAuthorization)

        do {
            try await workflow.inspectInputs()
            XCTFail("inspection must not run while another operation owns the workflow")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .operationInProgress(.convertSystem))
        }

        do {
            try await workflow.runCoreDryRun()
            XCTFail("a second operation must be rejected while the first operation owns the workflow")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .operationInProgress(.convertSystem))
        }

        do {
            try await workflow.rollback(manifest: URL(fileURLWithPath: "/tmp/rollback.json"))
            XCTFail("rollback must not run while another operation owns the workflow")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .operationInProgress(.convertSystem))
        }

        XCTAssertEqual(workflow.activeOperation, .convertSystem)
        XCTAssertNil(workflow.failure)
        XCTAssertEqual(workflow.dryRunFingerprint, originalCoreAuthorization)

        await executor.complete(with: systemDryRunResult())
        try await firstOperation.value

        XCTAssertNil(workflow.activeOperation)
        XCTAssertEqual(workflow.latestReport?.operation, ConverterOperation.convertSystem.rawValue)
        XCTAssertEqual(workflow.dryRunFingerprint, originalCoreAuthorization)
    }

    func testChangingTargetInvalidatesDryRunAuthorization() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        try await workflow.runCoreDryRun()
        XCTAssertTrue(workflow.canWrite)

        workflow.configure(input: ConversionInput(source: fixtureInput.source, target: URL(fileURLWithPath: "/tmp/other-user2")))
        workflow.applyInspections(source: fixtureSourceInspection, target: InputInspection(profile: "JpCemu", size: 35_392, sha256: "c".repeated(64)))

        XCTAssertFalse(workflow.canWrite)
        XCTAssertNil(workflow.dryRunFingerprint)
    }

    func testCoreAuthorizationDoesNotDependOnIndependentComponentSelection() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        let coreAuthorization = workflow.dryRunFingerprint

        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                includeGuildCards: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget,
                extraSourceDirectory: URL(fileURLWithPath: "/tmp/extdata/user"),
                extraStagingDirectory: URL(fileURLWithPath: "/tmp/mh3g-staging"),
                extraTargetDirectory: URL(fileURLWithPath: "/tmp/cemu"),
                cecSourceDirectory: URL(fileURLWithPath: "/tmp/CEC/00048100"),
                cecTarget: URL(fileURLWithPath: "/tmp/cemu/cec"),
                acknowledgeExperimentalCEC: true
            )
        )

        XCTAssertEqual(workflow.dryRunFingerprint, coreAuthorization)
        XCTAssertTrue(workflow.canWrite)
    }

    func testWritePlanRejectsAnOptionalGroupWithoutItsOwnAuthorization() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(systemDryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                includeGuildCards: true,
                includeQuests: false,
                systemSource: URL(fileURLWithPath: "/tmp/3ds/system"),
                systemTarget: URL(fileURLWithPath: "/tmp/cemu/system"),
                extraSourceDirectory: URL(fileURLWithPath: "/tmp/extdata/user"),
                extraStagingDirectory: URL(fileURLWithPath: "/tmp/mh3g-staging"),
                extraTargetDirectory: URL(fileURLWithPath: "/tmp/cemu")
            )
        )
        try workflow.authorizeDryRunForTesting()

        XCTAssertThrowsError(try workflow.writePlan()) { error in
            XCTAssertEqual(error as? ConversionWorkflowError, .dryRunRequired)
        }

        try await workflow.runSystemDryRun()

        XCTAssertThrowsError(try workflow.writePlan()) { error in
            XCTAssertEqual(error as? ConversionWorkflowError, .dryRunRequired)
        }
    }

    func testExperimentalCECNeedsSeparateAcknowledgement() async throws {
        let workflow = ConversionWorkflow(
            executable: fixtureExecutable,
            executor: FakeConverterCommandExecutor(results: [.success(cecDryRunResult())])
        )
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(
            ComponentSelection(
                cecSourceDirectory: URL(fileURLWithPath: "/tmp/CEC/00048100"),
                cecTarget: URL(fileURLWithPath: "/tmp/cec"),
                acknowledgeExperimentalCEC: false
            )
        )
        try workflow.authorizeDryRunForTesting()

        XCTAssertThrowsError(try workflow.writePlan()) { error in
            XCTAssertEqual(error as? ConversionWorkflowError, .experimentalCECAcknowledgementRequired)
        }

        workflow.setComponents(
            ComponentSelection(
                cecSourceDirectory: URL(fileURLWithPath: "/tmp/CEC/00048100"),
                cecTarget: URL(fileURLWithPath: "/tmp/cec"),
                acknowledgeExperimentalCEC: true
            )
        )
        try await workflow.runCECDryRun()
        try workflow.authorizeDryRunForTesting()
        let command = try XCTUnwrap(
            try workflow.writePlan().first(where: { $0.operation == .convertCEC })
        )
        XCTAssertTrue(command.arguments.contains("--expected-source-record-set-sha256"))
        XCTAssertTrue(command.arguments.contains(fixtureCECSourceRecordSetSHA256))
        XCTAssertTrue(command.arguments.contains("--expected-target-sha256"))
        XCTAssertTrue(command.arguments.contains(fixtureCECTargetSHA256))
    }

    func testSystemWriteRequiresItsOwnDryRunAndUsesSystemHashPreconditions() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(systemDryRunResult()),
            .success(writtenResult(operation: ConverterOperation.convertSystem.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget
            )
        )
        try workflow.authorizeDryRunForTesting()

        do {
            try await workflow.writeSystem()
            XCTFail("a core-slot Dry Run cannot authorize a system write")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .dryRunRequired)
        }

        try await workflow.runSystemDryRun()
        XCTAssertEqual(workflow.systemDryRunFingerprint?.sourceSHA256, fixtureSystemSourceSHA256)
        XCTAssertEqual(workflow.systemDryRunFingerprint?.targetSHA256, fixtureSystemTargetSHA256)

        try await workflow.writeSystem()

        let commands = await executor.recordedCommands()
        XCTAssertEqual(commands.count, 2)
        XCTAssertEqual(commands[0].arguments, [
            ConverterOperation.convertSystem.rawValue,
            fixtureSystemSource.path,
            "--output", fixtureSystemTarget.path,
            "--dry-run",
        ])
        XCTAssertEqual(commands[1].arguments, [
            ConverterOperation.convertSystem.rawValue,
            fixtureSystemSource.path,
            "--output", fixtureSystemTarget.path,
            "--write",
            "--expected-source-sha256", fixtureSystemSourceSHA256,
            "--expected-target-sha256", fixtureSystemTargetSHA256,
        ])
    }

    func testChangingUnrelatedComponentSelectionPreservesSystemDryRunAuthorization() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(systemDryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget
            )
        )
        try await workflow.runSystemDryRun()
        XCTAssertNotNil(workflow.systemDryRunFingerprint)

        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                includeGuildCards: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget,
                extraSourceDirectory: URL(fileURLWithPath: "/tmp/extdata/user"),
                extraStagingDirectory: URL(fileURLWithPath: "/tmp/mh3g-staging"),
                extraTargetDirectory: URL(fileURLWithPath: "/tmp/cemu")
            )
        )

        XCTAssertNotNil(workflow.systemDryRunFingerprint)
        XCTAssertTrue(workflow.canWriteSystem)
    }

    func testSuccessfulSystemWriteDoesNotClearCoreAuthorization() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(dryRunResult()),
            .success(systemDryRunResult()),
            .success(writtenResult(operation: ConverterOperation.convertSystem.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget
            )
        )

        try await workflow.runCoreDryRun()
        try await workflow.runSystemDryRun()
        XCTAssertTrue(workflow.canWrite)
        XCTAssertTrue(workflow.canWriteSystem)

        try await workflow.writeSystem()

        XCTAssertTrue(workflow.canWrite)
        XCTAssertNil(workflow.systemDryRunFingerprint)
    }

    func testExtrasStageAndInstallRequireTheirOwnDryRuns() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(extrasStageDryRunResult()),
            .success(extrasStageDryRunResult()),
            .success(writtenResult(operation: ConverterOperation.convertExtras.rawValue)),
            .success(extrasInstallDryRunResult()),
            .success(writtenResult(operation: ConverterOperation.installExtras.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(
            ComponentSelection(
                includeGuildCards: true,
                extraSourceDirectory: URL(fileURLWithPath: "/tmp/extdata/user"),
                extraStagingDirectory: URL(fileURLWithPath: "/tmp/mh3g-staging"),
                extraTargetDirectory: URL(fileURLWithPath: "/tmp/cemu")
            )
        )

        do {
            try await workflow.stageExtras()
            XCTFail("an ExtData stage write must require its own Dry Run")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .dryRunRequired)
        }

        try await workflow.runExtrasStageDryRun()
        XCTAssertTrue(workflow.canStageExtras)
        try await workflow.stageExtras()

        try await workflow.runExtrasInstallDryRun()
        XCTAssertTrue(workflow.canInstallExtras)
        try await workflow.installExtraGroups()

        let commands = await executor.recordedCommands()
        XCTAssertEqual(commands.count, 5)
        XCTAssertEqual(commands[0].arguments.last, "--dry-run")
        XCTAssertEqual(commands[1].arguments.last, "--dry-run")
        XCTAssertTrue(commands[2].arguments.contains("--write"))
        XCTAssertEqual(commands[3].arguments.last, "--dry-run")
        XCTAssertTrue(commands[4].arguments.contains("--expected-staging-set-sha256"))
        XCTAssertTrue(commands[4].arguments.contains("--expected-target-set-sha256"))
        XCTAssertFalse(commands[4].arguments.contains("quests"))
    }

    func testExperimentalCECWriteRequiresItsOwnDryRunAndRechecksItBeforeWriting() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(cecDryRunResult()),
            .success(cecDryRunResult()),
            .success(writtenResult(operation: ConverterOperation.convertCEC.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(
            ComponentSelection(
                cecSourceDirectory: URL(fileURLWithPath: "/tmp/CEC/00048100"),
                cecTarget: URL(fileURLWithPath: "/tmp/cemu/cec"),
                acknowledgeExperimentalCEC: true
            )
        )
        try workflow.authorizeDryRunForTesting()

        do {
            try await workflow.writeCEC()
            XCTFail("core-slot authorization cannot authorize experimental CEC")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .dryRunRequired)
        }

        try await workflow.runCECDryRun()
        XCTAssertTrue(workflow.canWriteCEC)
        try await workflow.writeCEC()

        let commands = await executor.recordedCommands()
        XCTAssertEqual(commands.count, 3)
        XCTAssertEqual(commands[0].arguments.last, "--dry-run")
        XCTAssertEqual(commands[1].arguments.last, "--dry-run")
        let command = try XCTUnwrap(commands.last)
        XCTAssertEqual(Array(command.arguments.prefix(1)), [ConverterOperation.convertCEC.rawValue])
        XCTAssertTrue(command.arguments.contains("--experimental"))
        XCTAssertTrue(command.arguments.contains("--expected-source-record-set-sha256"))
        XCTAssertTrue(command.arguments.contains(fixtureCECSourceRecordSetSHA256))
        XCTAssertTrue(command.arguments.contains("--expected-target-sha256"))
        XCTAssertTrue(command.arguments.contains(fixtureCECTargetSHA256))
    }

    func testExperimentalCECWriteClearsAuthorizationWhenItsRecordSetChanges() async throws {
        let changedPlan = ConverterCommandResult(
            exitCode: 0,
            stdout: Data("{\"operation\":\"convert-cec\",\"status\":\"dry-run\",\"source_record_sha256\":[\"\("e".repeated(64))\"],\"source_record_set_sha256\":\"\("h".repeated(64))\",\"target_sha256_before\":\"\(fixtureCECTargetSHA256)\"}".utf8),
            stderr: Data()
        )
        let executor = FakeConverterCommandExecutor(results: [.success(cecDryRunResult()), .success(changedPlan)])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(
            ComponentSelection(
                cecSourceDirectory: URL(fileURLWithPath: "/tmp/CEC/00048100"),
                cecTarget: URL(fileURLWithPath: "/tmp/cemu/cec"),
                acknowledgeExperimentalCEC: true
            )
        )

        try await workflow.runCECDryRun()
        XCTAssertTrue(workflow.canWriteCEC)

        do {
            try await workflow.writeCEC()
            XCTFail("a changed CEC record set must not write")
        } catch {
            XCTAssertEqual(
                error as? ConversionWorkflowError,
                .invalidReport("CEC mailbox or target changed after Dry Run")
            )
        }

        XCTAssertNil(workflow.cecDryRunFingerprint)
        XCTAssertFalse(workflow.canWriteCEC)
        XCTAssertEqual(workflow.state, .failure)
    }

    func testWriteRejectsSuccessfulProcessWithNonWrittenStatus() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(dryRunResult()),
            .success(ConverterCommandResult(exitCode: 0, stdout: Data("{}".utf8), stderr: Data())),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        try await workflow.runCoreDryRun()

        do {
            try await workflow.writeCore()
            XCTFail("a response without written status must not be reported as success")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .invalidReport("expected written status"))
        }

        XCTAssertEqual(workflow.state, .failure)
    }

    func testFailureKeepsOperationAndStderrVisible() async {
        let executor = FakeConverterCommandExecutor(results: [.success(ConverterCommandResult(exitCode: 2, stdout: Data(), stderr: Data("target changed".utf8)))])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        do {
            try await workflow.runCoreDryRun()
            XCTFail("the command should fail")
        } catch {
            XCTAssertEqual(workflow.state, .failure)
            XCTAssertEqual(workflow.failure?.operation, .convert)
            XCTAssertEqual(workflow.failure?.stderr, "target changed")
        }
    }
}

private let fixtureExecutable = URL(fileURLWithPath: "/tmp/mh3g-save-convert")
private let fixtureInput = ConversionInput(
    source: URL(fileURLWithPath: "/tmp/3ds/user2"),
    target: URL(fileURLWithPath: "/tmp/cemu/user2")
)
private let fixtureSourceInspection = InputInspection(profile: "JpThreeDs", size: 35_328, sha256: "a".repeated(64))
private let fixtureTargetInspection = InputInspection(profile: "JpCemu", size: 35_392, sha256: "b".repeated(64))
private let fixtureSystemSource = URL(fileURLWithPath: "/tmp/3ds/system")
private let fixtureSystemTarget = URL(fileURLWithPath: "/tmp/cemu/system")
private let fixtureSystemSourceSHA256 = "c".repeated(64)
private let fixtureSystemTargetSHA256 = "d".repeated(64)
private let fixtureCECSourceRecordSetSHA256 = "f".repeated(64)
private let fixtureCECTargetSHA256 = "g".repeated(64)

private func dryRunResult() -> ConverterCommandResult {
    let json = """
    {"operation":"convert","status":"dry-run","hashes":{"source":"\(fixtureSourceInspection.sha256)","target_before":"\(fixtureTargetInspection.sha256)","output":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"}}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func systemDryRunResult() -> ConverterCommandResult {
    let json = """
    {"operation":"convert-system","status":"dry-run","hashes":{"source":"\(fixtureSystemSourceSHA256)","target_before":"\(fixtureSystemTargetSHA256)","output":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func writtenResult(operation: String) -> ConverterCommandResult {
    let json = "{\"operation\":\"\(operation)\",\"status\":\"written\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func cecDryRunResult() -> ConverterCommandResult {
    let json = "{\"operation\":\"convert-cec\",\"status\":\"dry-run\",\"source_record_sha256\":[\"\("e".repeated(64))\"],\"source_record_set_sha256\":\"\(fixtureCECSourceRecordSetSHA256)\",\"target_sha256_before\":\"\(fixtureCECTargetSHA256)\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func extrasStageDryRunResult() -> ConverterCommandResult {
    let json = """
    {"status":"dry-run","components":[{"component":"card1","source_sha256":"\("1".repeated(64))","output_sha256":"\("2".repeated(64))","output":"/tmp/mh3g-staging/card1","size":64}]}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func extrasInstallDryRunResult() -> ConverterCommandResult {
    let json = """
    {"operation":"install-extras","status":"dry-run","groups":["guild-cards"],"staging_set_sha256":"\("3".repeated(64))","target_set_sha256_before":"\("4".repeated(64))"}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private actor FakeConverterCommandExecutor: ConverterCommandExecuting {
    private var results: [Result<ConverterCommandResult, Error>]
    private var commands: [ConverterCommand] = []

    init(results: [Result<ConverterCommandResult, Error>] = []) {
        self.results = results
    }

    func run(_ command: ConverterCommand) async throws -> ConverterCommandResult {
        commands.append(command)
        let next = results.isEmpty ? .success(ConverterCommandResult(exitCode: 0, stdout: Data("{}".utf8), stderr: Data())) : results.removeFirst()
        return try next.get()
    }

    func recordedCommands() -> [ConverterCommand] {
        commands
    }
}

private actor BlockingConverterCommandExecutor: ConverterCommandExecuting {
    private var didStart = false
    private var startWaiter: CheckedContinuation<Void, Never>?
    private var resultWaiter: CheckedContinuation<ConverterCommandResult, Error>?
    private var followUpResults: [Result<ConverterCommandResult, Error>]

    init(followUpResults: [Result<ConverterCommandResult, Error>] = []) {
        self.followUpResults = followUpResults
    }

    func run(_ command: ConverterCommand) async throws -> ConverterCommandResult {
        if didStart {
            let next = followUpResults.isEmpty
                ? .success(ConverterCommandResult(exitCode: 0, stdout: Data("{}".utf8), stderr: Data()))
                : followUpResults.removeFirst()
            return try next.get()
        }
        didStart = true
        startWaiter?.resume()
        startWaiter = nil
        return try await withCheckedThrowingContinuation { continuation in
            resultWaiter = continuation
        }
    }

    func waitUntilStarted() async {
        guard !didStart else { return }
        await withCheckedContinuation { continuation in
            startWaiter = continuation
        }
    }

    func complete(with result: ConverterCommandResult) {
        resultWaiter?.resume(returning: result)
        resultWaiter = nil
    }
}

private extension String {
    func repeated(_ count: Int) -> String { String(repeating: self, count: count) }
}
