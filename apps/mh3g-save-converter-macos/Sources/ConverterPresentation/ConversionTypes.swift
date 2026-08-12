import Foundation

/// The Rust CLI operations that the UI is permitted to invoke.  Keeping this
/// vocabulary closed prevents presentation code from turning into a generic
/// shell launcher.
public enum ConverterOperation: String, CaseIterable, Codable, Sendable {
    case inspect
    case convert
    case repairConverted = "repair-converted"
    case repairExtras = "repair-extras"
    case repairSystem = "repair-system"
    case repairCEC = "repair-cec"
    case rollbackRepair = "rollback-repair"
    case convertSystem = "convert-system"
    case convertExtras = "convert-extras"
    case installExtras = "install-extras"
    case convertCEC = "convert-cec"
    case rollback
    case rollbackExtras = "rollback-extras"
    case rollbackCEC = "rollback-cec"
}

public enum ConversionMode: String, CaseIterable, Identifiable, Sendable {
    case newConversion
    case repairConverted

    public var id: String { rawValue }
}

public enum HistoricalConverterRevision: String, CaseIterable, Identifiable, Codable, Sendable {
    case v0_0_3 = "0.0.3"
    case v0_0_4 = "0.0.4"
    case v0_0_5 = "0.0.5"
    case v0_0_6 = "0.0.6"

    public var id: String { rawValue }
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

/// Explicit core paths. New conversion uses source + target. Compatibility
/// repair additionally requires a read-only current Wii U/Cemu reference;
/// target always remains the independent write destination.
public struct ConversionInput: Equatable, Sendable {
    public let source: URL
    public let current: URL?
    public let target: URL

    public init(source: URL, target: URL, current: URL? = nil) {
        self.source = source.standardizedFileURL
        self.current = current?.standardizedFileURL
        self.target = target.standardizedFileURL
    }
}

/// A core conversion always addresses one of the three named save slots. The
/// UI may accept a selected directory for convenience, but it resolves that
/// choice to this exact child file before it ever constructs CLI arguments.
public enum SaveSlot: String, CaseIterable, Identifiable, Sendable {
    case user1
    case user2
    case user3

    public var id: String { rawValue }

    public init?(fileName: String) {
        self.init(rawValue: fileName.lowercased())
    }
}

public enum SavePathResolutionError: LocalizedError, Equatable, Sendable {
    case sourceSlotMissing(slot: SaveSlot, directory: URL)
    case slotNameMismatch(expected: SaveSlot, actual: String)
    case extDataUserDirectoryMissing(URL)

    public var errorDescription: String? {
        switch self {
        case .sourceSlotMissing(let slot, let directory):
            "\(slot.rawValue) is not directly inside \(directory.path)."
        case .slotNameMismatch(let expected, let actual):
            "The selected slot is \(expected.rawValue), but the file is \(actual)."
        case .extDataUserDirectoryMissing(let directory):
            "The selected ExtData location does not contain a direct user directory: \(directory.path)."
        }
    }
}

public enum SavePathResolver {
    /// A direct `user#` file is accepted as-is. A directory is never searched
    /// recursively; only its direct `user#` child is considered.
    public static func resolveSource(selection: URL, slot: SaveSlot, fileManager: FileManager = .default) throws -> URL {
        let selected = selection.standardizedFileURL
        if isDirectory(selected, fileManager: fileManager) {
            let candidate = selected.appendingPathComponent(slot.rawValue)
            guard fileManager.fileExists(atPath: candidate.path) else {
                throw SavePathResolutionError.sourceSlotMissing(slot: slot, directory: selected)
            }
            return candidate.standardizedFileURL
        }
        try validate(file: selected, matches: slot)
        return selected
    }

    /// A selected directory represents an explicit export location and is
    /// resolved to `<directory>/user#`. It deliberately creates nothing; the
    /// transaction layer remains the only writer.
    public static func resolveTarget(selection: URL, slot: SaveSlot, fileManager: FileManager = .default) throws -> URL {
        let selected = selection.standardizedFileURL
        if isDirectory(selected, fileManager: fileManager) {
            return selected.appendingPathComponent(slot.rawValue).standardizedFileURL
        }
        try validate(file: selected, matches: slot)
        return selected
    }

