import Foundation

/// The Rust CLI operations that the UI is permitted to invoke.  Keeping this
/// vocabulary closed prevents presentation code from turning into a generic
/// shell launcher.
public enum ConverterOperation: String, CaseIterable, Codable, Sendable {
    case inspect
    case convert
    case convertSystem = "convert-system"
    case convertExtras = "convert-extras"
    case installExtras = "install-extras"
    case convertCEC = "convert-cec"
    case rollback
    case rollbackExtras = "rollback-extras"
    case rollbackCEC = "rollback-cec"
}

public enum WorkflowState: String, Equatable, Sendable {
    case input
    case componentSelection
    case dryRun
    case writing
    case success
    case failure
}

public enum ExtraGroup: String, CaseIterable, Codable, Hashable, Sendable {
    case guildCards = "guild-cards"
    case quests

    public var componentNames: [String] {
        switch self {
        case .guildCards:
            ["card1", "card2", "card3", "cardbox"]
        case .quests:
            ["quest1", "quest2", "quest3", "quest4"]
        }
    }
}

/// A Rust-derived read-only inspection of one explicitly selected file.
public struct InputInspection: Equatable, Sendable {
    public let profile: String
    public let size: Int
    public let sha256: String

    public init(profile: String, size: Int, sha256: String) {
        self.profile = profile
        self.size = size
        self.sha256 = sha256
    }
}

/// The two mandatory slot files.  The UI does not discover an MLC root or
/// expand a directory recursively: both URLs originate from explicit user
/// selection.
public struct ConversionInput: Equatable, Sendable {
    public let source: URL
    public let target: URL

    public init(source: URL, target: URL) {
        self.source = source.standardizedFileURL
        self.target = target.standardizedFileURL
    }
}

/// Optional data groups.  Each group is deliberately represented as a single
/// boolean: the Rust transaction owns the indivisible card#/quest# set.
public struct ComponentSelection: Equatable, Sendable {
    public var includeSystem: Bool
    public var includeGuildCards: Bool
    public var includeQuests: Bool
    public var systemSource: URL?
    public var systemTarget: URL?
    public var extraSourceDirectory: URL?
    public var extraStagingDirectory: URL?
    public var extraTargetDirectory: URL?
    public var cecSourceDirectory: URL?
    public var cecTarget: URL?
    public var acknowledgeExperimentalCEC: Bool

    public init(
        includeSystem: Bool = false,
        includeGuildCards: Bool = false,
        includeQuests: Bool = false,
        systemSource: URL? = nil,
        systemTarget: URL? = nil,
        extraSourceDirectory: URL? = nil,
        extraStagingDirectory: URL? = nil,
        extraTargetDirectory: URL? = nil,
        cecSourceDirectory: URL? = nil,
        cecTarget: URL? = nil,
        acknowledgeExperimentalCEC: Bool = false
    ) {
        self.includeSystem = includeSystem
        self.includeGuildCards = includeGuildCards
        self.includeQuests = includeQuests
        self.systemSource = systemSource?.standardizedFileURL
        self.systemTarget = systemTarget?.standardizedFileURL
        self.extraSourceDirectory = extraSourceDirectory?.standardizedFileURL
        self.extraStagingDirectory = extraStagingDirectory?.standardizedFileURL
        self.extraTargetDirectory = extraTargetDirectory?.standardizedFileURL
        self.cecSourceDirectory = cecSourceDirectory?.standardizedFileURL
        self.cecTarget = cecTarget?.standardizedFileURL
        self.acknowledgeExperimentalCEC = acknowledgeExperimentalCEC
    }

    public var selectedGroups: Set<ExtraGroup> {
        var groups = Set<ExtraGroup>()
        if includeGuildCards { groups.insert(.guildCards) }
        if includeQuests { groups.insert(.quests) }
        return groups
    }

    public var includesCEC: Bool {
        cecSourceDirectory != nil || cecTarget != nil
    }
}

