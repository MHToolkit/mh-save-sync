import Foundation
import Observation

public enum ConverterLanguage: String, CaseIterable, Codable, Sendable {
    case system
    case zhHans = "zh-Hans"
    case english = "en"
}

@MainActor
@Observable
public final class ConverterLocaleSettings {
    public static let storageKey = "mh3g.converter.localeOverride"

    public var override: ConverterLanguage {
        didSet { defaults.set(override.rawValue, forKey: Self.storageKey) }
    }

    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        self.override = ConverterLanguage(rawValue: defaults.string(forKey: Self.storageKey) ?? "") ?? .system
    }

    public func resolvedLanguage(systemIdentifier: String = Locale.current.identifier) -> ConverterLanguage {
        guard override == .system else { return override }
        let normalized = systemIdentifier.replacingOccurrences(of: "_", with: "-").lowercased()
        return normalized.hasPrefix("zh") ? .zhHans : .english
    }
}

public enum ConverterNavigation: String, CaseIterable, Identifiable, Sendable {
    case input
    case components
    case dryRun
    case writeRollback
    case history
    case experimentalCEC
    case settings

    public var id: String { rawValue }

    public var titleKey: String {
        switch self {
        case .input: "Navigation.Input"
        case .components: "Navigation.Components"
        case .dryRun: "Navigation.DryRun"
        case .writeRollback: "Navigation.WriteRollback"
        case .history: "Navigation.History"
        case .experimentalCEC: "Navigation.ExperimentalCEC"
        case .settings: "Navigation.Settings"
        }
    }

    public var systemImage: String {
        switch self {
        case .input: "doc.text.magnifyingglass"
        case .components: "square.stack.3d.up"
        case .dryRun: "arrow.triangle.2.circlepath"
        case .writeRollback: "arrow.uturn.backward.circle"
        case .history: "clock.arrow.circlepath"
        case .experimentalCEC: "envelope.badge"
        case .settings: "gearshape"
        }
    }

    public var accessibilityIdentifier: String { "mh3g.converter.navigation.\(rawValue)" }
}