    /// ExtData is accepted only as the precise `user` directory or its direct
    /// `00000481` parent. This helps common SDMC layouts without guessing a
    /// broader SD-card or emulator root.
    public static func resolveExtDataUserDirectory(selection: URL, fileManager: FileManager = .default) throws -> URL {
        let selected = selection.standardizedFileURL
        if selected.lastPathComponent.lowercased() == "user", isDirectory(selected, fileManager: fileManager) {
            return selected
        }
        let candidate = selected.appendingPathComponent("user", isDirectory: true)
        guard isDirectory(candidate, fileManager: fileManager) else {
            throw SavePathResolutionError.extDataUserDirectoryMissing(selected)
        }
        return candidate.standardizedFileURL
    }

    public static func slot(for selection: URL) -> SaveSlot? {
        SaveSlot(fileName: selection.lastPathComponent)
    }

    private static func validate(file: URL, matches slot: SaveSlot) throws {
        let actual = file.lastPathComponent.lowercased()
        guard actual == slot.rawValue else {
            throw SavePathResolutionError.slotNameMismatch(expected: slot, actual: actual)
        }
    }

    private static func isDirectory(_ url: URL, fileManager: FileManager) -> Bool {
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory) else { return false }
        return isDirectory.boolValue
    }
}

/// Optional data groups.  Each group is deliberately represented as a single
/// boolean: the Rust transaction owns the indivisible card#/quest# set.
public struct ComponentSelection: Equatable, Sendable {
    public var includeSystem: Bool
    public var includeGuildCards: Bool
    public var includeQuests: Bool
    public var systemSource: URL?
    public var systemCurrent: URL?
    public var systemTarget: URL?
    public var extraSourceDirectory: URL?
    public var extraCurrentDirectory: URL?
    public var extraStagingDirectory: URL?
    public var extraTargetDirectory: URL?
    public var cecSourceDirectory: URL?
    public var cecCurrent: URL?
    public var cecTarget: URL?
    public var acknowledgeExperimentalCEC: Bool

    public init(
        includeSystem: Bool = false,
        includeGuildCards: Bool = false,
        includeQuests: Bool = false,
        systemSource: URL? = nil,
        systemCurrent: URL? = nil,
        systemTarget: URL? = nil,
        extraSourceDirectory: URL? = nil,
        extraCurrentDirectory: URL? = nil,
        extraStagingDirectory: URL? = nil,
        extraTargetDirectory: URL? = nil,
        cecSourceDirectory: URL? = nil,
        cecCurrent: URL? = nil,
        cecTarget: URL? = nil,
        acknowledgeExperimentalCEC: Bool = false
    ) {
        self.includeSystem = includeSystem
        self.includeGuildCards = includeGuildCards
        self.includeQuests = includeQuests
        self.systemSource = systemSource?.standardizedFileURL
        self.systemCurrent = systemCurrent?.standardizedFileURL
        self.systemTarget = systemTarget?.standardizedFileURL
        self.extraSourceDirectory = extraSourceDirectory?.standardizedFileURL
        self.extraCurrentDirectory = extraCurrentDirectory?.standardizedFileURL
        self.extraStagingDirectory = extraStagingDirectory?.standardizedFileURL
        self.extraTargetDirectory = extraTargetDirectory?.standardizedFileURL
        self.cecSourceDirectory = cecSourceDirectory?.standardizedFileURL
        self.cecCurrent = cecCurrent?.standardizedFileURL
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
        cecSourceDirectory != nil || cecCurrent != nil || cecTarget != nil
    }
}

/// Authorization for the mandatory character slot is a statement about that
/// exact source/target pair, not a timestamp, UI route, or optional component
/// selection. `system`, ExtData, and CEC establish their own authorization
/// boundaries and must not invalidate a verified core-slot Dry Run.
public struct DryRunFingerprint: Equatable, Sendable {
    public let sourceSHA256: String
    /// An existing Cemu slot is pinned by its SHA-256. A missing target is an
    /// explicit new-export authorization and is protected by the CLI's
    /// `--expected-target-absent` precondition instead.
    public let targetSHA256: String?
    /// Hash of the exact bytes the read-only converter preview said it would
    /// install. A later `written` report must echo this value as `output`.
    public let outputSHA256: String