/// Authorization for the mandatory character slot is a statement about that
/// exact source/target pair, not a timestamp, UI route, or optional component
/// selection. `system`, ExtData, and CEC establish their own authorization
/// boundaries and must not invalidate a verified core-slot Dry Run.
public struct DryRunFingerprint: Equatable, Sendable {
    public let sourceSHA256: String
    public let targetSHA256: String

    public init(
        sourceSHA256: String,
        targetSHA256: String
    ) {
        self.sourceSHA256 = sourceSHA256
        self.targetSHA256 = targetSHA256
    }
}

/// `system` is a distinct 3DS/Wii U file pair, so it must retain its own
/// authorization instead of borrowing the selected `user#` slot fingerprint.
public struct SystemDryRunFingerprint: Equatable, Sendable {
    public let source: URL
    public let target: URL
    public let sourceSHA256: String
    public let targetSHA256: String

    public init(source: URL, target: URL, sourceSHA256: String, targetSHA256: String) {
        self.source = source.standardizedFileURL
        self.target = target.standardizedFileURL
        self.sourceSHA256 = sourceSHA256
        self.targetSHA256 = targetSHA256
    }
}

/// One converted ExtData component as planned by `convert-extras`.  Both
/// hashes are retained so the UI can refuse to stage a changed source after a
/// successful preview without treating a directory as one opaque file.
public struct ExtraComponentFingerprint: Equatable, Sendable {
    public let component: String
    public let sourceSHA256: String
    public let outputSHA256: String

    public init(component: String, sourceSHA256: String, outputSHA256: String) {
        self.component = component
        self.sourceSHA256 = sourceSHA256
        self.outputSHA256 = outputSHA256
    }
}

/// The authorization for creating a new staging directory.  This deliberately
/// does not authorize installation into Cemu: installation has its own
/// target-set fingerprint after staging exists.
public struct ExtrasStageDryRunFingerprint: Equatable, Sendable {
    public let sourceDirectory: URL
    public let stagingDirectory: URL
    public let groups: Set<ExtraGroup>
    public let components: [ExtraComponentFingerprint]

    public init(
        sourceDirectory: URL,
        stagingDirectory: URL,
        groups: Set<ExtraGroup>,
        components: [ExtraComponentFingerprint]
    ) {
        self.sourceDirectory = sourceDirectory.standardizedFileURL
        self.stagingDirectory = stagingDirectory.standardizedFileURL
        self.groups = groups
        self.components = components
    }
}

/// The authorization for replacing the selected groups in Cemu.  The CLI
/// receives both values as expected hashes and checks them inside its own
/// transaction lock.
public struct ExtrasInstallDryRunFingerprint: Equatable, Sendable {
    public let stagingDirectory: URL
    public let targetDirectory: URL
    public let groups: Set<ExtraGroup>
    public let stagingSetSHA256: String
    public let targetSetSHA256: String

    public init(
        stagingDirectory: URL,
        targetDirectory: URL,
        groups: Set<ExtraGroup>,
        stagingSetSHA256: String,
        targetSetSHA256: String
    ) {
        self.stagingDirectory = stagingDirectory.standardizedFileURL
        self.targetDirectory = targetDirectory.standardizedFileURL
        self.groups = groups
        self.stagingSetSHA256 = stagingSetSHA256
        self.targetSetSHA256 = targetSetSHA256
    }
}

/// CEC is a mailbox directory plus a separate Cemu cache, not a `user#`
/// component. Its Dry Run therefore records the complete received record-set
/// fingerprint and cache fingerprint independently from the core-slot
/// authorization.
public struct CECDryRunFingerprint: Equatable, Sendable {
    public let sourceDirectory: URL
    public let target: URL
    public let sourceRecordSetSHA256: String
    public let targetSHA256Before: String

    public init(
        sourceDirectory: URL,
        target: URL,
        sourceRecordSetSHA256: String,
        targetSHA256Before: String
    ) {
        self.sourceDirectory = sourceDirectory.standardizedFileURL
        self.target = target.standardizedFileURL
        self.sourceRecordSetSHA256 = sourceRecordSetSHA256
        self.targetSHA256Before = targetSHA256Before
    }
}

