import Foundation

/// A closed, semantic vocabulary for the workbench chrome. The UI must not
/// infer safety from a raw workflow phase or from color alone.
public enum WorkflowStatusKind: String, Equatable, Sendable {
    case needsInput
    case needsInspection
    case readyForDryRun
    case blocked
    case authorized
    case running
    case succeeded
    case failed
}

public struct WorkflowStatusPresentation: Equatable, Sendable {
    public let kind: WorkflowStatusKind
    public let titleKey: String
    public let detailKey: String
    public let isBlocking: Bool

    public init(
        kind: WorkflowStatusKind,
        titleKey: String,
        detailKey: String,
        isBlocking: Bool
    ) {
        self.kind = kind
        self.titleKey = titleKey
        self.detailKey = detailKey
        self.isBlocking = isBlocking
    }
}

public extension ConversionWorkflow {
    /// The single presentation source for the toolbar and the always-visible
    /// safety card. Ordering is deliberate: an in-flight operation or failure
    /// always wins over stale authorizations. Optional domains deliberately do
    /// not block or downgrade an independently authorized core transaction.
    var statusPresentation: WorkflowStatusPresentation {
        if activeOperation != nil || state == .writing {
            return .init(
                kind: .running,
                titleKey: "Status.Running",
                detailKey: "Status.Detail.Running",
                isBlocking: true
            )
        }
        if failure != nil || state == .failure {
            return .init(
                kind: .failed,
                titleKey: "Status.Failed",
                detailKey: "Status.Detail.Failed",
                isBlocking: true
            )
        }
        if repairRevisionSelectionRequired {
            return .init(
                kind: .blocked,
                titleKey: "Status.RevisionRequired",
                detailKey: "Status.Detail.RevisionRequired",
                isBlocking: true
            )
        }
        if canWrite || canWriteSystem || canStageExtras || canInstallExtras || canWriteCEC {
            return .init(
                kind: .authorized,
                titleKey: "Status.Authorized",
                detailKey: "Status.Detail.Authorized",
                isBlocking: false
            )
        }
        if state == .success {
            let hasPendingWork = hasPendingSelectedConversionWork
            return .init(
                kind: hasPendingWork ? .blocked : .succeeded,
                titleKey: hasPendingWork ? "Status.SelectedWorkPending" : "Status.Succeeded",
                detailKey: hasPendingWork ? "Status.Detail.SelectedWorkPending" : "Status.Detail.Succeeded",
                isBlocking: hasPendingWork
            )
        }
        if input == nil {
            return .init(
                kind: .needsInput,
                titleKey: "Status.NeedsInput",
                detailKey: "Status.Detail.NeedsInput",
                isBlocking: true
            )
        }
        if !coreInspectionComplete {
            return .init(
                kind: .needsInspection,
                titleKey: "Status.NeedsInspection",
                detailKey: "Status.Detail.NeedsInspection",
                isBlocking: true
            )
        }
        return .init(
            kind: .readyForDryRun,
            titleKey: "Status.ReadyForDryRun",
            detailKey: "Status.Detail.ReadyForDryRun",
            isBlocking: false
        )
    }
}