    public init(
        sourceSHA256: String,
        targetSHA256: String?,
        outputSHA256: String
    ) {
        self.sourceSHA256 = sourceSHA256
        self.targetSHA256 = targetSHA256
        self.outputSHA256 = outputSHA256
    }

    public var exportsNewTarget: Bool {
        targetSHA256 == nil
    }
}

public struct RepairDryRunFingerprint: Equatable, Sendable {
    public let source: URL
    public let current: URL
    public let output: URL
    public let extDataSource: URL?
    public let fromVersion: HistoricalConverterRevision?
    public let sourceSetSHA256: String
    public let currentSetSHA256: String
    public let outputSetSHA256: String
    public let previewSHA256: String
    public let components: [RepairComponentFingerprint]

    public init(
        source: URL,
        current: URL,
        output: URL,
        extDataSource: URL?,
        fromVersion: HistoricalConverterRevision?,
        sourceSetSHA256: String,
        currentSetSHA256: String,
        outputSetSHA256: String,
        previewSHA256: String,
        components: [RepairComponentFingerprint]
    ) {
        self.source = source.standardizedFileURL
        self.current = current.standardizedFileURL
        self.output = output.standardizedFileURL
        self.extDataSource = extDataSource?.standardizedFileURL
        self.fromVersion = fromVersion
        self.sourceSetSHA256 = sourceSetSHA256
        self.currentSetSHA256 = currentSetSHA256
        self.outputSetSHA256 = outputSetSHA256
        self.previewSHA256 = previewSHA256
        self.components = components
    }
}

public struct RepairComponentFingerprint: Equatable, Sendable {
    public let component: String
    public let target: URL
    public let sourceSHA256: String
    public let currentSHA256: String
    public let mergedSHA256: String
    public let modified: Bool
}

public struct RepairExtrasDryRunFingerprint: Equatable, Sendable {
    public let group: ExtraGroup
    public let sourceDirectory: URL
    public let currentDirectory: URL
    public let outputDirectory: URL
    public let fromVersion: HistoricalConverterRevision?
    public let sourceSetSHA256: String
    public let currentSetSHA256: String
    public let outputSetSHA256: String
    public let previewSHA256: String
    public let components: [RepairComponentFingerprint]

    public init(
        group: ExtraGroup,
        sourceDirectory: URL,
        currentDirectory: URL,
        outputDirectory: URL,
        fromVersion: HistoricalConverterRevision?,
        sourceSetSHA256: String,
        currentSetSHA256: String,
        outputSetSHA256: String,
        previewSHA256: String,
        components: [RepairComponentFingerprint]
    ) {
        self.group = group
        self.sourceDirectory = sourceDirectory.standardizedFileURL
        self.currentDirectory = currentDirectory.standardizedFileURL
        self.outputDirectory = outputDirectory.standardizedFileURL
        self.fromVersion = fromVersion
        self.sourceSetSHA256 = sourceSetSHA256
        self.currentSetSHA256 = currentSetSHA256
        self.outputSetSHA256 = outputSetSHA256
        self.previewSHA256 = previewSHA256
        self.components = components
    }
}

public struct RepairSystemDryRunFingerprint: Equatable, Sendable {
    public let source: URL
    public let current: URL
    public let output: URL
    public let sourceSetSHA256: String
    public let currentSetSHA256: String
    public let outputSetSHA256: String
    public let previewSHA256: String

    public init(
        source: URL,
        current: URL,
        output: URL,
        sourceSetSHA256: String,
        currentSetSHA256: String,
        outputSetSHA256: String,
        previewSHA256: String
    ) {
        self.source = source.standardizedFileURL
        self.current = current.standardizedFileURL
        self.output = output.standardizedFileURL
        self.sourceSetSHA256 = sourceSetSHA256
        self.currentSetSHA256 = currentSetSHA256
        self.outputSetSHA256 = outputSetSHA256
        self.previewSHA256 = previewSHA256
    }
}

