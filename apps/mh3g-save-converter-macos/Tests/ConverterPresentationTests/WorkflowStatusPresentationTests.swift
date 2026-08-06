import Foundation
import XCTest
@testable import ConverterPresentation

@MainActor
final class WorkflowStatusPresentationTests: XCTestCase {
    private let source = URL(fileURLWithPath: "/tmp/apple-design-source/user1")
    private let target = URL(fileURLWithPath: "/tmp/apple-design-target/user1")
    private let sourceInspection = InputInspection(profile: "jp-3ds-user", size: 1, sha256: String(repeating: "a", count: 64))
    private let targetInspection = InputInspection(profile: "jp-wiiu-user", size: 1, sha256: String(repeating: "b", count: 64))

    func testStatusStartsWithAnExplicitBlockingInputState() {
        let workflow = ConversionWorkflow(executable: URL(fileURLWithPath: "/tmp/converter"))

        XCTAssertEqual(workflow.statusPresentation.kind, .needsInput)
        XCTAssertTrue(workflow.statusPresentation.isBlocking)
        XCTAssertEqual(workflow.statusPresentation.detailKey, "Status.Detail.NeedsInput")
    }

    func testInspectedInputIsPresentedAsReadyForDryRunWithoutClaimingAuthorization() {
        let workflow = configuredWorkflow()

        XCTAssertEqual(workflow.statusPresentation.kind, .readyForDryRun)
        XCTAssertFalse(workflow.statusPresentation.isBlocking)
        XCTAssertFalse(workflow.canWrite)
    }

    func testStageRailUsesBlockingIconBeforeInputIsSelected() {
        let workflow = ConversionWorkflow(executable: URL(fileURLWithPath: "/tmp/converter"))

        let inputStep = workflow.stageRailPresentation.first { $0.route == .input }

        XCTAssertEqual(inputStep?.tone, .blocked)
        XCTAssertEqual(inputStep?.iconName, "exclamationmark.triangle.fill")
        XCTAssertEqual(inputStep?.accessibilityStateKey, "Status.Blocked")
    }

    func testStageRailMarksInspectedInputCompleteAndDryRunCurrent() {
        let workflow = configuredWorkflow()

        let rail = workflow.stageRailPresentation
        let inputStep = rail.first { $0.route == .input }
        let dryRunStep = rail.first { $0.route == .dryRun }

        XCTAssertEqual(inputStep?.tone, .complete)
        XCTAssertEqual(inputStep?.iconName, "checkmark.circle.fill")
        XCTAssertEqual(dryRunStep?.tone, .current)
        XCTAssertEqual(dryRunStep?.iconName, "checkmark.shield")
    }

    func testStageRailLayoutContractRequiresAdaptiveReadableFallbacks() {
        XCTAssertEqual(
            WorkflowStageRailLayoutContract.adaptive.fallbackOrder,
            [.horizontal, .twoColumnGrid, .vertical]
        )
        XCTAssertTrue(WorkflowStageRailLayoutContract.adaptive.preservesFullLabels)
        XCTAssertTrue(WorkflowStageRailLayoutContract.adaptive.preservesAccessibilityStateLabels)
    }

    func testIncompleteOptionalSelectionOverridesAStaleCoreAuthorization() throws {
        let workflow = configuredWorkflow()
        try workflow.authorizeDryRunForTesting()
        XCTAssertEqual(workflow.statusPresentation.kind, .authorized)

        workflow.setComponents(ComponentSelection(includeGuildCards: true))

        XCTAssertEqual(workflow.statusPresentation.kind, .blocked)
        XCTAssertEqual(workflow.statusPresentation.titleKey, "Status.OptionalDataBlocked")
        XCTAssertTrue(workflow.statusPresentation.isBlocking)
        XCTAssertFalse(workflow.canWrite)
        let dryRunStep = workflow.stageRailPresentation.first { $0.route == .dryRun }
        XCTAssertEqual(dryRunStep?.tone, .blocked)
        XCTAssertEqual(dryRunStep?.iconName, "exclamationmark.triangle.fill")
        XCTAssertEqual(dryRunStep?.accessibilityStateKey, "Status.Blocked")
    }

    private func configuredWorkflow() -> ConversionWorkflow {
        let workflow = ConversionWorkflow(executable: URL(fileURLWithPath: "/tmp/converter"))
        workflow.configure(input: ConversionInput(source: source, target: target))
        workflow.applyInspections(source: sourceInspection, target: targetInspection)
        return workflow
    }
}
