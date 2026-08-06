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

    func testRepairDryRunRequiresARevisionWhenDetectionIsAmbiguous() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(repairDryRunResult(confidence: "ambiguous", candidates: ["0.0.3", "0.0.4"])),
            .success(repairDryRunResult(confidence: "selected", candidates: ["0.0.3"])),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setMode(.repairConverted)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()

        XCTAssertTrue(workflow.repairRevisionSelectionRequired)
        XCTAssertEqual(workflow.repairRevisionCandidates, [.v0_0_3, .v0_0_4])
        XCTAssertNil(workflow.repairDryRunFingerprint)
        XCTAssertFalse(workflow.canWrite)

        workflow.setRepairFromVersion(.v0_0_3)
        try await workflow.runCoreDryRun()

        XCTAssertFalse(workflow.repairRevisionSelectionRequired)
        XCTAssertEqual(workflow.repairDryRunFingerprint?.fromVersion, .v0_0_3)
        XCTAssertTrue(workflow.canWrite)
        let commands = await executor.recordedCommands()
        XCTAssertFalse(commands[0].arguments.contains("--from-version"))
        XCTAssertTrue(commands[1].arguments.containsAdjacent("--from-version", "0.0.3"))
    }

    func testRepairWriteReusesTheAuthorizedRevisionAndPublishesCoordinatorManifest() async throws {
        let manifest = "/tmp/.mh3g-compatibility-repair-test.json"
        let executor = FakeConverterCommandExecutor(results: [
            .success(repairDryRunResult(confidence: "selected", candidates: ["0.0.5"])),
            .success(repairWrittenResult(manifest: manifest)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setMode(.repairConverted)
        workflow.setRepairFromVersion(.v0_0_5)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        try await workflow.writeCore()

        XCTAssertEqual(workflow.latestReport?.compatibilityManifest, manifest)
        let commands = await executor.recordedCommands()
        XCTAssertEqual(commands.count, 2)
        XCTAssertTrue(commands[0].arguments.containsAdjacent("--from-version", "0.0.5"))
        XCTAssertTrue(commands[1].arguments.containsAdjacent("--from-version", "0.0.5"))
        XCTAssertTrue(commands[1].arguments.containsAdjacent("--expected-source-set-sha256", fixtureRepairSourceSetSHA256))
        XCTAssertTrue(commands[1].arguments.containsAdjacent("--expected-current-set-sha256", fixtureRepairCurrentSetSHA256))
        XCTAssertTrue(commands[1].arguments.containsAdjacent("--expected-preview-sha256", fixtureRepairPreviewSHA256))
    }

    func testRepairNoChangesCompletesWithoutInventingWriteManifests() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(repairDryRunResult(confidence: "selected", candidates: ["0.0.5"], modified: false)),
            .success(repairNoChangesResult()),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setMode(.repairConverted)
        workflow.setRepairFromVersion(.v0_0_5)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        try await workflow.writeCore()

        XCTAssertEqual(workflow.state, .success)
        XCTAssertEqual(workflow.latestReport?.status, "no-changes")
        XCTAssertTrue(workflow.coreWriteCompleted)
        XCTAssertNil(workflow.repairDryRunFingerprint)
    }

    func testCompatibilityManifestUsesRollbackRepair() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(rolledBackResult(operation: ConverterOperation.rollbackRepair.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        let manifest = URL(fileURLWithPath: "/tmp/.mh3g-compatibility-repair-test.json")

        try await workflow.rollback(manifest: manifest)

        let commands = await executor.recordedCommands()
        let command = try XCTUnwrap(commands.first)
        XCTAssertEqual(command.arguments, [
            ConverterOperation.rollbackRepair.rawValue,
            "--manifest", manifest.path,
        ])
    }

    func testNewExportAuthorizesAnAbsentTargetAndWritesWithAbsencePrecondition() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(newExportDryRunResult()),
            .success(newExportWrittenResult()),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: nil)

        XCTAssertTrue(workflow.canStartDryRun)
        try await workflow.runCoreDryRun()

        XCTAssertTrue(workflow.canWrite)
        XCTAssertNil(workflow.dryRunFingerprint?.targetSHA256)

        let plan = try workflow.writePlan()
        XCTAssertEqual(plan.count, 1)
        XCTAssertTrue(plan[0].arguments.contains("--expected-target-absent"))
        XCTAssertFalse(plan[0].arguments.contains("--expected-target-sha256"))

        try await workflow.writeCore()
        let commands = await executor.recordedCommands()
        XCTAssertEqual(commands.count, 2)
        XCTAssertTrue(commands[1].arguments.contains("--expected-target-absent"))
        XCTAssertFalse(commands[1].arguments.contains("--expected-target-sha256"))
    }

    func testNewExportIsPresentedAsAReadyPlannedOutput() {
        let workflow = ConversionWorkflow(executable: fixtureExecutable)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: nil)

        XCTAssertTrue(workflow.isNewTargetExport)
    }

    func testSelectedOptionalDataMustBeFullyConfiguredBeforeItsContinuationIsReady() {
        let workflow = ConversionWorkflow(executable: fixtureExecutable)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        workflow.setComponents(ComponentSelection(includeGuildCards: true))
        XCTAssertFalse(workflow.selectedOptionalDataIsConfigured)

        workflow.setComponents(
            ComponentSelection(
                includeGuildCards: true,
                extraSourceDirectory: URL(fileURLWithPath: "/tmp/extdata/user"),
                extraStagingDirectory: URL(fileURLWithPath: "/tmp/mh3g-staging"),
                extraTargetDirectory: URL(fileURLWithPath: "/tmp/cemu")
            )
        )
        XCTAssertTrue(workflow.selectedOptionalDataIsConfigured)
    }

    func testIncompleteSelectedOptionalDataBlocksCoreDryRunAcrossNavigation() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        workflow.setComponents(ComponentSelection(includeGuildCards: true))

        XCTAssertFalse(workflow.selectedOptionalDataIsConfigured)
        XCTAssertFalse(workflow.canStartDryRun)

        do {
            try await workflow.runCoreDryRun()
            XCTFail("selected optional data must be configured before the core Dry Run")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .missingExtraDirectories)
        }
    }

    func testIncompleteSelectedOptionalDataBlocksAnExistingCoreWriteAuthorization() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)
        try await workflow.runCoreDryRun()
        XCTAssertTrue(workflow.canWrite)

        workflow.setComponents(ComponentSelection(includeGuildCards: true))
        XCTAssertFalse(workflow.selectedOptionalDataIsConfigured)
        XCTAssertFalse(workflow.canWrite)

        do {
            try await workflow.writeCore()
            XCTFail("an incomplete optional selection must block an existing core write authorization")
        } catch {
            XCTAssertEqual(error as? ConversionWorkflowError, .missingExtraDirectories)
        }
    }

    func testCoreWriteDoesNotMarkSelectedOptionalDataAsComplete() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(dryRunResult()),
            .success(writtenResult(operation: ConverterOperation.convert.rawValue)),
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
        try await workflow.writeCore()

        XCTAssertTrue(workflow.hasPendingSelectedOptionalWork)
        XCTAssertTrue(workflow.hasPendingSelectedConversionWork)
    }

    func testNewExportRefusesDryRunWhenTargetAppearsAfterInspection() async throws {
        let executor = FakeConverterCommandExecutor(results: [.success(dryRunResult())])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: nil)

        do {
            try await workflow.runCoreDryRun()
            XCTFail("a newly appeared target must invalidate an export Dry Run")
        } catch {
            XCTAssertEqual(
                error as? ConversionWorkflowError,
                .invalidReport("target appeared during Dry Run; refuse export")
            )
        }
        XCTAssertFalse(workflow.canWrite)
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
                .invalidReport("Dry Run requires valid source and output SHA-256")
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
                cecTarget: URL(fileURLWithPath: "/tmp/cemu/cec"),
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
                cecTarget: URL(fileURLWithPath: "/tmp/cemu/cec"),
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

    func testCoreWrittenStatusWithoutTransactionEvidenceFailsClosed() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(dryRunResult()),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.convert.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        do {
            try await workflow.writeCore()
            XCTFail("written status without output, backup, manifest, and hashes must fail closed")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canWrite)
        XCTAssertFalse(workflow.coreWriteCompleted)
    }

    func testCoreWrittenOutputHashMustMatchTheAuthorizedDryRun() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(dryRunResult()),
            .success(coreWrittenResult(outputSHA256: validSHA("c"))),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        do {
            try await workflow.writeCore()
            XCTFail("a write report for different bytes must not consume the authorized Dry Run")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canWrite)
        XCTAssertFalse(workflow.coreWriteCompleted)
    }

    func testEvidenceHashesRejectUppercaseShortAndEmptyValues() {
        XCTAssertTrue(ConverterEvidence.isValidSHA256(validSHA("a")))
        XCTAssertFalse(ConverterEvidence.isValidSHA256(validSHA("a").uppercased()))
        XCTAssertFalse(ConverterEvidence.isValidSHA256("abc123"))
        XCTAssertFalse(ConverterEvidence.isValidSHA256(""))
        XCTAssertFalse(ConverterEvidence.isValidSHA256(nil))
    }

    func testSystemWrittenStatusWithoutTransactionEvidenceFailsClosed() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(systemDryRunResult()),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.convertSystem.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(
            ComponentSelection(
                includeSystem: true,
                systemSource: fixtureSystemSource,
                systemTarget: fixtureSystemTarget
            )
        )

        try await workflow.runSystemDryRun()
        do {
            try await workflow.writeSystem()
            XCTFail("system written status without transaction evidence must fail closed")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canWriteSystem)
        XCTAssertFalse(workflow.systemWriteCompleted)
    }

    func testSystemEvidenceFailureRevokesOnlySystemAuthorization() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(dryRunResult()),
            .success(systemDryRunResult()),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.convertSystem.rawValue)),
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
        do {
            try await workflow.writeSystem()
            XCTFail("invalid system evidence must fail")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertTrue(workflow.canWrite)
        XCTAssertFalse(workflow.canWriteSystem)
    }

    func testExtrasStageWrittenStatusWithoutComponentEvidenceFailsClosed() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(extrasStageDryRunResult()),
            .success(extrasStageDryRunResult()),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.convertExtras.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(fixtureExtrasSelection)

        try await workflow.runExtrasStageDryRun()
        do {
            try await workflow.stageExtras()
            XCTFail("staging must require exact output/component evidence")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canStageExtras)
    }

    func testExtrasInstallWrittenStatusWithoutManifestAndSetEvidenceFailsClosed() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(extrasInstallDryRunResult()),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.installExtras.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(fixtureExtrasSelection)

        try await workflow.runExtrasInstallDryRun()
        do {
            try await workflow.installExtraGroups()
            XCTFail("ExtData install must require manifest, set hashes, and entry evidence")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canInstallExtras)
        XCTAssertFalse(workflow.extrasInstallCompleted)
    }

    func testExtrasInstallRejectsDuplicateComponentEvidence() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(extrasInstallDryRunResult()),
            .success(extrasInstallWrittenResult(components: ["card1", "card2", "card3", "card1"])),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(fixtureExtrasSelection)

        try await workflow.runExtrasInstallDryRun()
        do {
            try await workflow.installExtraGroups()
            XCTFail("duplicate group/component evidence must fail closed")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertFalse(workflow.canInstallExtras)
        XCTAssertFalse(workflow.extrasInstallCompleted)
    }

    func testCECWrittenStatusWithoutManifestAndHashEvidenceFailsClosed() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(cecDryRunResult()),
            .success(cecDryRunResult()),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.convertCEC.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(fixtureCECSelection)

        try await workflow.runCECDryRun()
        do {
            try await workflow.writeCEC()
            XCTFail("CEC write must require manifest and exact before/after hash evidence")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canWriteCEC)
    }

    func testCECWrittenTargetAfterMustMatchTheAuthorizedPreview() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(cecDryRunResult()),
            .success(cecDryRunResult()),
            .success(cecWrittenResult(targetAfterSHA256: validSHA("c"))),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setComponents(fixtureCECSelection)

        try await workflow.runCECDryRun()
        do {
            try await workflow.writeCEC()
            XCTFail("CEC target-after hash must match the authorized preview")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertFalse(workflow.canWriteCEC)
        XCTAssertEqual(workflow.state, .failure)
    }

    func testRepairWrittenStatusWithoutCoordinatorEvidenceFailsClosed() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(repairDryRunResult(confidence: "selected", candidates: ["0.0.5"])),
            .success(statusOnlyWrittenResult(operation: ConverterOperation.repairConverted.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setMode(.repairConverted)
        workflow.setRepairFromVersion(.v0_0_5)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        do {
            try await workflow.writeCore()
            XCTFail("repair write must require compatibility manifest and exact set hashes")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
        XCTAssertFalse(workflow.canWrite)
        XCTAssertFalse(workflow.coreWriteCompleted)
    }

    func testRepairWrittenComponentEvidenceMustMatchTheDryRun() async throws {
        let executor = FakeConverterCommandExecutor(results: [
            .success(repairDryRunResult(confidence: "selected", candidates: ["0.0.5"])),
            .success(
                repairWrittenResult(
                    manifest: "/tmp/.mh3g-compatibility-repair-test.json",
                    mergedSHA256: validSHA("7")
                )
            ),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)
        workflow.setMode(.repairConverted)
        workflow.setRepairFromVersion(.v0_0_5)
        workflow.configure(input: fixtureInput)
        workflow.applyInspections(source: fixtureSourceInspection, target: fixtureTargetInspection)

        try await workflow.runCoreDryRun()
        do {
            try await workflow.writeCore()
            XCTFail("repair component evidence must match the authorized Dry Run")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertFalse(workflow.canWrite)
        XCTAssertFalse(workflow.coreWriteCompleted)
    }

    func testRollbackStatusWithoutExactManifestEvidenceFailsClosed() async throws {
        let manifest = URL(fileURLWithPath: "/tmp/core-rollback.json")
        let executor = FakeConverterCommandExecutor(results: [
            .success(statusOnlyRolledBackResult(operation: ConverterOperation.rollback.rawValue)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)

        do {
            try await workflow.rollback(manifest: manifest)
            XCTFail("rollback status without the exact manifest must fail closed")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
    }

    func testRollbackWrongManifestEvidenceFailsClosed() async throws {
        let expected = URL(fileURLWithPath: "/tmp/core-rollback.json")
        let executor = FakeConverterCommandExecutor(results: [
            .success(
                rollbackResult(
                    operation: .rollback,
                    manifest: URL(fileURLWithPath: "/tmp/other-rollback.json")
                )
            ),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)

        do {
            try await workflow.rollback(manifest: expected)
            XCTFail("rollback must echo the exact selected manifest")
        } catch {
            XCTAssertTrue(error is ConversionWorkflowError)
        }

        XCTAssertEqual(workflow.state, .failure)
    }

    func testCoreAndCECRollbackAcceptOnlyTheExactEchoedManifest() async throws {
        let coreManifest = URL(fileURLWithPath: "/tmp/core-rollback.json")
        let cecManifest = URL(fileURLWithPath: "/tmp/cec-rollback.json")
        let executor = FakeConverterCommandExecutor(results: [
            .success(rollbackResult(operation: .rollback, manifest: coreManifest)),
            .success(rollbackResult(operation: .rollbackCEC, manifest: cecManifest)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)

        try await workflow.rollback(manifest: coreManifest)
        XCTAssertEqual(workflow.state, .success)
        try await workflow.rollback(manifest: cecManifest, cec: true)
        XCTAssertEqual(workflow.state, .success)
    }

    func testExtrasRollbackRequiresGroupAndEntryEvidence() async throws {
        let manifest = URL(fileURLWithPath: "/tmp/extras-rollback.json")
        let executor = FakeConverterCommandExecutor(results: [
            .success(extrasRollbackResult(manifest: manifest)),
        ])
        let workflow = ConversionWorkflow(executable: fixtureExecutable, executor: executor)

        try await workflow.rollback(manifest: manifest, extraGroup: true)

        XCTAssertEqual(workflow.state, .success)
        XCTAssertEqual(workflow.latestReport?.groups, [.guildCards])
        XCTAssertEqual(workflow.latestReport?.entries?.count, 4)
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
private let fixtureCECSourceRecordSetSHA256 = validSHA("e")
private let fixtureCECTargetSHA256 = validSHA("f")
private let fixtureRepairSourceSetSHA256 = validSHA("1")
private let fixtureRepairCurrentSetSHA256 = validSHA("2")
private let fixtureRepairPreviewSHA256 = validSHA("3")
private let fixtureExtrasSelection = ComponentSelection(
    includeGuildCards: true,
    extraSourceDirectory: URL(fileURLWithPath: "/tmp/extdata/user"),
    extraStagingDirectory: URL(fileURLWithPath: "/tmp/mh3g-staging"),
    extraTargetDirectory: URL(fileURLWithPath: "/tmp/cemu")
)
private let fixtureCECSelection = ComponentSelection(
    cecSourceDirectory: URL(fileURLWithPath: "/tmp/CEC/00048100"),
    cecTarget: URL(fileURLWithPath: "/tmp/cemu/cec"),
    acknowledgeExperimentalCEC: true
)

private func dryRunResult() -> ConverterCommandResult {
    let json = """
    {"operation":"convert","status":"dry-run","hashes":{"source":"\(fixtureSourceInspection.sha256)","target_before":"\(fixtureTargetInspection.sha256)","output":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"}}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func newExportDryRunResult() -> ConverterCommandResult {
    let json = """
    {"operation":"convert","status":"dry-run","hashes":{"source":"\(fixtureSourceInspection.sha256)","output":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"}}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func newExportWrittenResult() -> ConverterCommandResult {
    let json = """
    {"status":"written","hashes":{"source":"\(fixtureSourceInspection.sha256)","output":"\(validSHA("d"))"},"output":"\(fixtureInput.target.path)","backup":null,"manifest":"/tmp/cemu/.user2.manifest.json"}
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
    let json: String
    switch operation {
    case ConverterOperation.convert.rawValue:
        json = """
        {"status":"written","hashes":{"source":"\(fixtureSourceInspection.sha256)","target_before":"\(fixtureTargetInspection.sha256)","output":"\(validSHA("d"))"},"output":"\(fixtureInput.target.path)","backup":"/tmp/cemu/.user2.backup","manifest":"/tmp/cemu/.user2.manifest.json"}
        """
    case ConverterOperation.convertSystem.rawValue:
        json = """
        {"status":"written","hashes":{"source":"\(fixtureSystemSourceSHA256)","target_before":"\(fixtureSystemTargetSHA256)","output":"\(validSHA("e"))"},"output":"\(fixtureSystemTarget.path)","backup":"/tmp/cemu/.system.backup","manifest":"/tmp/cemu/.system.manifest.json"}
        """
    case ConverterOperation.convertExtras.rawValue:
        json = """
        {"status":"written","source_dir":"/tmp/extdata/user","output_dir":"/tmp/mh3g-staging","components":[{"component":"card1","source_sha256":"\(validSHA("4"))","output_sha256":"\(validSHA("5"))","output":"/tmp/mh3g-staging/card1","size":64}]}
        """
    case ConverterOperation.installExtras.rawValue:
        return extrasInstallWrittenResult()
    case ConverterOperation.convertCEC.rawValue:
        return cecWrittenResult()
    default:
        preconditionFailure("missing written fixture for \(operation)")
    }
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func coreWrittenResult(outputSHA256: String) -> ConverterCommandResult {
    let json = """
    {"status":"written","hashes":{"source":"\(fixtureSourceInspection.sha256)","target_before":"\(fixtureTargetInspection.sha256)","output":"\(outputSHA256)"},"output":"\(fixtureInput.target.path)","backup":"/tmp/cemu/.user2.backup","manifest":"/tmp/cemu/.user2.manifest.json"}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func statusOnlyWrittenResult(operation: String) -> ConverterCommandResult {
    let json = "{\"operation\":\"\(operation)\",\"status\":\"written\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func repairDryRunResult(
    confidence: String,
    candidates: [String],
    modified: Bool = true
) -> ConverterCommandResult {
    let candidateJSON = candidates.map { "\"\($0)\"" }.joined(separator: ",")
    let mergedSHA256 = modified ? validSHA("6") : validSHA("5")
    let json = """
    {"operation":"repair-converted","status":"dry-run","source_set_sha256":"\(fixtureRepairSourceSetSHA256)","current_set_sha256":"\(fixtureRepairCurrentSetSHA256)","preview_sha256":"\(fixtureRepairPreviewSHA256)","detection":{"confidence":"\(confidence)","candidates":[\(candidateJSON)]},"components":[{"component":"user2","target":"\(fixtureInput.target.path)","modified":\(modified),"detection":{"confidence":"\(confidence)","candidates":[\(candidateJSON)]},"merge":{"component":"user2","source_sha256":"\(validSHA("4"))","current_sha256":"\(validSHA("5"))","merged_sha256":"\(mergedSHA256)"}}]}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func repairWrittenResult(
    manifest: String,
    mergedSHA256: String = validSHA("6")
) -> ConverterCommandResult {
    let json = """
    {"operation":"repair-converted","status":"written","source":"\(fixtureInput.source.path)","current":"\(fixtureInput.target.path)","source_set_sha256":"\(fixtureRepairSourceSetSHA256)","current_set_sha256":"\(fixtureRepairCurrentSetSHA256)","preview_sha256":"\(fixtureRepairPreviewSHA256)","components":[{"component":"user2","target":"\(fixtureInput.target.path)","modified":true,"detection":{"confidence":"selected","candidates":["0.0.5"]},"merge":{"component":"user2","source_sha256":"\(validSHA("4"))","current_sha256":"\(validSHA("5"))","merged_sha256":"\(mergedSHA256)"}}],"manifests":["/tmp/.mh3g-user2-repair.json"],"compatibility_manifest":"\(manifest)"}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func repairNoChangesResult() -> ConverterCommandResult {
    let json = """
    {"operation":"repair-converted","status":"no-changes","source":"\(fixtureInput.source.path)","current":"\(fixtureInput.target.path)","source_set_sha256":"\(fixtureRepairSourceSetSHA256)","current_set_sha256":"\(fixtureRepairCurrentSetSHA256)","preview_sha256":"\(fixtureRepairPreviewSHA256)","components":[{"component":"user2","target":"\(fixtureInput.target.path)","modified":false,"detection":{"confidence":"selected","candidates":["0.0.5"]},"merge":{"component":"user2","source_sha256":"\(validSHA("4"))","current_sha256":"\(validSHA("5"))","merged_sha256":"\(validSHA("5"))"}}],"manifests":[],"compatibility_manifest":null}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func rolledBackResult(operation: String) -> ConverterCommandResult {
    let manifest = operation == ConverterOperation.rollbackRepair.rawValue
        ? "/tmp/.mh3g-compatibility-repair-test.json"
        : "/tmp/rollback.json"
    let json = "{\"operation\":\"\(operation)\",\"status\":\"rolled-back\",\"manifest\":\"\(manifest)\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func statusOnlyRolledBackResult(operation: String) -> ConverterCommandResult {
    let json = "{\"operation\":\"\(operation)\",\"status\":\"rolled-back\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func rollbackResult(operation: ConverterOperation, manifest: URL) -> ConverterCommandResult {
    let operationField = operation == .rollbackCEC ? "" : "\"operation\":\"\(operation.rawValue)\","
    let json = "{\(operationField)\"status\":\"rolled-back\",\"manifest\":\"\(manifest.path)\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func extrasRollbackResult(manifest: URL) -> ConverterCommandResult {
    let entries = ["card1", "card2", "card3", "cardbox"].map { component in
        """
        {"group":"guild-cards","component":"\(component)","target":"/tmp/cemu/\(component)","temporary":"/tmp/cemu/.\(component).tmp","before_sha256":"\(validSHA("8"))","after_sha256":"\(validSHA("9"))","backup":"/tmp/cemu/.\(component).backup","target_previously_existed":true}
        """
    }.joined(separator: ",")
    let json = """
    {"operation":"rollback-extras","status":"rolled-back","groups":["guild-cards"],"entries":[\(entries)],"manifest":"\(manifest.path)"}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func cecDryRunResult() -> ConverterCommandResult {
    let json = "{\"operation\":\"convert-cec\",\"status\":\"dry-run\",\"source_dir\":\"/tmp/CEC/00048100\",\"target\":\"/tmp/cemu/cec\",\"source_record_sha256\":[\"\(validSHA("a"))\"],\"source_record_set_sha256\":\"\(fixtureCECSourceRecordSetSHA256)\",\"target_sha256_before\":\"\(fixtureCECTargetSHA256)\",\"target_sha256_after\":\"\(validSHA("b"))\"}"
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func cecWrittenResult(targetAfterSHA256: String = validSHA("b")) -> ConverterCommandResult {
    let json = """
    {"status":"written","source_dir":"/tmp/CEC/00048100","target":"/tmp/cemu/cec","source_record_sha256":["\(validSHA("a"))"],"source_record_set_sha256":"\(fixtureCECSourceRecordSetSHA256)","target_sha256_before":"\(fixtureCECTargetSHA256)","target_sha256_after":"\(targetAfterSHA256)","manifest":"/tmp/cemu/.cec.manifest.json"}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func extrasStageDryRunResult() -> ConverterCommandResult {
    let json = """
    {"status":"dry-run","source_dir":"/tmp/extdata/user","output_dir":"/tmp/mh3g-staging","components":[{"component":"card1","source_sha256":"\(validSHA("4"))","output_sha256":"\(validSHA("5"))","output":"/tmp/mh3g-staging/card1","size":64}]}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func extrasInstallDryRunResult() -> ConverterCommandResult {
    let entries = ["card1", "card2", "card3", "cardbox"].map { component in
        """
        {"group":"guild-cards","component":"\(component)","target":"/tmp/cemu/\(component)","temporary":"/tmp/cemu/.dryrun-\(component).tmp","before_sha256":"\(validSHA("8"))","after_sha256":"\(validSHA("9"))","backup":"/tmp/cemu/.dryrun-\(component).backup","target_previously_existed":true}
        """
    }.joined(separator: ",")
    let json = """
    {"operation":"install-extras","status":"dry-run","groups":["guild-cards"],"entries":[\(entries)],"manifest":"/tmp/cemu/.mh3g-extra-install.json","staging_dir":"/tmp/mh3g-staging","target_dir":"/tmp/cemu","staging_set_sha256":"\(validSHA("6"))","target_set_sha256_before":"\(validSHA("7"))"}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func extrasInstallWrittenResult(
    components: [String] = ["card1", "card2", "card3", "cardbox"]
) -> ConverterCommandResult {
    let entries = components.map { component in
        """
        {"group":"guild-cards","component":"\(component)","target":"/tmp/cemu/\(component)","temporary":"/tmp/cemu/.\(component).tmp","before_sha256":"\(validSHA("8"))","after_sha256":"\(validSHA("9"))","backup":"/tmp/cemu/.\(component).backup","target_previously_existed":true}
        """
    }.joined(separator: ",")
    let backups = components
        .map { "\"/tmp/cemu/.\($0).backup\"" }
        .joined(separator: ",")
    let json = """
    {"operation":"install-extras","status":"written","groups":["guild-cards"],"entries":[\(entries)],"manifest":"/tmp/cemu/.mh3g-extra-install.json","staging_dir":"/tmp/mh3g-staging","target_dir":"/tmp/cemu","staging_set_sha256":"\(validSHA("6"))","target_set_sha256_before":"\(validSHA("7"))","backup_paths":[\(backups)]}
    """
    return ConverterCommandResult(exitCode: 0, stdout: Data(json.utf8), stderr: Data())
}

private func validSHA(_ character: Character) -> String {
    precondition("0123456789abcdef".contains(character))
    return String(repeating: String(character), count: 64)
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

private extension Array where Element == String {
    func containsAdjacent(_ first: String, _ second: String) -> Bool {
        indices.dropLast().contains { index in
            self[index] == first && self[index + 1] == second
        }
    }
}

private extension String {
    func repeated(_ count: Int) -> String { String(repeating: self, count: count) }
}
