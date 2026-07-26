using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Infrastructure;

/// <summary>
/// Runtime-selectable copy for the small native shell. English is the fallback
/// for all keys; switching the persisted preference updates every binding.
/// </summary>
public sealed class ConverterCopy : ObservableObject
{
    private AppLanguageOverride _language;

    public ConverterCopy(AppLanguageOverride language)
    {
        _language = language;
    }

    public void SetLanguage(AppLanguageOverride language)
    {
        if (_language == language)
        {
            return;
        }

        _language = language;
        OnAllPropertiesChanged();
    }

    public string AppTitle => Text(nameof(AppTitle));
    public string AppSubtitle => Text(nameof(AppSubtitle));
    public string Language => Text(nameof(Language));
    public string LanguageSystem => Text(nameof(LanguageSystem));
    public string LanguageChinese => Text(nameof(LanguageChinese));
    public string LanguageEnglish => Text(nameof(LanguageEnglish));
    public string StageInput => Text(nameof(StageInput));
    public string StageInspect => Text(nameof(StageInspect));
    public string StageDryRun => Text(nameof(StageDryRun));
    public string StageWrite => Text(nameof(StageWrite));
    public string HeroTitle => Text(nameof(HeroTitle));
    public string HeroDescription => Text(nameof(HeroDescription));
    public string HeroSafety => Text(nameof(HeroSafety));
    public string ArtworkSource => Text(nameof(ArtworkSource));
    public string ArtworkTarget => Text(nameof(ArtworkTarget));
    public string CoreSectionTitle => Text(nameof(CoreSectionTitle));
    public string CoreSectionDescription => Text(nameof(CoreSectionDescription));
    public string SourceSlot => Text(nameof(SourceSlot));
    public string SourceSlotHint => Text(nameof(SourceSlotHint));
    public string TargetSlot => Text(nameof(TargetSlot));
    public string TargetSlotHint => Text(nameof(TargetSlotHint));
    public string CliPath => Text(nameof(CliPath));
    public string CliPathHint => Text(nameof(CliPathHint));
    public string Browse => Text(nameof(Browse));
    public string Inspect => Text(nameof(Inspect));
    public string InspectProgress => Text(nameof(InspectProgress));
    public string InspectEvents => Text(nameof(InspectEvents));
    public string DryRunTitle => Text(nameof(DryRunTitle));
    public string DryRunDescription => Text(nameof(DryRunDescription));
    public string RunDryRun => Text(nameof(RunDryRun));
    public string DryRunGate => Text(nameof(DryRunGate));
    public string WriteTitle => Text(nameof(WriteTitle));
    public string WriteDescription => Text(nameof(WriteDescription));
    public string WriteSelectedSave => Text(nameof(WriteSelectedSave));
    public string WriteUnavailable => Text(nameof(WriteUnavailable));
    public string TransactionDetails => Text(nameof(TransactionDetails));
    public string BackupAndManifest => Text(nameof(BackupAndManifest));
    public string OneTarget => Text(nameof(OneTarget));
    public string RollbackTitle => Text(nameof(RollbackTitle));
    public string RollbackDescription => Text(nameof(RollbackDescription));
    public string ManifestPath => Text(nameof(ManifestPath));
    public string RollbackFromManifest => Text(nameof(RollbackFromManifest));
    public string OptionalSectionTitle => Text(nameof(OptionalSectionTitle));
    public string OptionalSectionDescription => Text(nameof(OptionalSectionDescription));
    public string ExperimentalCec => Text(nameof(ExperimentalCec));
    public string ExperimentalCecHint => Text(nameof(ExperimentalCecHint));
    public string CecSourceDirectory => Text(nameof(CecSourceDirectory));
    public string CecTargetCache => Text(nameof(CecTargetCache));
    public string InspectCec => Text(nameof(InspectCec));
    public string RunCecDryRun => Text(nameof(RunCecDryRun));
    public string CecAcknowledge => Text(nameof(CecAcknowledge));
    public string WriteCec => Text(nameof(WriteCec));
    public string RollbackCec => Text(nameof(RollbackCec));
    public string ExtDataNotice => Text(nameof(ExtDataNotice));
    public string ResultTitle => Text(nameof(ResultTitle));
    public string ResultEmpty => Text(nameof(ResultEmpty));
    public string OperationHistory => Text(nameof(OperationHistory));
    public string Ready => Text(nameof(Ready));
    public string Working => Text(nameof(Working));
    public string Failed => Text(nameof(Failed));
    public string Inspected => Text(nameof(Inspected));
    public string DryRunAuthorized => Text(nameof(DryRunAuthorized));
    public string Written => Text(nameof(Written));
    public string RolledBack => Text(nameof(RolledBack));
    public string ConfirmWriteTitle => Text(nameof(ConfirmWriteTitle));
    public string ConfirmWriteBody => Text(nameof(ConfirmWriteBody));
    public string ConfirmRollbackTitle => Text(nameof(ConfirmRollbackTitle));
    public string ConfirmRollbackBody => Text(nameof(ConfirmRollbackBody));
    public string ConfirmCecTitle => Text(nameof(ConfirmCecTitle));
    public string ConfirmCecBody => Text(nameof(ConfirmCecBody));
    public string Continue => Text(nameof(Continue));
    public string Cancel => Text(nameof(Cancel));
    public string OpenFolder => Text(nameof(OpenFolder));
    public string ExpectedCliMissing => Text(nameof(ExpectedCliMissing));
    public string InvalidCorePaths => Text(nameof(InvalidCorePaths));
    public string FileChangedAfterDryRun => Text(nameof(FileChangedAfterDryRun));
    public string CecDisabled => Text(nameof(CecDisabled));
    public string CecAcknowledgementRequired => Text(nameof(CecAcknowledgementRequired));
    public string CecPathsRequired => Text(nameof(CecPathsRequired));
    public string CecFingerprintMissing => Text(nameof(CecFingerprintMissing));