/// `system` is a distinct 3DS/Wii U file pair, so it must retain its own
/// authorization instead of borrowing the selected `user#` slot fingerprint.
public struct SystemDryRunFingerprint: Equatable, Sendable {
    public let source: URL
    public let target: URL
    public let sourceSHA256: String
    public let targetSHA256: String
    public let outputSHA256: String

    public init(
        source: URL,
        target: URL,
        sourceSHA256: String,
        targetSHA256: String,
        outputSHA256: String
    ) {
        self.source = source.standardizedFileURL
        self.target = target.standardizedFileURL
        self.sourceSHA256 = sourceSHA256
        self.targetSHA256 = targetSHA256
        self.outputSHA256 = outputSHA256
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
    public let entries: [ExtraInstallEntryFingerprint]

    public init(
        stagingDirectory: URL,
        targetDirectory: URL,
        groups: Set<ExtraGroup>,
        stagingSetSHA256: String,
        targetSetSHA256: String,
        entries: [ExtraInstallEntryFingerprint]
    ) {
        self.stagingDirectory = stagingDirectory.standardizedFileURL
        self.targetDirectory = targetDirectory.standardizedFileURL
        self.groups = groups
        self.stagingSetSHA256 = stagingSetSHA256
        self.targetSetSHA256 = targetSetSHA256
        self.entries = entries
    }
}

public struct ExtraInstallEntryFingerprint: Equatable, Sendable {
    public let group: ExtraGroup
    public let component: String
    public let target: URL
    public let beforeSHA256: String?
    public let afterSHA256: String
    public let targetPreviouslyExisted: Bool
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
    public let targetSHA256After: String
    public let targetExisted: Bool

    public init(
        sourceDirectory: URL,
        target: URL,
        sourceRecordSetSHA256: String,
        targetSHA256Before: String,
        targetSHA256After: String,
        targetExisted: Bool
    ) {
        self.sourceDirectory = sourceDirectory.standardizedFileURL
        self.target = target.standardizedFileURL
        self.sourceRecordSetSHA256 = sourceRecordSetSHA256
        self.targetSHA256Before = targetSHA256Before
        self.targetSHA256After = targetSHA256After
        self.targetExisted = targetExisted
    }
}

public struct RepairCECDryRunFingerprint: Equatable, Sendable {
    public let sourceDirectory: URL
    public let current: URL
    public let output: URL
    public let sourceRecordSetSHA256: String
    public let currentSetSHA256: String
    public let outputSetSHA256: String
    public let previewSHA256: String