public struct PlannedConverterCommand: Equatable, Sendable {
    public let operation: ConverterOperation
    public let arguments: [String]

    public init(operation: ConverterOperation, arguments: [String]) {
        self.operation = operation
        self.arguments = arguments
    }
}

public struct WorkflowFailure: Equatable, Sendable {
    public let operation: ConverterOperation
    public let message: String
    public let stderr: String

    public init(operation: ConverterOperation, message: String, stderr: String) {
        self.operation = operation
        self.message = message
        self.stderr = stderr
    }
}

public enum ConversionWorkflowError: Error, Equatable, Sendable {
    case operationInProgress(ConverterOperation)
    case inputNotInspected
    case dryRunRequired
    case staleDryRun
    case missingSystemPaths
    case missingExtraDirectories
    case missingCECDirectories
    case experimentalCECAcknowledgementRequired
    case invalidReport(String)
    case commandFailed(operation: ConverterOperation, stderr: String)
}

extension ConversionWorkflowError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .operationInProgress(let operation):
            "The \(operation.rawValue) operation is already in progress."
        case .inputNotInspected:
            "Select and inspect both the 3DS source and the Cemu target first."
        case .dryRunRequired:
            "A successful Dry Run is required before writing."
        case .staleDryRun:
            "Inputs or selected components changed after Dry Run. Run it again."
        case .missingSystemPaths:
            "Select both source and target system files."
        case .missingExtraDirectories:
            "Select the 3DS ExtData source, staging directory, and Cemu target directory."
        case .missingCECDirectories:
            "Select both CEC mailbox and target cache files."
        case .experimentalCECAcknowledgementRequired:
            "Experimental CEC import needs its separate acknowledgement."
        case .invalidReport(let detail):
            "The converter returned an invalid report: \(detail)"
        case .commandFailed(let operation, let stderr):
            "\(operation.rawValue) failed: \(stderr)"
        }
    }
}

/// A deliberately additive decoder for the JSON reports.  Old report keys can
/// coexist with the newer operation/source/target/report fields while the UI
/// migration is being rolled out; unknown keys are ignored by `Decodable`.
public struct ConverterReport: Decodable, Sendable {
    public let operation: String?
    public let status: String?
    public let profile: String?
    public let size: Int?
    public let hashes: [String: String]?
    public let sourceSHA256: String?
    public let targetSHA256Before: String?
    public let targetSHA256After: String?
    public let sourceRecordSHA256: [String]?
    public let sourceRecordSetSHA256: String?
    public let components: [ConverterExtraComponent]?
    public let groups: [ExtraGroup]?
    public let stagingSetSHA256: String?
    public let targetSetSHA256Before: String?
    public let output: String?
    public let backup: String?
    public let manifest: String?
    public let stderr: String?

    enum CodingKeys: String, CodingKey {
        case operation, status, profile, size, hashes, output, backup, manifest, stderr, components, groups
        case sourceSHA256 = "source_sha256"
        case targetSHA256Before = "target_sha256_before"
        case targetSHA256After = "target_sha256_after"
        case sourceRecordSHA256 = "source_record_sha256"
        case sourceRecordSetSHA256 = "source_record_set_sha256"
        case stagingSetSHA256 = "staging_set_sha256"
        case targetSetSHA256Before = "target_set_sha256_before"
    }

    public func hash(named name: String) -> String? {
        switch name {
        case "source": sourceSHA256 ?? hashes?["source"]
        case "target_before": targetSHA256Before ?? hashes?["target_before"]
        case "target_after": targetSHA256After ?? hashes?["target_after"]
        default: hashes?[name]
        }
    }
}

public struct ConverterExtraComponent: Decodable, Sendable {
    public let component: String
    public let sourceSHA256: String
    public let outputSHA256: String

    enum CodingKeys: String, CodingKey {
        case component
        case sourceSHA256 = "source_sha256"
        case outputSHA256 = "output_sha256"
    }

    public func fingerprint() -> ExtraComponentFingerprint {
        ExtraComponentFingerprint(
            component: component,
            sourceSHA256: sourceSHA256,
            outputSHA256: outputSHA256
        )
    }
}