    private string Text(string key)
    {
        var table = _language == AppLanguageOverride.Chinese ? Chinese : English;
        return table.TryGetValue(key, out var value) ? value : English[key];
    }

    private static readonly IReadOnlyDictionary<string, string> English = new Dictionary<string, string>
    {
        [nameof(AppTitle)] = "MH3G Save Converter",
        [nameof(AppSubtitle)] = "Japanese MH3G 3DS to Wii U / Cemu",
        [nameof(Language)] = "Language",
        [nameof(LanguageSystem)] = "System default",
        [nameof(LanguageChinese)] = "Simplified Chinese",
        [nameof(LanguageEnglish)] = "English",
        [nameof(StageInput)] = "Input",
        [nameof(StageInspect)] = "Inspect",
        [nameof(StageDryRun)] = "Dry Run",
        [nameof(StageWrite)] = "Write / Rollback",
        [nameof(HeroTitle)] = "A local, transaction-backed migration",
        [nameof(HeroDescription)] = "Select exact files. Inspect first, preview with a read-only Dry Run, then authorize one matching target file.",
        [nameof(HeroSafety)] = "No archive discovery. No cloud upload. No recursive MLC overwrite.",
        [nameof(ArtworkSource)] = "Exact source file",
        [nameof(ArtworkTarget)] = "One matching target",
        [nameof(CoreSectionTitle)] = "Core slot",
        [nameof(CoreSectionDescription)] = "One 3DS user1, user2, or user3 file maps only to the identically named Cemu target.",
        [nameof(SourceSlot)] = "3DS source character file",
        [nameof(SourceSlotHint)] = "Choose exactly user1, user2, or user3. Do not choose a folder or archive.",
        [nameof(TargetSlot)] = "Cemu target character file",
        [nameof(TargetSlotHint)] = "Choose the matching initialized Cemu user# file. It is never inferred from an MLC root.",
        [nameof(CliPath)] = "Converter CLI",
        [nameof(CliPathHint)] = "Packaging places mh3g-save-convert.exe beside the app. You may choose an explicit signed sidecar for development.",
        [nameof(Browse)] = "Browse",
        [nameof(Inspect)] = "Inspect files",
        [nameof(InspectProgress)] = "Inspect progress",
        [nameof(InspectEvents)] = "Inspect events",
        [nameof(DryRunTitle)] = "Preview before any write",
        [nameof(DryRunDescription)] = "Dry Run calls the Rust converter with --dry-run and writes no target, backup, manifest, or CEC data.",
        [nameof(RunDryRun)] = "Run Dry Run",
        [nameof(DryRunGate)] = "A successful Dry Run binds this screen to the current source and target SHA-256 values.",
        [nameof(WriteTitle)] = "Write one authorized target",
        [nameof(WriteDescription)] = "Before launching --write, the app rechecks both selected files. The Rust transaction creates its own backup and manifest.",
        [nameof(WriteSelectedSave)] = "Write selected save",
        [nameof(WriteUnavailable)] = "Inspect both paths and complete a current Dry Run before writing.",
        [nameof(TransactionDetails)] = "Transaction scope",
        [nameof(BackupAndManifest)] = "Backup and manifest are created beside this one target only.",
        [nameof(OneTarget)] = "1 selected target",
        [nameof(RollbackTitle)] = "Rollback a known transaction",
        [nameof(RollbackDescription)] = "Rollback accepts only the exact manifest created by the converter. It never treats a directory as a rollback request.",
        [nameof(ManifestPath)] = "Transaction manifest",
        [nameof(RollbackFromManifest)] = "Rollback from manifest",
        [nameof(OptionalSectionTitle)] = "Optional data",
        [nameof(OptionalSectionDescription)] = "Shared system and ExtData remain separate CLI transactions. CEC is isolated and disabled until you explicitly enable it.",
        [nameof(ExperimentalCec)] = "Experimental CEC",
        [nameof(ExperimentalCecHint)] = "CEC is a separate StreetPass / Hunter Search cache. Guild-card and offline-partner migration do not require it.",
        [nameof(CecSourceDirectory)] = "3DS CEC mailbox directory",
        [nameof(CecTargetCache)] = "Cemu cec cache",
        [nameof(InspectCec)] = "Inspect CEC",
        [nameof(RunCecDryRun)] = "Run CEC Dry Run",
        [nameof(CecAcknowledge)] = "I understand that CEC import remains experimental",
        [nameof(WriteCec)] = "Write experimental CEC",
        [nameof(RollbackCec)] = "Rollback CEC manifest",
        [nameof(ExtDataNotice)] = "Guild cards, quests, and shared system have their own complete transaction groups. This first Windows shell leaves those CLI routes explicit rather than guessing an MLC directory.",
        [nameof(ResultTitle)] = "Structured report",
        [nameof(ResultEmpty)] = "Run an inspection or Dry Run to see the CLI JSON report here.",
        [nameof(OperationHistory)] = "This session",
        [nameof(Ready)] = "Ready",
        [nameof(Working)] = "Working",
        [nameof(Failed)] = "Operation failed",
        [nameof(Inspected)] = "Inputs inspected",
        [nameof(DryRunAuthorized)] = "Dry Run authorized",
        [nameof(Written)] = "Written with transaction evidence",
        [nameof(RolledBack)] = "Rolled back from manifest",
        [nameof(ConfirmWriteTitle)] = "Write the selected Cemu save?",
        [nameof(ConfirmWriteBody)] = "This replaces exactly one selected target after a final SHA-256 check. The Rust CLI will create its transaction backup and manifest.",
        [nameof(ConfirmRollbackTitle)] = "Restore from this manifest?",
        [nameof(ConfirmRollbackBody)] = "Rollback restores only the target bound to this converter-created manifest.",
        [nameof(ConfirmCecTitle)] = "Write experimental CEC data?",
        [nameof(ConfirmCecBody)] = "This writes only the selected Cemu cec cache after a separate CEC Dry Run. It does not alter user#, card#, or quest# files.",
        [nameof(Continue)] = "Continue",
        [nameof(Cancel)] = "Cancel",
        [nameof(OpenFolder)] = "Open folder",
        [nameof(ExpectedCliMissing)] = "The bundled converter CLI is missing. Choose the signed mh3g-save-convert.exe sidecar before running a command.",
        [nameof(InvalidCorePaths)] = "Choose matching user1, user2, or user3 source and target files.",
        [nameof(FileChangedAfterDryRun)] = "The source or target changed after Dry Run. Inspect and run Dry Run again before writing.",
        [nameof(CecDisabled)] = "Enable Experimental CEC to use its isolated commands.",
        [nameof(CecAcknowledgementRequired)] = "Acknowledge the experimental CEC warning before its write command is available.",
        [nameof(CecPathsRequired)] = "Choose the exact CEC mailbox directory and Cemu cec target path.",
        [nameof(CecFingerprintMissing)] = "CEC Dry Run did not provide the expected JSON fingerprints.",
    };

