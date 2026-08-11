import Foundation

public enum WorkflowStageRailTone: String, Equatable, Sendable {
    case pending
    case current
    case complete
    case blocked
}

public struct WorkflowStageRailStepPresentation: Equatable, Identifiable, Sendable {
    public let route: ConverterNavigation
    public let titleKey: String
    public let iconName: String
    public let tone: WorkflowStageRailTone
    public let accessibilityStateKey: String

    public var id: String { route.rawValue }

    public init(
        route: ConverterNavigation,
        titleKey: String,
        iconName: String,
        tone: WorkflowStageRailTone,
        accessibilityStateKey: String
    ) {
        self.route = route
        self.titleKey = titleKey
        self.iconName = iconName
        self.tone = tone
        self.accessibilityStateKey = accessibilityStateKey
    }
}

public enum WorkflowStageRailLayoutFallback: String, Equatable, Sendable {
    case horizontal
    case twoColumnGrid
    case vertical
}

public struct WorkflowStageRailLayoutContract: Equatable, Sendable {
    public let fallbackOrder: [WorkflowStageRailLayoutFallback]
    public let preservesFullLabels: Bool
    public let preservesAccessibilityStateLabels: Bool

    public static let adaptive = WorkflowStageRailLayoutContract(
        fallbackOrder: [.horizontal, .twoColumnGrid, .vertical],
        preservesFullLabels: true,
        preservesAccessibilityStateLabels: true
    )
}

public extension ConversionWorkflow {
    /// Non-decorative presentation for the visible stage rail. Icons encode
    /// the same fail-closed state as the banner, so inspection/dry-run/write
    /// cannot look visually complete just because a step has a pretty symbol.
    var stageRailPresentation: [WorkflowStageRailStepPresentation] {
        [
            railStep(
                route: .input,
                isCurrent: input == nil || !coreInspectionComplete,
                isComplete: coreInspectionComplete,
                isBlocked: input == nil
            ),
            railStep(
                route: .dryRun,
                isCurrent: coreInspectionComplete && !canWrite && state != .success,
                isComplete: canWrite || state == .success,
                isBlocked: statusPresentation.kind == .blocked
            ),
            railStep(
                route: .writeRollback,
                isCurrent: canWrite || state == .writing,
                isComplete: state == .success && !hasPendingSelectedConversionWork,
                isBlocked: statusPresentation.kind == .failed
            ),
            railStep(
                route: .history,
                isCurrent: state == .success,
                isComplete: state == .success && !hasPendingSelectedConversionWork,
                isBlocked: false
            ),
        ]
    }

    private func railStep(
        route: ConverterNavigation,
        isCurrent: Bool,
        isComplete: Bool,
        isBlocked: Bool
    ) -> WorkflowStageRailStepPresentation {
        let tone: WorkflowStageRailTone
        if isBlocked {
            tone = .blocked
        } else if isComplete {
            tone = .complete
        } else if isCurrent {
            tone = .current
        } else {
            tone = .pending
        }
        return WorkflowStageRailStepPresentation(
            route: route,
            titleKey: route.titleKey,
            iconName: iconName(route: route, tone: tone),
            tone: tone,
            accessibilityStateKey: accessibilityStateKey(tone: tone)
        )
    }

    private func iconName(route: ConverterNavigation, tone: WorkflowStageRailTone) -> String {
        if tone == .blocked {
            return "exclamationmark.triangle.fill"
        }
        if tone == .complete {
            return "checkmark.circle.fill"
        }
        return route.systemImage
    }

    private func accessibilityStateKey(tone: WorkflowStageRailTone) -> String {
        switch tone {
        case .blocked:
            return "Status.Blocked"
        case .complete:
            return "Status.Succeeded"
        case .current:
            return "Status.Running"
        case .pending:
            return "Status.NotReady"
        }
    }
}
