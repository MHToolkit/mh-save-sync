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
        case .dryRun: "checkmark.shield"
        case .writeRollback: "externaldrive.badge.checkmark"
        case .history: "clock.arrow.circlepath"
        case .experimentalCEC: "flask"
        case .settings: "gearshape"
        }
    }

    public var accessibilityIdentifier: String { "mh3g.converter.navigation.\(rawValue)" }

    /// The primary route remains a suggestion, not a lock: a user can revisit
    /// any sidebar section, while completed stages always expose one clear
    /// continuation for first-time conversion.
    public static func guidedSuccessor(after state: WorkflowState) -> Self? {
        switch state {
        case .componentSelection:
            .components
        case .dryRun:
            .writeRollback
        case .success:
            .history
        case .input, .writing, .failure:
            nil
        }
    }
}

public extension WorkflowState {
    /// Human-readable workflow phase for the diagnostics screen. The raw value
    /// is intentionally an implementation detail and must not be rendered.
    var localizationKey: String {
        switch self {
        case .input: "WorkflowState.Input"
        case .componentSelection: "WorkflowState.ComponentSelection"
        case .dryRun: "WorkflowState.DryRun"
        case .writing: "WorkflowState.Writing"
        case .success: "WorkflowState.Success"
        case .failure: "WorkflowState.Failure"
        }
    }
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
        "Input.Current": "当前 Wii U / Cemu 引用存档",
        "Input.CurrentReadOnly": "该存档只用于读取继续游玩后的数据，修复流程不会写入它。",
        "Input.Target": "Wii U / Cemu 输出位置",
        "Input.RepairOutput": "修复后的 Wii U / Cemu 输出位置",
        "Input.Required": "必选",
        "Input.Mode": "处理方式",
        "Input.Mode.New": "全新转换",
        "Input.Mode.Repair": "修复已转换存档",
        "Input.RepairHint": "请分别选择原始 3DS 存档、继续游玩后的当前 Wii U/Cemu 存档，以及独立输出位置。当前 Wii U 存档始终只读。",
        "Repair.Version": "原转换器版本",
        "Repair.Version.Auto": "自动检测",
        "Repair.Version.Hint": "不知道版本时先自动检测；若多个历史版本都匹配，Dry Run 会要求你明确选择后再运行。",
        "Repair.Version.Required": "检测到多个可能版本。请选择当时使用的转换器版本，然后重新运行 Dry Run。",
        "Repair.PreviewSHA256": "修复预览 SHA-256",
        "Components.RepairExtData": "可选：添加原始 3DS ExtData，修复已收到的公会名片；当前 card*/quest* 从 Wii U 存档目录自动读取。",
        "Input.Slot": "存档槽位",
        "Input.FinalOutput": "最终输出",
        "Input.NewOutput": "新输出文件；写入时必须保持不存在",
        "Input.Select": "选择…",
        "Input.SelectFile": "选择文件…",
        "Input.SelectDirectory": "选择目录…",
        "Input.SelectFileOrDirectory": "选择文件或目录…",
        "Input.SourceSelectionHint": "允许选择：单个 user# 文件，或直接包含所选 user# 的父目录。",
        "Input.CurrentSelectionHint": "允许选择：单个同槽位 Wii U/Cemu user# 文件，或它的直接父目录；必须已存在。",
        "Input.OutputSelectionHint": "允许选择：同名 user# 文件，或一个已有输出目录；选择目录时会解析为 <目录>/<所选 user#>。",
        "Input.PathGuideTitle": "支持单文件或直接父目录（查看结构示例）",
        "Input.PathGuideSummary": "只解析所选目录的直接同名 user#；不会递归扫描 SD 卡或 MLC，也不会读取 ZIP、7z、RAR。",
        "Input.PathGuideExample": "示例（选择 user2）\n\n3DS 存档直接父目录/\n├── system        （本控件不读取）\n└── user2         ← 3DS 源\n\n当前 Cemu 存档目录/\n├── user2         ← 当前引用（仅修复，只读）\n├── card1、card2、card3、cardbox   （修复名片时读取）\n└── quest1 … quest4               （修复名片时校验并保留）\n\n输出目录/\n├── user2         ← 转换结果（仅处理核心角色时可暂不存在）\n└── card1、card2、card3、cardbox   （修复名片并独立输出时必须已存在）",
        "Input.Inspect": "检查所选内容",
        "Input.NoSelection": "尚未选择文件",
        "Components.System": "共享 system",
        "Components.GuildCards": "公会名片与离线伙伴",
        "Components.Quests": "任务",
        "Components.ExtDataSource": "3DS ExtData user 目录",
        "Components.Staging": "临时转换目录",
        "Components.Target": "Cemu ExtData 目录",
        "DryRun.Start": "运行 Dry Run",
        "DryRun.NotAuthorized": "完成全部输入与输出检查后运行 Dry Run；修复授权会分别绑定 3DS 源、当前 Wii U 引用、输出状态与预览。",
        "Write.Confirm": "确认写入",
        "Write.Unavailable": "需要当前有效的 Dry Run 才能写入。",
        "Write.Rollback": "从 manifest 回滚",
        "CEC.Acknowledge": "我了解 CEC 导入仍属实验性功能",
        "CEC.Hidden": "实验性 CEC 默认关闭；普通名片与离线伙伴迁移不依赖它。",
        "Settings.Language": "语言",
        "Settings.Language.System": "跟随系统",
        "Settings.Language.Chinese": "简体中文",
        "Settings.Language.English": "English",
        "Status.NotReady": "尚未准备",
        "Status.NeedsInput": "选择存档开始",
        "Status.NeedsInspection": "等待只读检查",
        "Status.ReadyForDryRun": "可以运行 Dry Run",
        "Status.RevisionRequired": "需要选择原转换器版本",
        "Status.OptionalDataBlocked": "可选数据尚未配齐",
        "Status.SelectedWorkPending": "所选数据尚未全部完成",
        "Status.Blocked": "已阻止写入",
        "Status.Authorized": "Dry Run 已授权",
        "Status.Ready": "已就绪",
        "Status.Running": "正在执行…",
        "Status.Succeeded": "操作完成",
        "Status.Failed": "操作失败",
        "Status.Detail.NeedsInput": "全新转换需选择 3DS 源和输出；修复模式还必须独立选择当前 Wii U/Cemu 引用存档。应用不会自动扫描目录。",
        "Status.Detail.NeedsInspection": "运行只读检查以确认存档类型、大小与 SHA-256；此步骤不会写入文件。",
        "Status.Detail.ReadyForDryRun": "输入检查已通过。Dry Run 会把写入授权绑定到当前文件指纹。",
        "Status.Detail.RevisionRequired": "多个历史转换版本同时匹配；明确选择版本并重新运行 Dry Run 前不会写入。",
        "Status.Detail.OptionalDataBlocked": "已启用的 system 或 ExtData 缺少明确路径；补齐前所有写入保持禁用。",
        "Status.Detail.SelectedWorkPending": "核心角色已完成，但已选 system 或 ExtData 仍需各自 Dry Run 与事务写入。",
        "Status.Detail.Authorized": "写入只对当前 SHA-256 与明确目标有效；任何变化都会撤销授权。",
        "Status.Detail.Running": "事务正在执行；界面保持状态可见，完成前不会接受第二个写入。",
        "Status.Detail.Succeeded": "事务已完成；请在本次报告中核对备份与 manifest。",
        "Status.Detail.Failed": "授权已撤销。查看错误详情并重新检查；不会静默覆盖目标。",
        "Error.Detail": "完整错误详情",
        "Input.SHA256": "SHA-256",
        "Input.Profile": "存档类型",
        "Input.Bytes": "字节数",
        "Input.Subtitle": "每一种概念使用独立控件：3DS 源、修复时的当前 Wii U 引用，以及最终输出。目录只解析直接同名 user#。",
        "Input.SourceMessage": "请选择一个 3DS user1、user2 或 user3 文件，或包含它们的直接父目录。",
        "Input.CurrentMessage": "请选择之前转换过且继续游玩后的同槽位 Wii U/Cemu user#，或直接包含它的目录。该输入不会被写入。",
        "Input.TargetMessage": "请选择新转换结果的输出 user# 文件或已有输出目录。",
        "Input.RepairOutputMessage": "请选择修复结果的独立输出控件；若明确选择与当前引用相同的 user#，则执行原地修复。已有输出会先备份；若修复公会名片，目录必须已初始化并包含 card1/card2/card3/cardbox。",
        "Input.InvalidSlot": "请选择 user1、user2 或 user3 文件，或一个直接包含所选槽位的目录。",
        "Guide.InputComplete": "检查已完成。下一步可按需添加名片、任务或其他可选数据；也可以直接继续 Dry Run。",
        "Guide.ComponentsReady": "可选数据已确认。接下来运行只读 Dry Run，确认本次转换会写入什么。",
        "Guide.OptionalDataNeedsConfiguration": "已启用的可选数据尚未配齐。请先在本页选择所需的 system 或 ExtData 目录，再继续 Dry Run。",
        "Guide.OptionalDataReadyForTransaction": "已选可选数据已配齐。核心角色无需重复转换；前往写入页分别进行可选组件的 Dry Run 和写入。",
        "Guide.DryRunComplete": "Dry Run 已通过。确认写入前不会修改任何存档。",
        "Guide.CoreDryRunCompleteWithOptionals": "核心角色 Dry Run 已通过。接下来进入写入页；已选 system 或 ExtData 仍须分别 Dry Run 和写入。",
        "Guide.SelectedWorkPending": "本次已选数据尚未全部完成。请在本页完成每项的 Dry Run 和写入；完成前不会显示最终报告入口。",
        "Guide.WriteComplete": "转换已完成。可在本次报告中查看写入结果、备份与 manifest。",
        "Guide.ToComponents": "继续：配置可选数据",
        "Guide.ToDryRun": "继续：运行 Dry Run",
        "Guide.ToWrite": "继续：确认写入",
        "Guide.ToWriteAndOptionals": "继续：处理写入与可选数据",
        "Guide.ToHistory": "查看本次报告",
        "Guide.NextStep": "下一步",
        "Components.Subtitle": "可选数据不是必需的。只转换角色时可直接继续 Dry Run；选择名片或任务后，再提供对应的 ExtData 目录。",
        "Components.GuildCardsDetail": "迁移收到的公会名片和离线集会所伙伴。",
        "Components.QuestsDetail": "迁移任务数据。",
        "Components.SystemSource": "3DS system",
        "Components.SystemTarget": "Cemu system",
        "Components.SystemWarning": "system 由三个角色槽位共享。必须同时提供 3DS 源文件和已初始化的 Cemu 目标；转换器只合并管家画廊/动画标记，其他 Cemu 设置与跨槽位数据保持不变。",
        "Components.SystemFooter": "目标必须是已存在且格式有效的 Cemu system。若不提供这一对文件，则不会迁移管家画廊/动画记录。",
        "Components.SystemSourceMessage": "请选择一个 3DS system 文件。",
        "Components.SystemTargetMessage": "请选择一个 Cemu system 文件。",
        "Components.ExtrasFooter": "选择 00000481 或其直接 user 子目录即可。应用只处理已选的名片/任务组件，不会扫描 SD 卡或覆盖整个 Cemu 存档目录。",
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
        "DryRun.NewExport": "新输出文件（写入前必须保持不存在）",
        "Write.Subtitle": "核心角色写入由事务保护。替换所选 Cemu 目标前会先创建备份和 manifest。",
        "Write.Authorization": "写入授权",
        "Write.CurrentAuthorized": "当前 Dry Run 有效",
        "Write.OptionalDeferred": "可选组需要各自的组级 Dry Run，暂不包含在此核心角色事务中。",
        "Write.Footer": "确认页只列出一个目标文件。它不会复制 MLC，也不会递归替换目录。",
        "Write.Manifest": "Manifest",
        "Write.SelectManifest": "选择事务 manifest",
        "Write.SelectManifestMessage": "请选择此前转换或兼容修复创建的准确 manifest（.mh3g-install.json 或 .mh3g-compatibility-repair-*.json）。",
        "Write.RollbackFooter": "回滚绑定到所选 manifest，不能指向普通目标目录。",
        "Write.Files": "文件",
        "Write.OneTarget": "1 个选定目标",
        "Write.RepairTargets": "核心输出 + 公会名片完整组件组",
        "Write.Target": "目标",
        "Write.SourceSHA256": "源文件 SHA-256",
        "Write.TargetSHA256": "目标文件 SHA-256",
        "Write.CurrentSetSHA256": "当前 Wii U 引用集合 SHA-256",
        "Write.OutputSetSHA256": "输出状态集合 SHA-256",
        "Write.NewExport": "新输出文件（写入前必须保持不存在）",
        "Write.StagingSetSHA256": "暂存集合 SHA-256",
        "Write.TargetSetSHA256": "目标集合 SHA-256",
        "Write.Backup": "备份",
        "Write.OneBackup": "替换前创建 1 份事务备份",
        "Write.ManifestCreated": "在所选目标旁创建",
        "Write.ExperimentalCEC": "实验性 CEC",
        "Write.NotIncluded": "不包含",
        "Write.Cancel": "取消",
        "Write.SelectedSave": "写入所选存档",
        "Write.Core": "核心角色存档",
        "Write.System": "共享 system",
        "Write.SystemDryRun": "运行 system Dry Run",
        "Write.WriteSystem": "写入 system",
        "Write.SystemFooter": "system 使用独立的成对输入和 SHA-256 授权；仅合并画廊/动画标记并保留其他 Cemu 共享字节。",
        "Write.Extras": "可选 ExtData",
        "Write.ExtrasStageDryRun": "预览临时转换",
        "Write.ExtrasStage": "写入临时转换",
        "Write.ExtrasInstallDryRun": "预览 ExtData 安装",
        "Write.ExtrasInstall": "安装所选 ExtData",
        "Write.ExtrasManifest": "ExtData manifest",
        "Write.ExtrasFooter": "仅安装已选的完整组；暂存与 Cemu 目标会分别绑定 Dry Run。",
        "Write.TargetDirectory": "目标目录",
        "Write.StageReady": "临时转换已授权",
        "Write.InstallReady": "ExtData 安装已授权",
        "CEC.Source": "3DS CEC 邮箱",
        "CEC.Target": "Cemu CEC 缓存",
        "CEC.SourceRecords": "源记录",
        "CEC.SourceRecordSetSHA256": "源记录集合 SHA-256",
        "CEC.TargetSHA256": "目标 CEC SHA-256",
        "CEC.Warning": "CEC 与 card1/card2/card3/cardbox 相互独立。选择名片组绝不会自动启用 CEC。",
        "CEC.Disclosure": "导入 CEC 邮箱",
        "CEC.Footer": "CEC 需要独立 Dry Run；写入前会再次读取邮箱与目标缓存。",
        "CEC.SourceMessage": "请选择准确的 .../CEC/00048100。应用不会搜索 SD 卡或 MLC。",
        "CEC.TargetMessage": "请选择准确的 Cemu MH3G cec 文件。",
        "CEC.DryRun": "运行 CEC Dry Run",
        "CEC.Authorized": "CEC Dry Run 已授权当前邮箱与缓存",
        "CEC.Write": "写入 CEC 缓存",
        "CEC.ConfirmTitle": "写入实验性 CEC",
        "CEC.ConfirmMessage": "仅替换所选 Cemu cec 缓存，并在其旁创建备份与 manifest。",
        "CEC.Manifest": "CEC manifest",
        "CEC.Rollback": "回滚 CEC",
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
        "WorkflowState.Input": "等待选择存档",
        "WorkflowState.ComponentSelection": "已完成检查",
        "WorkflowState.DryRun": "Dry Run 已完成",
        "WorkflowState.Writing": "正在执行",
        "WorkflowState.Success": "操作完成",
        "WorkflowState.Failure": "操作失败",
        "Settings.BundledSidecar": "打包时使用内置 sidecar",
        "Settings.Development": "开发版本",
        "Update.About": "关于与更新",
        "Update.CurrentVersion": "当前版本",
        "Update.Check": "检查更新",
        "Update.Checking": "正在连接 GitHub…",
        "Update.UpToDate": "已经是最新版（GitHub：%@）",
        "Update.Available": "发现新版本 %@，请在弹窗中查看更新说明。",
        "Update.Failed": "无法检查更新",
        "Update.NetworkNote": "每天首次启动会静默检查一次 GitHub Release；网络不可用时不会阻塞或影响任何本地存档操作。也可以随时手动重试。",
        "Update.AvailableTitle": "发现新版本",
        "Update.VersionSummary": "当前 %@ · 最新 %@",
        "Update.ReleaseNotes": "更新说明",
        "Update.NoReleaseNotes": "该版本未提供更新说明。",
        "Update.Later": "稍后",
        "Update.OpenRelease": "打开发布页面",
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
        "Input.Current": "Current Wii U / Cemu reference save",
        "Input.CurrentReadOnly": "This save supplies continued-play data only. Repair never writes to it.",
        "Input.Target": "Wii U / Cemu output",
        "Input.RepairOutput": "Repaired Wii U / Cemu output",
        "Input.Required": "Required",
        "Input.Mode": "Operation",
        "Input.Mode.New": "New conversion",
        "Input.Mode.Repair": "Repair converted save",
        "Input.RepairHint": "Choose three separate paths: the original 3DS save, the current Wii U/Cemu save after continued play, and an independent output. The current Wii U input is always read-only.",
        "Repair.Version": "Original converter version",
        "Repair.Version.Auto": "Detect automatically",
        "Repair.Version.Hint": "Use automatic detection if the version is unknown. If several historical versions match, Dry Run will require an explicit selection before it can authorize a write.",
        "Repair.Version.Required": "Several versions match this save. Select the converter version originally used, then run Dry Run again.",
        "Repair.PreviewSHA256": "Repair preview SHA-256",
        "Components.RepairExtData": "Optional: add the original 3DS ExtData to repair received guild cards. Current card*/quest* files are read from the Wii U save directory.",
        "Input.Slot": "Save slot",
        "Input.FinalOutput": "Final output",
        "Input.NewOutput": "New output file; it must remain absent before writing",
        "Input.Select": "Choose…",
        "Input.SelectFile": "Choose File…",
        "Input.SelectDirectory": "Choose Folder…",
        "Input.SelectFileOrDirectory": "Choose File or Folder…",
        "Input.SourceSelectionHint": "Allowed: one user# file, or the direct parent folder containing the selected user#.",
        "Input.CurrentSelectionHint": "Allowed: one existing same-slot Wii U/Cemu user# file, or its direct parent folder.",
        "Input.OutputSelectionHint": "Allowed: a same-name user# file, or an existing output folder; a folder resolves to <folder>/<selected user#>.",
        "Input.PathGuideTitle": "File or direct parent folder supported (show example)",
        "Input.PathGuideSummary": "Only the direct same-name user# is resolved. The app never recursively scans an SD card or MLC and never reads ZIP, 7z, or RAR archives.",
        "Input.PathGuideExample": "Example (user2 selected)\n\n3DS save parent/\n├── system        (not read by this control)\n└── user2         ← 3DS source\n\nCurrent Cemu save folder/\n├── user2         ← current reference (repair only, read-only)\n├── card1, card2, card3, cardbox   (read for guild-card repair)\n└── quest1 … quest4               (validated and preserved for repair)\n\nOutput folder/\n├── user2         ← result (may be absent for a core-only operation)\n└── card1, card2, card3, cardbox   (must exist for separate guild-card output)",
        "Input.Inspect": "Inspect selection",
        "Input.NoSelection": "No file selected",
        "Components.System": "Shared system",
        "Components.GuildCards": "Guild cards & offline partners",
        "Components.Quests": "Quests",
        "Components.ExtDataSource": "3DS ExtData user directory",
        "Components.Staging": "Conversion staging directory",
        "Components.Target": "Cemu ExtData directory",
        "DryRun.Start": "Run Dry Run",
        "DryRun.NotAuthorized": "Inspect every input and output, then run Dry Run. Repair authorization independently binds the 3DS source, current Wii U reference, output state, and preview.",
        "Write.Confirm": "Confirm write",
        "Write.Unavailable": "A current successful Dry Run is required before writing.",
        "Write.Rollback": "Roll back from manifest",
        "CEC.Acknowledge": "I understand that CEC import is experimental",
        "CEC.Hidden": "Experimental CEC is off by default. Guild-card and offline-partner migration does not depend on it.",
        "Settings.Language": "Language",
        "Settings.Language.System": "System default",
        "Settings.Language.Chinese": "Simplified Chinese",
        "Settings.Language.English": "English",
        "Status.NotReady": "Not ready",
        "Status.NeedsInput": "Choose saves to begin",
        "Status.NeedsInspection": "Read-only inspection required",
        "Status.ReadyForDryRun": "Ready for Dry Run",
        "Status.RevisionRequired": "Original converter version required",
        "Status.OptionalDataBlocked": "Optional data is incomplete",
        "Status.SelectedWorkPending": "Selected data is not complete",
        "Status.Blocked": "Write blocked",
        "Status.Authorized": "Dry Run authorized",
        "Status.Ready": "Ready",
        "Status.Running": "Working…",
        "Status.Succeeded": "Operation complete",
        "Status.Failed": "Operation failed",
        "Status.Detail.NeedsInput": "New conversion requires a 3DS source and output. Repair additionally requires a separately selected current Wii U/Cemu reference save. The app never scans directories automatically.",
        "Status.Detail.NeedsInspection": "Run the read-only inspection to verify profile, size, and SHA-256. No file is written.",
        "Status.Detail.ReadyForDryRun": "Inspection passed. Dry Run will bind write authorization to the current file fingerprints.",
        "Status.Detail.RevisionRequired": "Several historical revisions match. Choose one and repeat Dry Run before any write is allowed.",
        "Status.Detail.OptionalDataBlocked": "A selected system or ExtData domain lacks an explicit path. All writes remain disabled until it is complete.",
        "Status.Detail.SelectedWorkPending": "The core slot completed, but selected system or ExtData still needs its own Dry Run and transaction.",
        "Status.Detail.Authorized": "Authorization applies only to the current SHA-256 values and explicit target. Any change revokes it.",
        "Status.Detail.Running": "The transaction is running. Its state remains visible and a second write is refused until completion.",
        "Status.Detail.Succeeded": "The transaction completed. Review its backup and manifest in this session's report.",
        "Status.Detail.Failed": "Authorization was revoked. Review the error and inspect again; the target is never silently replaced.",
        "Error.Detail": "Full error details",
        "Input.SHA256": "SHA-256",
        "Input.Profile": "Profile",
        "Input.Bytes": "Bytes",
        "Input.Subtitle": "Each concept has its own control: 3DS source, current Wii U reference for repair, and final output. A folder resolves only its direct matching user#.",
        "Input.SourceMessage": "Choose a 3DS user1, user2, or user3 file, or its direct parent directory.",
        "Input.CurrentMessage": "Choose the same-slot Wii U/Cemu user# that was converted and then played, or its direct parent folder. This input is never written.",
        "Input.TargetMessage": "Choose the output user# file or an existing output directory for a new conversion.",
        "Input.RepairOutputMessage": "Choose the repaired output independently. Explicitly selecting the same user# as the current reference performs an in-place repair. Existing output is backed up; guild-card repair requires an initialized directory containing card1/card2/card3/cardbox.",
        "Input.InvalidSlot": "Choose user1, user2, or user3, or a directory that directly contains the selected slot.",
        "Guide.InputComplete": "Inspection is complete. Add guild cards, quests, or other optional data if needed, or continue directly to Dry Run.",
        "Guide.ComponentsReady": "Optional data is set. Next, run a read-only Dry Run to review what this conversion will write.",
        "Guide.OptionalDataNeedsConfiguration": "Selected optional data is not fully configured. Choose the required system or ExtData directories here before continuing to Dry Run.",
        "Guide.OptionalDataReadyForTransaction": "Selected optional data is configured. The core slot does not need another conversion; continue to the write page for each optional component's Dry Run and write.",
        "Guide.DryRunComplete": "Dry Run passed. No save has been modified; continue when you are ready to confirm the write.",
        "Guide.CoreDryRunCompleteWithOptionals": "The core-slot Dry Run passed. Continue to the write page; selected system or ExtData data still needs its own Dry Run and write.",
        "Guide.SelectedWorkPending": "Selected data is not all complete. Finish each Dry Run and write on this page before the final report becomes available.",
        "Guide.WriteComplete": "Conversion is complete. Review this session's result, backup, and manifest.",
        "Guide.ToComponents": "Continue: configure optional data",
        "Guide.ToDryRun": "Continue: run Dry Run",
        "Guide.ToWrite": "Continue: confirm write",
        "Guide.ToWriteAndOptionals": "Continue: handle writes and optional data",
        "Guide.ToHistory": "View this session's report",
        "Guide.NextStep": "Next step",
        "Components.Subtitle": "Optional data is not required. Continue directly to Dry Run for a character-only conversion, or add the matching ExtData directory after choosing cards or quests.",
        "Components.GuildCardsDetail": "Migrate received guild cards and offline-hall partners.",
        "Components.QuestsDetail": "Migrate quest data.",
        "Components.SystemSource": "3DS system",
        "Components.SystemTarget": "Cemu system",
        "Components.SystemWarning": "System is shared by all three character slots. Supply both the 3DS source and an initialized Cemu target; only housekeeper gallery/movie flags are merged while every other Cemu setting and shared-slot byte is retained.",
        "Components.SystemFooter": "The target must be an existing valid Cemu system. If this pair is omitted, housekeeper gallery/movie history is not migrated.",
        "Components.SystemSourceMessage": "Choose exactly one 3DS system file.",
        "Components.SystemTargetMessage": "Choose exactly one Cemu system file.",
        "Components.ExtrasFooter": "Choose 00000481 or its direct user child. The app only handles selected card/quest components; it does not scan an SD card or replace an entire Cemu save directory.",
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
        "DryRun.NewExport": "New export (must remain absent before write)",
        "Write.Subtitle": "The core slot write is transaction-backed. It creates a backup and manifest before replacing the chosen Cemu target.",
        "Write.Authorization": "Write authorization",
        "Write.CurrentAuthorized": "Current Dry Run is valid",
        "Write.OptionalDeferred": "Optional groups require their own group Dry Run and are not included in this primary-slot transaction yet.",
        "Write.Footer": "The confirmation lists exactly one target file. It never copies an MLC or recursively replaces a directory.",
        "Write.Manifest": "Manifest",
        "Write.SelectManifest": "Select transaction manifest",
        "Write.SelectManifestMessage": "Choose the exact manifest created by a prior conversion or compatibility repair (.mh3g-install.json or .mh3g-compatibility-repair-*.json).",
        "Write.RollbackFooter": "Rollback is bound to the selected manifest. It cannot be pointed at a generic target directory.",
        "Write.Files": "Files",
        "Write.OneTarget": "1 selected target",
        "Write.RepairTargets": "Core output + complete guild-card component group",
        "Write.Target": "Target",
        "Write.SourceSHA256": "Source SHA-256",
        "Write.TargetSHA256": "Target SHA-256",
        "Write.CurrentSetSHA256": "Current Wii U reference set SHA-256",
        "Write.OutputSetSHA256": "Output-state set SHA-256",
        "Write.NewExport": "New export (must remain absent before write)",
        "Write.StagingSetSHA256": "Staging set SHA-256",
        "Write.TargetSetSHA256": "Target set SHA-256",
        "Write.Backup": "Backup",
        "Write.OneBackup": "1 transaction backup before replacement",
        "Write.ManifestCreated": "Created beside the selected target",
        "Write.ExperimentalCEC": "Experimental CEC",
        "Write.NotIncluded": "Not included",
        "Write.Cancel": "Cancel",
        "Write.SelectedSave": "Write selected save",
        "Write.Core": "Core character slot",
        "Write.System": "Shared system",
        "Write.SystemDryRun": "Run system Dry Run",
        "Write.WriteSystem": "Write system",
        "Write.SystemFooter": "System uses a separate paired-input SHA-256 authorization; it merges only gallery/movie flags and retains all other Cemu shared bytes.",
        "Write.Extras": "Optional ExtData",
        "Write.ExtrasStageDryRun": "Preview staging conversion",
        "Write.ExtrasStage": "Write staging conversion",
        "Write.ExtrasInstallDryRun": "Preview ExtData install",
        "Write.ExtrasInstall": "Install selected ExtData",
        "Write.ExtrasManifest": "ExtData manifest",
        "Write.ExtrasFooter": "Only selected complete groups are installed; staging and the Cemu target each bind to their own Dry Run.",
        "Write.TargetDirectory": "Target directory",
        "Write.StageReady": "Staging conversion authorized",
        "Write.InstallReady": "ExtData install authorized",
        "CEC.Source": "3DS CEC mailbox",
        "CEC.Target": "Cemu CEC cache",
        "CEC.SourceRecords": "Source records",
        "CEC.SourceRecordSetSHA256": "Source record set SHA-256",
        "CEC.TargetSHA256": "Target CEC SHA-256",
        "CEC.Warning": "CEC remains separate from card1/card2/card3/cardbox. It is never enabled by choosing a guild-card group.",
        "CEC.Disclosure": "CEC mailbox import",
        "CEC.Footer": "CEC requires its own Dry Run; the mailbox and target cache are read again before writing.",
        "CEC.SourceMessage": "Choose exactly .../CEC/00048100. The app does not search an SD card or MLC.",
        "CEC.TargetMessage": "Choose exactly the Cemu MH3G cec file.",
        "CEC.DryRun": "Run CEC Dry Run",
        "CEC.Authorized": "CEC Dry Run authorizes this mailbox and cache",
        "CEC.Write": "Write CEC cache",
        "CEC.ConfirmTitle": "Write experimental CEC",
        "CEC.ConfirmMessage": "Only the selected Cemu cec cache is replaced. A backup and manifest are created beside it.",
        "CEC.Manifest": "CEC manifest",
        "CEC.Rollback": "Roll back CEC",
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
        "WorkflowState.Input": "Awaiting save selection",
        "WorkflowState.ComponentSelection": "Inspection complete",
        "WorkflowState.DryRun": "Dry Run complete",
        "WorkflowState.Writing": "Operation in progress",
        "WorkflowState.Success": "Operation complete",
        "WorkflowState.Failure": "Operation failed",
        "Settings.BundledSidecar": "Bundled sidecar when packaged",
        "Settings.Development": "development",
        "Update.About": "About & Updates",
        "Update.CurrentVersion": "Current version",
        "Update.Check": "Check for Updates",
        "Update.Checking": "Contacting GitHub…",
        "Update.UpToDate": "You are up to date (GitHub: %@)",
        "Update.Available": "Version %@ is available. Review its release notes in the update window.",
        "Update.Failed": "Unable to check for updates",
        "Update.NetworkNote": "The first launch each day silently checks GitHub Releases once. An unavailable network never blocks or changes any local save operation, and you can retry manually at any time.",
        "Update.AvailableTitle": "Update Available",
        "Update.VersionSummary": "Current %@ · Latest %@",
        "Update.ReleaseNotes": "Release notes",
        "Update.NoReleaseNotes": "No release notes were provided for this version.",
        "Update.Later": "Later",
        "Update.OpenRelease": "Open Release Page",
    ]

    /// The complete set of copy IDs rendered by the SwiftUI workbench.  Tests
    /// assert that each ID has a non-fallback value in both supported languages.
    public static var visibleKeys: [String] { zhHans.keys.sorted() }

    public static func text(_ key: String, language: ConverterLanguage) -> String {
        let table = language == .zhHans ? zhHans : english
        return table[key] ?? key
    }
}