/// A small typed facade keeps visual code independent from raw localized-string
/// IDs.  The matching `.xcstrings` catalog is shipped for system tooling and
/// localization workflows; this table lets the locale override update text
/// immediately without restarting a SwiftPM-built app.
public enum ConverterCopy {
    private static let zhHans: [String: String] = [
        "App.Title": "MH3G 存档转换器",
        "Navigation.Input": "输入与检查",
        "Navigation.Components": "组件选择",
        "Navigation.DryRun": "Dry Run",
        "Navigation.WriteRollback": "写入与回滚",
        "Navigation.History": "转换历史",
        "Navigation.ExperimentalCEC": "实验性 CEC",
        "Navigation.Settings": "设置",
        "Input.Source": "3DS 源角色文件",
        "Input.Target": "Cemu 目标角色文件",
        "Input.Select": "选择…",
        "Input.Inspect": "检查所选文件",
        "Input.NoSelection": "尚未选择文件",
        "Components.System": "共享 system",
        "Components.GuildCards": "公会名片与离线伙伴",
        "Components.Quests": "任务",
        "Components.ExtDataSource": "3DS ExtData user 目录",
        "Components.Staging": "临时转换目录",
        "Components.Target": "Cemu ExtData 目录",
        "DryRun.Start": "运行 Dry Run",
        "DryRun.NotAuthorized": "完成检查后运行 Dry Run，写入授权会绑定当前 SHA-256 与已选组件。",
        "Write.Confirm": "确认写入",
        "Write.Unavailable": "需要当前有效的 Dry Run 才能写入。",
        "Write.Rollback": "从 manifest 回滚",
        "CEC.Acknowledge": "我了解 CEC 导入仍属实验性功能",
        "CEC.Hidden": "实验性 CEC 默认关闭；普通名片与离线伙伴迁移不依赖它。",
        "Settings.Language": "语言",
        "Settings.Language.System": "跟随系统",
        "Settings.Language.Chinese": "简体中文",
        "Settings.Language.English": "English",
        "Status.Ready": "已就绪",
        "Status.Running": "正在执行…",
        "Status.Failed": "操作失败",
        "Error.Detail": "完整错误详情",
        "Input.SHA256": "SHA-256",
        "Input.Profile": "存档类型",
        "Input.Bytes": "字节数",
        "Input.SourceMessage": "请选择一个 3DS 的 user1、user2 或 user3 文件。",
        "Input.TargetMessage": "请选择匹配的 Cemu user1、user2 或 user3 文件。",
        "Input.InvalidSlot": "请选择 user1、user2 或 user3 文件；不接受文件夹或压缩包。",
        "Components.Subtitle": "可选数据必须由你明确启用。名片与任务文件始终按 Rust 事务定义的完整组安装。",
        "Components.SystemSource": "3DS system",
        "Components.SystemTarget": "Cemu system",
        "Components.SystemFooter": "已选择的 system 只对应一个明确文件，不会扩展为整个存档目录。",
        "Components.SystemSourceMessage": "请选择一个 3DS system 文件。",
        "Components.SystemTargetMessage": "请选择一个 Cemu system 文件。",
        "Components.ExtrasFooter": "请自行选择文档中指定的 3DS ExtData user 目录与 Cemu 存档目录。应用不会猜测 MLC 根目录，也不会覆盖整个目录。",
        "Components.ExtDataSourceMessage": "请选择准确的 3DS .../extdata/00000000/00000481/user 目录。",
        "Components.StagingMessage": "请选择一个新的空目录，用于暂存转换后的 ExtData 组件。",
        "Components.TargetMessage": "请选择准确的 Cemu MH3G 存档目录，其中应包含 card1/card2/card3/cardbox 与 quest1 至 quest4。",
        "Components.GroupScope": "完整事务组",
        "DryRun.BackupManifest": "备份与 manifest",
        "DryRun.BackupPending": "只会在事务写入时创建",
        "DryRun.BackupAvailable": "成功完成 Dry Run 后可写入时创建",
        "DryRun.ReadOnly": "只读流程",
        "DryRun.Authorized": "已授权当前源文件与目标文件的 SHA-256",
        "DryRun.CoreOnly": "仅核心角色存档",
        "DryRun.Source": "源文件",
        "DryRun.Target": "目标文件",
        "Write.Subtitle": "核心角色写入由事务保护。替换所选 Cemu 目标前会先创建备份和 manifest。",
        "Write.Authorization": "写入授权",
        "Write.CurrentAuthorized": "当前 Dry Run 有效",
        "Write.OptionalDeferred": "可选组需要各自的组级 Dry Run，暂不包含在此核心角色事务中。",
        "Write.Footer": "确认页只列出一个目标文件。它不会复制 MLC，也不会递归替换目录。",
        "Write.Manifest": "Manifest",
        "Write.SelectManifest": "选择事务 manifest",
        "Write.SelectManifestMessage": "请选择由此前转换创建的准确 .mh3g-install.json manifest。",
        "Write.RollbackFooter": "回滚绑定到所选 manifest，不能指向普通目标目录。",
        "Write.Files": "文件",
        "Write.OneTarget": "1 个选定目标",
        "Write.Target": "目标",
        "Write.Backup": "备份",
        "Write.OneBackup": "替换前创建 1 份事务备份",
        "Write.ManifestCreated": "在所选目标旁创建",
        "Write.ExperimentalCEC": "实验性 CEC",
        "Write.NotIncluded": "不包含",
        "Write.Cancel": "取消",
        "Write.SelectedSave": "写入所选存档",
        "CEC.Source": "3DS CEC 邮箱",
        "CEC.Target": "Cemu CEC 缓存",
        "CEC.Warning": "CEC 与 card1/card2/card3/cardbox 相互独立。选择名片组绝不会自动启用 CEC。",
        "CEC.Disclosure": "导入 CEC 邮箱",
        "CEC.Footer": "在界面开放实验性写入命令前，CEC 必须有独立的检查与 Dry Run 报告。",
        "CEC.SourceMessage": "请选择准确的 .../CEC/00048100。应用不会搜索 SD 卡或 MLC。",
        "CEC.TargetMessage": "请选择准确的 Cemu MH3G cec 文件。",
        "History.Subtitle": "此窗口显示当前会话的报告。可持久化的回滚证据仍保留在所选目标旁的 Rust manifest 中。",
        "History.Operation": "操作",
        "History.Status": "状态",
        "History.Backup": "备份",
        "History.Manifest": "Manifest",
        "History.Empty": "本次会话尚无转换报告",
        "History.EmptyDescription": "运行只读检查或 Dry Run 后，可在此查看其结构化结果。",
        "Settings.Subtitle": "语言会立即切换。转换器本身仍是本地 JSON/argv 工具，不使用云端账号，也不会自动发现目录。",
        "Settings.Diagnostics": "诊断信息",
        "Settings.UI": "界面",
        "Settings.CLI": "转换器 CLI",
        "Settings.WorkflowState": "流程状态",
        "Settings.BundledSidecar": "打包时使用内置 sidecar",
        "Settings.Development": "开发版本",
    ]