    private static readonly IReadOnlyDictionary<string, string> Chinese = new Dictionary<string, string>
    {
        [nameof(AppTitle)] = "MH3G 存档转换器",
        [nameof(AppSubtitle)] = "日版 MH3G 3DS 到 Wii U / Cemu",
        [nameof(Language)] = "语言",
        [nameof(LanguageSystem)] = "跟随系统",
        [nameof(LanguageChinese)] = "简体中文",
        [nameof(LanguageEnglish)] = "English",
        [nameof(StageInput)] = "输入",
        [nameof(StageInspect)] = "检查",
        [nameof(StageDryRun)] = "Dry Run",
        [nameof(StageWrite)] = "写入 / 回滚",
        [nameof(HeroTitle)] = "本地、事务保护的存档迁移",
        [nameof(HeroDescription)] = "只选择准确文件。先检查，再只读 Dry Run，最后只授权一个同名目标文件。",
        [nameof(HeroSafety)] = "不扫描压缩包，不上传云端，不递归覆盖 MLC。",
        [nameof(ArtworkSource)] = "准确的源文件",
        [nameof(ArtworkTarget)] = "一个同名目标",
        [nameof(CoreSectionTitle)] = "核心角色槽位",
        [nameof(CoreSectionDescription)] = "一个 3DS user1、user2 或 user3 只会映射到同名的 Cemu 目标文件。",
        [nameof(SourceSlot)] = "3DS 源角色文件",
        [nameof(SourceSlotHint)] = "只能选择 user1、user2 或 user3，不接受文件夹或压缩包。",
        [nameof(TargetSlot)] = "Cemu 目标角色文件",
        [nameof(TargetSlotHint)] = "选择同名且已初始化的 Cemu user# 文件；应用不会根据 MLC 根目录猜测。",
        [nameof(CliPath)] = "转换器 CLI",
        [nameof(CliPathHint)] = "打包时会把 mh3g-save-convert.exe 放在应用旁。开发时可明确选择已签名的 sidecar。",
        [nameof(Browse)] = "选择",
        [nameof(Inspect)] = "检查文件",
        [nameof(InspectProgress)] = "检查任务进度",
        [nameof(InspectEvents)] = "检查事件标记",
        [nameof(DryRunTitle)] = "任何写入之前先预览",
        [nameof(DryRunDescription)] = "Dry Run 使用 Rust 转换器的 --dry-run，不会写入目标、备份、manifest 或 CEC 数据。",
        [nameof(RunDryRun)] = "运行 Dry Run",
        [nameof(DryRunGate)] = "成功的 Dry Run 会绑定当前源文件和目标文件的 SHA-256。",
        [nameof(WriteTitle)] = "写入一个已授权目标",
        [nameof(WriteDescription)] = "启动 --write 前应用会重新检查两个已选文件。Rust 事务会自行创建备份和 manifest。",
        [nameof(WriteSelectedSave)] = "写入所选存档",
        [nameof(WriteUnavailable)] = "请先检查两个路径并完成当前有效的 Dry Run。",
        [nameof(TransactionDetails)] = "事务范围",
        [nameof(BackupAndManifest)] = "备份和 manifest 只会创建在这个目标文件旁。",
        [nameof(OneTarget)] = "1 个已选目标",
        [nameof(RollbackTitle)] = "回滚已知事务",
        [nameof(RollbackDescription)] = "回滚只接受转换器生成的准确 manifest，不会把文件夹当成回滚请求。",
        [nameof(ManifestPath)] = "事务 manifest",
        [nameof(RollbackFromManifest)] = "从 manifest 回滚",
        [nameof(OptionalSectionTitle)] = "可选数据",
        [nameof(OptionalSectionDescription)] = "共享 system 和 ExtData 是独立的 CLI 事务。CEC 完全隔离，默认关闭。",
        [nameof(ExperimentalCec)] = "实验性 CEC",
        [nameof(ExperimentalCecHint)] = "CEC 是独立的擦身通信 / 猎人搜索缓存。公会名片和离线伙伴迁移不依赖它。",
        [nameof(CecSourceDirectory)] = "3DS CEC 邮箱目录",
        [nameof(CecTargetCache)] = "Cemu cec 缓存",
        [nameof(InspectCec)] = "检查 CEC",
        [nameof(RunCecDryRun)] = "运行 CEC Dry Run",
        [nameof(CecAcknowledge)] = "我了解 CEC 导入仍是实验性功能",
        [nameof(WriteCec)] = "写入实验性 CEC",
        [nameof(RollbackCec)] = "回滚 CEC manifest",
        [nameof(ExtDataNotice)] = "公会名片、任务和共享 system 都有独立完整的事务组。这个首版 Windows 外壳保持它们的 CLI 路径明确，不会猜测 MLC 目录。",
        [nameof(ResultTitle)] = "结构化报告",
        [nameof(ResultEmpty)] = "运行检查或 Dry Run 后，会在这里显示 CLI JSON 报告。",
        [nameof(OperationHistory)] = "本次会话",
        [nameof(Ready)] = "已就绪",
        [nameof(Working)] = "正在执行",
        [nameof(Failed)] = "操作失败",
        [nameof(Inspected)] = "输入已检查",
        [nameof(DryRunAuthorized)] = "Dry Run 已授权",
        [nameof(Written)] = "已写入事务证据",
        [nameof(RolledBack)] = "已从 manifest 回滚",
        [nameof(ConfirmWriteTitle)] = "写入所选 Cemu 存档？",
        [nameof(ConfirmWriteBody)] = "最终 SHA-256 检查后只会替换一个目标。Rust CLI 会创建事务备份和 manifest。",
        [nameof(ConfirmRollbackTitle)] = "从这个 manifest 恢复？",
        [nameof(ConfirmRollbackBody)] = "回滚只会恢复此转换器 manifest 绑定的目标。",
        [nameof(ConfirmCecTitle)] = "写入实验性 CEC 数据？",
        [nameof(ConfirmCecBody)] = "独立 CEC Dry Run 后只会写入所选 Cemu cec 缓存，不会修改 user#、card# 或 quest# 文件。",
        [nameof(Continue)] = "继续",
        [nameof(Cancel)] = "取消",
        [nameof(OpenFolder)] = "打开文件夹",
        [nameof(ExpectedCliMissing)] = "未找到打包的转换器 CLI。运行命令前请选择已签名的 mh3g-save-convert.exe sidecar。",
        [nameof(InvalidCorePaths)] = "请选择同名的 user1、user2 或 user3 源文件和目标文件。",
        [nameof(FileChangedAfterDryRun)] = "Dry Run 后源文件或目标文件发生变化。写入前请重新检查并运行 Dry Run。",
        [nameof(CecDisabled)] = "启用实验性 CEC 后才能使用它的独立命令。",
        [nameof(CecAcknowledgementRequired)] = "使用 CEC 写入命令前请确认实验性警告。",
        [nameof(CecPathsRequired)] = "请选择准确的 CEC 邮箱目录和 Cemu cec 目标路径。",
        [nameof(CecFingerprintMissing)] = "CEC Dry Run 没有提供预期的 JSON 指纹。",
    };
}