    public init(
        sourceDirectory: URL,
        current: URL,
        output: URL,
        sourceRecordSetSHA256: String,
        currentSetSHA256: String,
        outputSetSHA256: String,
        previewSHA256: String
    ) {
        self.sourceDirectory = sourceDirectory.standardizedFileURL
        self.current = current.standardizedFileURL
        self.output = output.standardizedFileURL
        self.sourceRecordSetSHA256 = sourceRecordSetSHA256
        self.currentSetSHA256 = currentSetSHA256
        self.outputSetSHA256 = outputSetSHA256
        self.previewSHA256 = previewSHA256
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
    public let group: ExtraGroup?
    public let stagingSetSHA256: String?
    public let targetSetSHA256Before: String?
    public let sourceSetSHA256: String?
    public let currentSetSHA256: String?
    public let outputSetSHA256: String?
    public let previewSHA256: String?
    public let detection: ConverterRevisionDetection?
    public let manifests: [String]?
    public let compatibilityManifest: String?
    public let sourceDirectory: String?
    public let outputDirectory: String?
    public let stagingDirectory: String?
    public let targetDirectory: String?
    public let source: String?
    public let current: String?
    public let currentDirectory: String?
    public let target: String?
    public let entries: [ConverterExtraInstallEntry]?
    public let backupPaths: [String]?
    public let output: String?
    public let backup: String?
    public let manifest: String?
    public let stderr: String?

    enum CodingKeys: String, CodingKey {
        case operation, status, profile, size, hashes, output, backup, manifest, stderr, components, groups, group, manifests, detection
        case source, current, target, entries
        case compatibilityManifest = "compatibility_manifest"
        case sourceDirectory = "source_dir"
        case outputDirectory = "output_dir"
        case currentDirectory = "current_dir"
        case stagingDirectory = "staging_dir"
        case targetDirectory = "target_dir"
        case backupPaths = "backup_paths"
        case sourceSHA256 = "source_sha256"
        case targetSHA256Before = "target_sha256_before"
        case targetSHA256After = "target_sha256_after"
        case sourceRecordSHA256 = "source_record_sha256"
        case sourceRecordSetSHA256 = "source_record_set_sha256"
        case stagingSetSHA256 = "staging_set_sha256"
        case targetSetSHA256Before = "target_set_sha256_before"
        case sourceSetSHA256 = "source_set_sha256"
        case currentSetSHA256 = "current_set_sha256"
        case outputSetSHA256 = "output_set_sha256"
        case previewSHA256 = "preview_sha256"
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
    public let sourceSHA256: String?
    public let outputSHA256: String?
    public let detection: ConverterRevisionDetection?
    public let output: String?
    public let size: Int?
    public let target: String?
    public let modified: Bool?
    public let merge: ConverterCompatibilityMerge?

    enum CodingKeys: String, CodingKey {
        case component, detection, output, size, target, modified, merge
        case sourceSHA256 = "source_sha256"
        case outputSHA256 = "output_sha256"
    }

    public func fingerprint() -> ExtraComponentFingerprint? {
        guard let sourceSHA256, let outputSHA256 else { return nil }
        return ExtraComponentFingerprint(
            component: component,
            sourceSHA256: sourceSHA256,
            outputSHA256: outputSHA256
        )
    }

    public func repairFingerprint() -> RepairComponentFingerprint? {
        guard let target,
              let modified,
              let merge,
              merge.component == component,
              ConverterEvidence.isValidSHA256(merge.sourceSHA256),
              ConverterEvidence.isValidSHA256(merge.currentSHA256),
              ConverterEvidence.isValidSHA256(merge.mergedSHA256)
        else { return nil }
        return RepairComponentFingerprint(
            component: component,
            target: URL(fileURLWithPath: target).standardizedFileURL,
            sourceSHA256: merge.sourceSHA256,
            currentSHA256: merge.currentSHA256,
            mergedSHA256: merge.mergedSHA256,
            modified: modified
        )
    }
}

public struct ConverterCompatibilityMerge: Decodable, Sendable {
    public let component: String
    public let sourceSHA256: String
    public let currentSHA256: String
    public let mergedSHA256: String

    enum CodingKeys: String, CodingKey {
        case component
        case sourceSHA256 = "source_sha256"
        case currentSHA256 = "current_sha256"
        case mergedSHA256 = "merged_sha256"
    }
}

public struct ConverterExtraInstallEntry: Decodable, Sendable {
    public let group: ExtraGroup
    public let component: String
    public let target: String
    public let beforeSHA256: String?
    public let afterSHA256: String
    public let backup: String?
    public let targetPreviouslyExisted: Bool

    enum CodingKeys: String, CodingKey {
        case group, component, target, backup
        case beforeSHA256 = "before_sha256"
        case afterSHA256 = "after_sha256"
        case targetPreviouslyExisted = "target_previously_existed"
    }

    public func fingerprint() -> ExtraInstallEntryFingerprint {
        ExtraInstallEntryFingerprint(
            group: group,
            component: component,
            target: URL(fileURLWithPath: target).standardizedFileURL,
            beforeSHA256: beforeSHA256,
            afterSHA256: afterSHA256,
            targetPreviouslyExisted: targetPreviouslyExisted
        )
    }
}

public struct ConverterRevisionDetection: Decodable, Sendable {
    public let confidence: String
    public let candidates: [HistoricalConverterRevision]
}