    private static let english: [String: String] = [
        "App.Title": "MH3G Save Converter",
        "Navigation.Input": "Input & Inspect",
        "Navigation.Components": "Components",
        "Navigation.DryRun": "Dry Run",
        "Navigation.WriteRollback": "Write & Rollback",
        "Navigation.History": "History",
        "Navigation.ExperimentalCEC": "Experimental CEC",
        "Navigation.Settings": "Settings",
        "Input.Source": "3DS source character file",
        "Input.Target": "Cemu target character file",
        "Input.Select": "Choose…",
        "Input.Inspect": "Inspect selected files",
        "Input.NoSelection": "No file selected",
        "Components.System": "Shared system",
        "Components.GuildCards": "Guild cards & offline partners",
        "Components.Quests": "Quests",
        "Components.ExtDataSource": "3DS ExtData user directory",
        "Components.Staging": "Conversion staging directory",
        "Components.Target": "Cemu ExtData directory",
        "DryRun.Start": "Run Dry Run",
        "DryRun.NotAuthorized": "Inspect both files, then run Dry Run. Write authorization is bound to the current SHA-256 values and selected components.",
        "Write.Confirm": "Confirm write",
        "Write.Unavailable": "A current successful Dry Run is required before writing.",
        "Write.Rollback": "Roll back from manifest",
        "CEC.Acknowledge": "I understand that CEC import is experimental",
        "CEC.Hidden": "Experimental CEC is off by default. Guild-card and offline-partner migration does not depend on it.",
        "Settings.Language": "Language",
        "Settings.Language.System": "System default",
        "Settings.Language.Chinese": "Simplified Chinese",
        "Settings.Language.English": "English",
        "Status.Ready": "Ready",
        "Status.Running": "Working…",
        "Status.Failed": "Operation failed",
        "Error.Detail": "Full error details",
        "Input.SHA256": "SHA-256",
        "Input.Profile": "Profile",
        "Input.Bytes": "Bytes",
        "Input.SourceMessage": "Choose one 3DS user1, user2, or user3 file.",
        "Input.TargetMessage": "Choose the matching Cemu user1, user2, or user3 file.",
        "Input.InvalidSlot": "Choose exactly user1, user2, or user3; folders and archives are not accepted.",
        "Components.Subtitle": "Optional data stays opt-in. Card and quest files are always installed as complete Rust-owned groups.",
        "Components.SystemSource": "3DS system",
        "Components.SystemTarget": "Cemu system",
        "Components.SystemFooter": "A selected system is one explicit file. It never expands to a save directory.",
        "Components.SystemSourceMessage": "Choose exactly one 3DS system file.",
        "Components.SystemTargetMessage": "Choose exactly one Cemu system file.",
        "Components.ExtrasFooter": "Choose the documented 3DS ExtData user directory and Cemu save directory yourself. The app will not infer an MLC root or overwrite an entire directory.",
        "Components.ExtDataSourceMessage": "Choose the exact 3DS .../extdata/00000000/00000481/user directory.",
        "Components.StagingMessage": "Choose a new empty staging directory for converted ExtData components.",
        "Components.TargetMessage": "Choose the exact Cemu MH3G save directory that contains card1/card2/card3/cardbox and quest1 through quest4.",
        "Components.GroupScope": "Complete transaction group",
        "DryRun.BackupManifest": "Backup + manifest",
        "DryRun.BackupPending": "Will be created only by the transactional write",
        "DryRun.BackupAvailable": "Available after a successful Dry Run",
        "DryRun.ReadOnly": "Read-only flow",
        "DryRun.Authorized": "Authorized for current source and target SHA-256",
        "DryRun.CoreOnly": "Core slot only",
        "DryRun.Source": "Source",
        "DryRun.Target": "Target",
        "Write.Subtitle": "The core slot write is transaction-backed. It creates a backup and manifest before replacing the chosen Cemu target.",
        "Write.Authorization": "Write authorization",
        "Write.CurrentAuthorized": "Current Dry Run is valid",
        "Write.OptionalDeferred": "Optional groups require their own group Dry Run and are not included in this primary-slot transaction yet.",
        "Write.Footer": "The confirmation lists exactly one target file. It never copies an MLC or recursively replaces a directory.",
        "Write.Manifest": "Manifest",
        "Write.SelectManifest": "Select transaction manifest",
        "Write.SelectManifestMessage": "Choose the exact .mh3g-install.json manifest created by a prior conversion.",
        "Write.RollbackFooter": "Rollback is bound to the selected manifest. It cannot be pointed at a generic target directory.",
        "Write.Files": "Files",
        "Write.OneTarget": "1 selected target",
        "Write.Target": "Target",
        "Write.Backup": "Backup",
        "Write.OneBackup": "1 transaction backup before replacement",
        "Write.ManifestCreated": "Created beside the selected target",
        "Write.ExperimentalCEC": "Experimental CEC",
        "Write.NotIncluded": "Not included",
        "Write.Cancel": "Cancel",
        "Write.SelectedSave": "Write selected save",
        "CEC.Source": "3DS CEC mailbox",
        "CEC.Target": "Cemu CEC cache",
        "CEC.Warning": "CEC remains separate from card1/card2/card3/cardbox. It is never enabled by choosing a guild-card group.",
        "CEC.Disclosure": "CEC mailbox import",
        "CEC.Footer": "The UI will require a separate CEC inspection and Dry Run report before it exposes the experimental write command.",
        "CEC.SourceMessage": "Choose exactly .../CEC/00048100. The app does not search an SD card or MLC.",
        "CEC.TargetMessage": "Choose exactly the Cemu MH3G cec file.",
        "History.Subtitle": "This window shows the current session report. Durable rollback evidence remains in the Rust manifest next to the selected target.",
        "History.Operation": "Operation",
        "History.Status": "Status",
        "History.Backup": "Backup",
        "History.Manifest": "Manifest",
        "History.Empty": "No conversion report in this session",
        "History.EmptyDescription": "Run a read-only inspection or Dry Run to see its structured result here.",
        "Settings.Subtitle": "Language changes immediately. The converter itself remains a local JSON/argv tool with no cloud account or automatic directory discovery.",
        "Settings.Diagnostics": "Diagnostics",
        "Settings.UI": "UI",
        "Settings.CLI": "Converter CLI",
        "Settings.WorkflowState": "Workflow state",
        "Settings.BundledSidecar": "Bundled sidecar when packaged",
        "Settings.Development": "development",
    ]

    /// The complete set of copy IDs rendered by the SwiftUI workbench.  Tests
    /// assert that each ID has a non-fallback value in both supported languages.
    public static var visibleKeys: [String] { zhHans.keys.sorted() }

    public static func text(_ key: String, language: ConverterLanguage) -> String {
        let table = language == .zhHans ? zhHans : english
        return table[key] ?? key
    }
}
