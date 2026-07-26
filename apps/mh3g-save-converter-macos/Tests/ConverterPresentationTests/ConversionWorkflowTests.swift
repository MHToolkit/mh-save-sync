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
        XCTAssertEqual(workflow.dryRunFingerprint?.selectedGroups, [])
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

    func testDeselectedExtraGroupNeverAppearsInWritePlan() async throws {
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

        let plan = try workflow.writePlan()

        XCTAssertTrue(plan.contains(where: { $0.operation == .convertSystem }))
        XCTAssertTrue(plan.contains(where: { $0.operation == .installExtras && $0.arguments.contains("guild-cards") }))
        XCTAssertFalse(plan.contains(where: { $0.arguments.contains("quests") }))

        let systemCommand = try XCTUnwrap(plan.first(where: { $0.operation == .convertSystem }))
        XCTAssertEqual(systemCommand.arguments, [
            ConverterOperation.convertSystem.rawValue,
            fixtureSystemSource.path,
            "--output", fixtureSystemTarget.path,
            "--write",
            "--expected-source-sha256", fixtureSystemSourceSHA256,
            "--expected-target-sha256", fixtureSystemTargetSHA256,
        ])
    }

    func testExperimentalCECNeedsSeparateAcknowledgement() async throws {
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: FakeConverterCommandExecutor())
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
        try workflow.authorizeDryRunForTesting()
        XCTAssertTrue(try workflow.writePlan().contains(where: { $0.operation == .convertCEC }))
    }

    func testSystemWriteRequiresItsOwnDryRunAndUsesSystemHashPreconditions() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(systemDryRunResult())])
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

    func testChangingComponentSelectionInvalidatesSystemDryRunAuthorization() async throws {
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

        XCTAssertNil(workflow.systemDryRunFingerprint)
        do {
            try await workflow.writeSystem()
            XCTFail("changed components invalidate system Dry Run authorization")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .dryRunRequired)
        }
    }

    func testExperimentalCECWriteDoesNotPassUnsupportedHashPreconditions() async throws {
        let executor = FakeConverterCommandExecutor()
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

        try await workflow.writeCEC()

        let commands = await executor.recordedCommands()
        let command = try XCTUnwrap(commands.first)
        XCTAssertEqual(Array(command.arguments.prefix(1)), [ConverterOperation.convertCEC.rawValue])
        XCTAssertTrue(command.arguments.contains("--experimental"))
        XCTAssertFalse(command.arguments.contains("--expected-source-record-set-sha256"))
        XCTAssertFalse(command.arguments.contains("--expected-target-sha256"))
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

    func run(_ command: ConverterCommand) async throws -> ConverterCommandResult {
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
