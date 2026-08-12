using System.Collections.ObjectModel;
using System.Text.Json;
using Microsoft.UI.Xaml;
using MHToolkit.MH3GSaveConverter.Windows.Infrastructure;
using MHToolkit.MH3GSaveConverter.Windows.Models;
using MHToolkit.MH3GSaveConverter.Windows.Services;

namespace MHToolkit.MH3GSaveConverter.Windows.ViewModels;

/// <summary>
/// Presentation state for the native workbench. The Rust sidecar owns every
/// conversion and transaction; this class only preserves an explicit user
/// workflow and forwards typed argv to <see cref="ConverterCliClient"/>.
/// </summary>
public sealed class MainViewModel : ObservableObject
{
    // The Rust sidecar uses ReplaceFileW with a manifest-bound backup plus a
    // durable recovery journal for Windows multi-file ExtData transactions.
    private static bool SupportsSafeExtrasInstall => true;

    private readonly ConverterCliClient _cliClient;
    private readonly FileFingerprintService _fingerprints;
    private readonly LanguagePreferenceStore _languageStore;

    private AppLanguageOverride _languageOverride;
    private ConversionMode _conversionMode = ConversionMode.NewConversion;
    private string? _repairFromVersion;
    private bool _repairRevisionSelectionRequired;
    private string _repairDetectionSummary = string.Empty;
    private string _selectedSlot = SavePathResolver.AvailableSlots[1];
    private string _sourcePath = string.Empty;
    private string _currentPath = string.Empty;
    private string _targetPath = string.Empty;
    private string _cliPath;
    private string _rollbackManifestPath = string.Empty;
    private bool _isSystemEnabled;
    private string _systemSourcePath = string.Empty;
    private string _systemCurrentPath = string.Empty;
    private string _systemTargetPath = string.Empty;
    private string _systemRollbackManifestPath = string.Empty;
    private bool _includeGuildCards;
    private bool _includeQuests;
    private string _extrasSourceDirectory = string.Empty;
    private string _extrasCurrentDirectory = string.Empty;
    private string _extrasStagingDirectory = string.Empty;
    private string _extrasTargetDirectory = string.Empty;
    private string _extrasRollbackManifestPath = string.Empty;
    private string _repairGuildCardsRollbackManifestPath = string.Empty;
    private string _repairQuestsRollbackManifestPath = string.Empty;
    private string _cecSourceDirectory = string.Empty;
    private string _cecCurrentPath = string.Empty;
    private string _cecTargetPath = string.Empty;
    private string _cecRollbackManifestPath = string.Empty;
    private bool _isCecEnabled;
    private bool _isCecAcknowledged;
    private bool _isBusy;
    private WorkflowStage _stage = WorkflowStage.Input;
    private string _statusText;
    private string _latestReport = string.Empty;
    private string _latestError = string.Empty;
    private bool _sourceInspected;
    private bool _currentInspected;
    private bool _targetInspected;
    private FileFingerprint? _inspectedSource;
    private FileFingerprint? _inspectedCurrent;
    private FileFingerprint? _inspectedTarget;
    private DryRunAuthorization? _coreAuthorization;
    private RepairDryRunAuthorization? _repairAuthorization;
    private SystemDryRunAuthorization? _systemAuthorization;
    private RepairSystemDryRunAuthorization? _repairSystemAuthorization;
    private ExtrasStageDryRunAuthorization? _extrasStageAuthorization;
    private ExtrasInstallDryRunAuthorization? _extrasInstallAuthorization;
    private RepairExtrasDryRunAuthorization? _repairGuildCardsAuthorization;
    private RepairExtrasDryRunAuthorization? _repairQuestsAuthorization;
    private CecDryRunAuthorization? _cecAuthorization;
    private RepairCecDryRunAuthorization? _repairCecAuthorization;
    private bool _systemWriteCompleted;
    private bool _repairGuildCardsWriteCompleted;
    private bool _repairQuestsWriteCompleted;
    private WorkflowGuidance _workflowGuidance;

    private enum AuthorizationDomain
    {
        Core,
        System,
        Extras,
        GuildCards,
        Quests,
        Cec,
    }

    private enum WorkflowGuidance
    {
        None,
        CoreInspected,
        CoreDryRunAuthorized,
        CoreWritten,
        OptionalStepComplete,
        RolledBack,
    }

    private sealed record RepairRevisionDetection(bool IsAmbiguous, string Summary);
    private sealed record RepairExtrasPaths(string Source, string Current, string Output);

    public MainViewModel(
        ConverterCliClient? cliClient = null,
        FileFingerprintService? fingerprints = null,
        LanguagePreferenceStore? languageStore = null)
    {
        _cliClient = cliClient ?? new ConverterCliClient();
        _fingerprints = fingerprints ?? new FileFingerprintService();
        _languageStore = languageStore ?? new LanguagePreferenceStore();
        _languageOverride = _languageStore.Load();
        Copy = new ConverterCopy(LanguagePreferenceStore.Resolve(_languageOverride));
        _cliPath = ConverterExecutableLocator.Locate();
        _statusText = Copy.Ready;
    }

    public ConverterCopy Copy { get; }
    public ObservableCollection<OperationHistoryItem> History { get; } = new();
    public IReadOnlyList<string> SaveSlots => SavePathResolver.AvailableSlots;

    public AppLanguageOverride LanguageOverride
    {
        get => _languageOverride;
        private set => SetProperty(ref _languageOverride, value);
    }

    public ConversionMode SelectedConversionMode => _conversionMode;
    public bool IsRepairMode => _conversionMode == ConversionMode.RepairConverted;
    public string ConversionModeDescription => IsRepairMode
        ? Copy.ConversionModeRepairDescription
        : Copy.ConversionModeNewDescription;
    public Visibility RepairCurrentVisibility => IsRepairMode ? Visibility.Visible : Visibility.Collapsed;
    public Visibility RepairModeVisibility => IsRepairMode ? Visibility.Visible : Visibility.Collapsed;
    public Visibility NewConversionVisibility => IsRepairMode ? Visibility.Collapsed : Visibility.Visible;
    public string CoreTargetTitle => IsRepairMode ? Copy.RepairOutputSlot : Copy.TargetSlot;
    public string CoreTargetHint => IsRepairMode ? Copy.RepairOutputSlotHint : Copy.TargetSlotHint;
    public string CoreTargetPlaceholder => IsRepairMode ? Copy.RepairOutputPlaceholder : Copy.NewOutputPlaceholder;
    public bool IsRepairRevisionSelectionRequired
    {
        get => _repairRevisionSelectionRequired;
        private set => SetProperty(ref _repairRevisionSelectionRequired, value);
    }
    public string RepairDetectionSummary
    {
        get => _repairDetectionSummary;
        private set => SetProperty(ref _repairDetectionSummary, value);
    }

    public void SetRepairFromVersion(string? tag)
    {
        var revision = tag is "0.0.3" or "0.0.4" or "0.0.5" or "0.0.6"
            ? tag
            : null;
        if (string.Equals(_repairFromVersion, revision, StringComparison.Ordinal))
        {
            return;
        }
        _repairFromVersion = revision;
        _repairAuthorization = null;
        _repairGuildCardsAuthorization = null;
        _repairQuestsAuthorization = null;
        IsRepairRevisionSelectionRequired = false;
        RepairDetectionSummary = string.Empty;
        OnPropertyChanged(nameof(CanWriteCore));
        RaiseRepairExtrasActionAvailability();
        OnPropertyChanged(nameof(WriteUnavailableVisibility));
    }

    public void SetConversionMode(string? tag)
    {
        var mode = string.Equals(tag, "repair", StringComparison.OrdinalIgnoreCase)
            ? ConversionMode.RepairConverted
            : ConversionMode.NewConversion;
        if (_conversionMode == mode)
        {
            return;
        }
        _conversionMode = mode;
        _repairFromVersion = null;
        IsRepairRevisionSelectionRequired = false;
        RepairDetectionSummary = string.Empty;
        InvalidateCoreAuthorization();
        InvalidateSystemAuthorization();
        InvalidateExtrasAuthorization();
        InvalidateCecAuthorization();
        OnPropertyChanged(nameof(SelectedConversionMode));
        OnPropertyChanged(nameof(IsRepairMode));
        OnPropertyChanged(nameof(ConversionModeDescription));
        OnPropertyChanged(nameof(RepairCurrentVisibility));
        OnPropertyChanged(nameof(RepairModeVisibility));
        OnPropertyChanged(nameof(NewConversionVisibility));
        OnPropertyChanged(nameof(CoreTargetTitle));
        OnPropertyChanged(nameof(CoreTargetHint));
        OnPropertyChanged(nameof(CoreTargetPlaceholder));
        RaiseOptionalConfigurationAvailability();
    }

    public string SourcePath
    {
        get => _sourcePath;
        set
        {
            if (SetProperty(ref _sourcePath, value))
            {
                InvalidateCoreAuthorization();
                OnPropertyChanged(nameof(SourcePathPreview));
            }
        }
    }

    public string TargetPath
    {
        get => _targetPath;
        set
        {
            if (SetProperty(ref _targetPath, value))
            {
                InvalidateCoreAuthorization();
                OnPropertyChanged(nameof(TargetPathPreview));
            }
        }
    }

    public string CurrentPath
    {
        get => _currentPath;
        set
        {
            if (SetProperty(ref _currentPath, value))
            {
                InvalidateCoreAuthorization();
                OnPropertyChanged(nameof(CurrentPathPreview));
            }
        }
    }

    public string SelectedSlot
    {
        get => _selectedSlot;
        set
        {
            if (SetProperty(ref _selectedSlot, value))
            {
                InvalidateCoreAuthorization();
                OnPropertyChanged(nameof(SourcePathPreview));
                OnPropertyChanged(nameof(CurrentPathPreview));
                OnPropertyChanged(nameof(TargetPathPreview));
            }
        }
    }

    public string SourcePathPreview
    {
        get
        {
            if (string.IsNullOrWhiteSpace(SourcePath))
            {
                return string.Empty;
            }

            return SavePathResolver.TryResolveSource(SourcePath, SelectedSlot, out var source, out var error)
                ? Copy.DescribeResolvedSource(source)
                : Copy.DescribePathError(error);
        }
    }

    public string TargetPathPreview
    {
        get
        {
            if (string.IsNullOrWhiteSpace(TargetPath))
            {
                return string.Empty;
            }

            return SavePathResolver.TryResolveTarget(TargetPath, SelectedSlot, out var target, out var error)
                ? Copy.DescribeResolvedTarget(target)
                : Copy.DescribePathError(error);
        }
    }

    public string CurrentPathPreview
    {
        get
        {
            if (string.IsNullOrWhiteSpace(CurrentPath))
            {
                return string.Empty;
            }

            return SavePathResolver.TryResolveCurrent(CurrentPath, SelectedSlot, out var current, out var error)
                ? Copy.DescribeResolvedCurrent(current)
                : Copy.DescribePathError(error);
        }
    }

    public string CliPath
    {
        get => _cliPath;
        set => SetProperty(ref _cliPath, value);
    }

    public string RollbackManifestPath
    {
        get => _rollbackManifestPath;
        set
        {
            if (SetProperty(ref _rollbackManifestPath, value))
            {
                OnPropertyChanged(nameof(CanRollbackCore));
            }
        }
    }

    public bool IsSystemEnabled
    {
        get => _isSystemEnabled;
        set
        {
            if (SetProperty(ref _isSystemEnabled, value))
            {
                InvalidateSystemAuthorization();
                OnPropertyChanged(nameof(CanRunSystemDryRun));
            }
        }
    }

    public string SystemSourcePath
    {
        get => _systemSourcePath;
        set
        {
            if (SetProperty(ref _systemSourcePath, value))
            {
                InvalidateSystemAuthorization();
            }
        }
    }

    public string SystemTargetPath
    {
        get => _systemTargetPath;
        set
        {
            if (SetProperty(ref _systemTargetPath, value))
            {
                InvalidateSystemAuthorization();
            }
        }
    }

    public string SystemCurrentPath
    {
        get => _systemCurrentPath;
        set
        {
            if (SetProperty(ref _systemCurrentPath, value))
            {
                InvalidateSystemAuthorization();
            }
        }
    }

    public string SystemRollbackManifestPath
    {
        get => _systemRollbackManifestPath;
        set
        {
            if (SetProperty(ref _systemRollbackManifestPath, value))
            {
                OnPropertyChanged(nameof(CanRollbackSystem));
            }
        }
    }

    public bool IncludeGuildCards
    {
        get => _includeGuildCards;
        set
        {
            if (SetProperty(ref _includeGuildCards, value))
            {
                InvalidateExtrasAuthorization();
            }
        }
    }

    public bool IncludeQuests
    {
        get => _includeQuests;
        set
        {
            if (SetProperty(ref _includeQuests, value))
            {
                InvalidateExtrasAuthorization();
            }
        }
    }

    public string ExtrasSourceDirectory
    {
        get => _extrasSourceDirectory;
        set
        {
            if (SetProperty(ref _extrasSourceDirectory, value))
            {
                InvalidateExtrasAuthorization();
                OnPropertyChanged(nameof(ExtrasSourcePathPreview));
            }
        }
    }

    public string ExtrasSourcePathPreview
    {
        get
        {
            if (string.IsNullOrWhiteSpace(ExtrasSourceDirectory))
            {
                return string.Empty;
            }

            return SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out var source, out var error)
                ? Copy.DescribeResolvedExtData(source)
                : Copy.DescribePathError(error);
        }
    }

    public string ExtrasStagingDirectory
    {
        get => _extrasStagingDirectory;
        set
        {
            if (SetProperty(ref _extrasStagingDirectory, value))
            {
                InvalidateExtrasAuthorization();
            }
        }
    }

    public string ExtrasCurrentDirectory
    {
        get => _extrasCurrentDirectory;
        set
        {
            if (SetProperty(ref _extrasCurrentDirectory, value))
            {
                InvalidateExtrasAuthorization();
            }
        }
    }

    public string ExtrasTargetDirectory
    {
        get => _extrasTargetDirectory;
        set
        {
            if (SetProperty(ref _extrasTargetDirectory, value))
            {
                InvalidateExtrasAuthorization();
            }
        }
    }

    public string ExtrasRollbackManifestPath
    {
        get => _extrasRollbackManifestPath;
        set
        {
            if (SetProperty(ref _extrasRollbackManifestPath, value))
            {
                OnPropertyChanged(nameof(CanRollbackExtras));
            }
        }
    }

    public string RepairGuildCardsRollbackManifestPath
    {
        get => _repairGuildCardsRollbackManifestPath;
        set
        {
            if (SetProperty(ref _repairGuildCardsRollbackManifestPath, value))
            {
                OnPropertyChanged(nameof(CanRollbackRepairGuildCards));
            }
        }
    }

    public string RepairQuestsRollbackManifestPath
    {
        get => _repairQuestsRollbackManifestPath;
        set
        {
            if (SetProperty(ref _repairQuestsRollbackManifestPath, value))
            {
                OnPropertyChanged(nameof(CanRollbackRepairQuests));
            }
        }
    }

    public bool IsCecEnabled
    {
        get => _isCecEnabled;
        set
        {
            if (SetProperty(ref _isCecEnabled, value))
            {
                InvalidateCecAuthorization();
            }
        }
    }

    public bool IsCecAcknowledged
    {
        get => _isCecAcknowledged;
        set
        {
            if (SetProperty(ref _isCecAcknowledged, value))
            {
                OnPropertyChanged(nameof(CanWriteCec));
            }
        }
    }

    public string CecSourceDirectory
    {
        get => _cecSourceDirectory;
        set
        {
            if (SetProperty(ref _cecSourceDirectory, value))
            {
                InvalidateCecAuthorization();
            }
        }
    }

    public string CecTargetPath
    {
        get => _cecTargetPath;
        set
        {
            if (SetProperty(ref _cecTargetPath, value))
            {
                InvalidateCecAuthorization();
            }
        }
    }

    public string CecCurrentPath
    {
        get => _cecCurrentPath;
        set
        {
            if (SetProperty(ref _cecCurrentPath, value))
            {
                InvalidateCecAuthorization();
            }
        }
    }

    public string CecRollbackManifestPath
    {
        get => _cecRollbackManifestPath;
        set
        {
            if (SetProperty(ref _cecRollbackManifestPath, value))
            {
                OnPropertyChanged(nameof(CanRollbackCec));
            }
        }
    }

    public bool IsBusy
    {
        get => _isBusy;
        private set
        {
            if (SetProperty(ref _isBusy, value))
            {
                OnPropertyChanged(nameof(CanInspectCore));
                OnPropertyChanged(nameof(CanInspectProgress));
                OnPropertyChanged(nameof(CanInspectEvents));
                OnPropertyChanged(nameof(CanRunCoreDryRun));
                OnPropertyChanged(nameof(CanWriteCore));
                OnPropertyChanged(nameof(WriteUnavailableVisibility));
                OnPropertyChanged(nameof(CanRollbackCore));
                OnPropertyChanged(nameof(CanRunSystemDryRun));
                OnPropertyChanged(nameof(CanWriteSystem));
                OnPropertyChanged(nameof(CanRollbackSystem));
                OnPropertyChanged(nameof(CanRunExtrasStageDryRun));
                OnPropertyChanged(nameof(CanStageExtras));
                OnPropertyChanged(nameof(CanRunExtrasInstallDryRun));
                OnPropertyChanged(nameof(CanInstallExtras));
                OnPropertyChanged(nameof(CanRollbackExtras));
                OnPropertyChanged(nameof(CanRunRepairGuildCardsDryRun));
                OnPropertyChanged(nameof(CanWriteRepairGuildCards));
                OnPropertyChanged(nameof(CanRollbackRepairGuildCards));
                OnPropertyChanged(nameof(CanRunRepairQuestsDryRun));
                OnPropertyChanged(nameof(CanWriteRepairQuests));
                OnPropertyChanged(nameof(CanRollbackRepairQuests));
                OnPropertyChanged(nameof(CanInspectCec));
                OnPropertyChanged(nameof(CanRunCecDryRun));
                OnPropertyChanged(nameof(CanWriteCec));
                OnPropertyChanged(nameof(CanRollbackCec));
            }
        }
    }

    public WorkflowStage Stage
    {
        get => _stage;
        private set => SetProperty(ref _stage, value);
    }

    public string StatusText
    {
        get => _statusText;
        private set => SetProperty(ref _statusText, value);
    }

    public string LatestReport
    {
        get => _latestReport;
        private set
        {
            if (SetProperty(ref _latestReport, value))
            {
                OnPropertyChanged(nameof(HasLatestReport));
                OnPropertyChanged(nameof(LatestReportEmptyVisibility));
            }
        }
    }

    public string LatestError
    {
        get => _latestError;
        private set
        {
            if (SetProperty(ref _latestError, value))
            {
                OnPropertyChanged(nameof(HasLatestError));
            }
        }
    }

    public bool HasLatestReport => !string.IsNullOrWhiteSpace(LatestReport);
    public Visibility LatestReportEmptyVisibility => HasLatestReport ? Visibility.Collapsed : Visibility.Visible;
    public bool HasLatestError => !string.IsNullOrWhiteSpace(LatestError);
    public bool ShowPostInspectGuidance => _workflowGuidance == WorkflowGuidance.CoreInspected;
    public bool ShowPostDryRunGuidance => _workflowGuidance == WorkflowGuidance.CoreDryRunAuthorized;
    public bool ShowPostWriteGuidance => _workflowGuidance == WorkflowGuidance.CoreWritten;
    public bool ShowPostOptionalGuidance => _workflowGuidance == WorkflowGuidance.OptionalStepComplete;
    public bool ShowPostRollbackGuidance => _workflowGuidance == WorkflowGuidance.RolledBack;
    public bool SelectedOptionalDataIsConfigured => IsRepairMode
        ? (!IsSystemEnabled || HasSystemPaths())
            && (!HasSelectedExtraGroups() || HasRepairExtrasPaths())
        : (!IsSystemEnabled || HasSystemPaths())
            && (!SupportsSafeExtrasInstall || !HasSelectedExtraGroups() || HasExtrasInstallPaths());
    public bool HasPendingSelectedOptionalWork => (IsSystemEnabled && !_systemWriteCompleted)
        || (IsRepairMode && IncludeGuildCards && !_repairGuildCardsWriteCompleted)
        || (IsRepairMode && IncludeQuests && !_repairQuestsWriteCompleted);
    public string PostWriteGuidanceMessage => HasPendingSelectedOptionalWork
        ? Copy.NextAfterCoreWriteWithOptionalData
        : Copy.NextAfterWrite;
    public string PostWriteGuidanceAction => HasPendingSelectedOptionalWork
        ? Copy.ContinueOptionalData
        : Copy.ReviewTransaction;
    public bool CanInspectCore => !IsBusy && HasValidCorePaths();
    public bool CanInspectProgress => !IsBusy && HasValidCorePaths();
    public bool CanInspectEvents => !IsBusy && HasValidCorePaths();
    public bool CanRunCoreDryRun => !IsBusy
        && _sourceInspected
        && _targetInspected
        && (!IsRepairMode || _currentInspected && _inspectedCurrent?.Exists == true)
        && HasValidCorePaths();
    public bool CanWriteCore => !IsBusy
        && (IsRepairMode ? _repairAuthorization is not null : _coreAuthorization is not null)
        && HasValidCorePaths();
    public Visibility WriteUnavailableVisibility => CanWriteCore ? Visibility.Collapsed : Visibility.Visible;
    public bool CanRollbackCore => !IsBusy && !string.IsNullOrWhiteSpace(RollbackManifestPath);
    public bool CanRunSystemDryRun => !IsBusy && IsSystemEnabled && HasSystemPaths();
    public bool CanWriteSystem => !IsBusy && IsSystemEnabled
        && (IsRepairMode ? _repairSystemAuthorization is not null : _systemAuthorization is not null)
        && HasSystemPaths();
    public bool CanRollbackSystem => !IsBusy && IsSystemEnabled && !string.IsNullOrWhiteSpace(SystemRollbackManifestPath);
    public bool CanRunExtrasStageDryRun => !IsBusy && HasExtrasStagePaths();
    public bool CanStageExtras => !IsBusy && _extrasStageAuthorization is not null && HasExtrasStagePaths();
    public bool CanRunExtrasInstallDryRun => !IsBusy && HasExtrasInstallPaths();
    public bool CanInstallExtras => SupportsSafeExtrasInstall && !IsBusy && _extrasInstallAuthorization is not null && HasExtrasInstallPaths();
    public bool CanRollbackExtras => SupportsSafeExtrasInstall && !IsBusy && HasSelectedExtraGroups() && !string.IsNullOrWhiteSpace(ExtrasRollbackManifestPath);
    public bool CanRunRepairGuildCardsDryRun => !IsBusy && IsRepairMode && IncludeGuildCards && HasRepairExtrasPaths();
    public bool CanWriteRepairGuildCards => !IsBusy && IsRepairMode && IncludeGuildCards
        && _repairGuildCardsAuthorization is not null && HasRepairExtrasPaths();
    public bool CanRollbackRepairGuildCards => !IsBusy && !string.IsNullOrWhiteSpace(RepairGuildCardsRollbackManifestPath);
    public bool CanRunRepairQuestsDryRun => !IsBusy && IsRepairMode && IncludeQuests && HasRepairExtrasPaths();
    public bool CanWriteRepairQuests => !IsBusy && IsRepairMode && IncludeQuests
        && _repairQuestsAuthorization is not null && HasRepairExtrasPaths();
    public bool CanRollbackRepairQuests => !IsBusy && !string.IsNullOrWhiteSpace(RepairQuestsRollbackManifestPath);
    public bool CanInspectCec => !IsBusy && IsCecEnabled && HasCecPaths();
    public bool CanRunCecDryRun => !IsBusy && IsCecEnabled && HasCecPaths();
    public bool CanWriteCec => !IsBusy && IsCecEnabled && IsCecAcknowledged
        && (IsRepairMode ? _repairCecAuthorization is not null : _cecAuthorization is not null)
        && HasCecPaths();
    public bool CanRollbackCec => !IsBusy && IsCecEnabled && !string.IsNullOrWhiteSpace(CecRollbackManifestPath);

    public void SetLanguage(string? tag)
    {
        var language = tag switch
        {
            "zh-Hans" => AppLanguageOverride.Chinese,
            "en" => AppLanguageOverride.English,
            _ => AppLanguageOverride.System,
        };
        LanguageOverride = language;
        try
        {
            _languageStore.Save(language);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            // The choice still takes effect for this process if a restrictive
            // profile prevents persistence.
        }
        Copy.SetLanguage(LanguagePreferenceStore.Resolve(language));
        StatusText = StageText(Stage);
        OnAllPropertiesChanged();
    }

    public void ReportUnexpected(Exception exception)
    {
        Fail(exception.Message);
    }

    public async Task InspectCoreAsync()
    {
        if (!TryRequireCorePaths(out var paths))
        {
            return;
        }

        InvalidateCoreAuthorization();
        await RunOperationAsync("inspect", async cancellationToken =>
        {
            var sourceAtInspection = await _fingerprints.CaptureAsync(paths.Source, cancellationToken);
            var currentAtInspection = IsRepairMode
                ? await _fingerprints.CaptureAsync(paths.Current!, cancellationToken)
                : null;
            var targetAtInspection = await _fingerprints.CaptureAsync(paths.Target, cancellationToken);
            var sourceReport = await ExecuteAsync("inspect", new[] { "inspect", paths.Source }, cancellationToken);
            RequireSuccess(sourceReport, "inspect source");
            if (currentAtInspection is not null)
            {
                var currentReport = await ExecuteAsync(
                    "inspect current",
                    new[] { "inspect", paths.Current! },
                    cancellationToken);
                RequireSuccess(currentReport, "inspect current");
            }
            if (targetAtInspection.Exists)
            {
                var targetReport = await ExecuteAsync("inspect", new[] { "inspect", paths.Target }, cancellationToken);
                RequireSuccess(targetReport, "inspect target");
            }
            var sourceAfterInspection = await _fingerprints.CaptureAsync(paths.Source, cancellationToken);
            var currentAfterInspection = IsRepairMode
                ? await _fingerprints.CaptureAsync(paths.Current!, cancellationToken)
                : null;
            var targetAfterInspection = await _fingerprints.CaptureAsync(paths.Target, cancellationToken);
            if (!sourceAtInspection.Matches(sourceAfterInspection)
                || (currentAtInspection is not null
                    && (currentAfterInspection is null || !currentAtInspection.Matches(currentAfterInspection)))
                || !targetAtInspection.Matches(targetAfterInspection))
            {
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }
            _sourceInspected = true;
            _currentInspected = !IsRepairMode || currentAfterInspection?.Exists == true;
            _targetInspected = true;
            _inspectedSource = sourceAfterInspection;
            _inspectedCurrent = currentAfterInspection;
            _inspectedTarget = targetAfterInspection;
            _coreAuthorization = null;
            Stage = WorkflowStage.Inspected;
            StatusText = Copy.Inspected;
            SetWorkflowGuidance(WorkflowGuidance.CoreInspected);
            RaiseCoreActionAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task InspectProgressAsync()
    {
        if (!TryRequireCorePaths(out var paths))
        {
            return;
        }

        await RunOperationAsync("inspect-progress", async cancellationToken =>
        {
            var arguments = new List<string> { "inspect-progress", paths.Source };
            var comparisonTarget = IsRepairMode ? paths.Current! : paths.Target;
            if (File.Exists(comparisonTarget))
            {
                arguments.Add("--target");
                arguments.Add(comparisonTarget);
            }
            var result = await ExecuteAsync("inspect-progress", arguments, cancellationToken);
            RequireSuccess(result, "inspect progress");
            StatusText = Copy.Inspected;
        }, AuthorizationDomain.Core);
    }

    public async Task InspectEventsAsync()
    {
        if (!TryRequireCorePaths(out var paths))
        {
            return;
        }

        await RunOperationAsync("inspect-events", async cancellationToken =>
        {
            var arguments = new List<string> { "inspect-events", paths.Source };
            var comparisonTarget = IsRepairMode ? paths.Current! : paths.Target;
            if (File.Exists(comparisonTarget))
            {
                arguments.Add("--target");
                arguments.Add(comparisonTarget);
            }
            var result = await ExecuteAsync("inspect-events", arguments, cancellationToken);
            RequireSuccess(result, "inspect events");
            StatusText = Copy.Inspected;
        }, AuthorizationDomain.Core);
    }

    public async Task RunCoreDryRunAsync()
    {
        if (!CanRunCoreDryRun)
        {
            Fail(Copy.WriteUnavailable);
            return;
        }
        if (!TryRequireCorePaths(out var paths))
        {
            return;
        }
        var inspectedSource = _inspectedSource;
        var inspectedCurrent = _inspectedCurrent;
        var inspectedTarget = _inspectedTarget;
        if (inspectedSource is null
            || inspectedTarget is null
            || (IsRepairMode && inspectedCurrent is null))
        {
            Fail(Copy.WriteUnavailable);
            return;
        }
        _coreAuthorization = null;
        _repairAuthorization = null;
        OnPropertyChanged(nameof(CanWriteCore));
        OnPropertyChanged(nameof(WriteUnavailableVisibility));

        var operation = IsRepairMode ? "repair-converted --dry-run" : "convert --dry-run";
        await RunOperationAsync(operation, async cancellationToken =>
        {
            var sourceBefore = await _fingerprints.CaptureAsync(paths.Source, cancellationToken);
            var currentBefore = IsRepairMode
                ? await _fingerprints.CaptureAsync(paths.Current!, cancellationToken)
                : null;
            var targetBefore = await _fingerprints.CaptureAsync(paths.Target, cancellationToken);
            if (!inspectedSource.Matches(sourceBefore)
                || (IsRepairMode
                    && (currentBefore is null || !inspectedCurrent!.Matches(currentBefore)))
                || !inspectedTarget.Matches(targetBefore))
            {
                InvalidateCoreAuthorization();
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }
            CliExecutionResult result;
            if (IsRepairMode)
            {
                var arguments = new List<string>
                {
                    "repair-converted", paths.Source,
                    "--current", paths.Current!,
                    "--output", paths.Target,
                };
                if (!string.IsNullOrWhiteSpace(_repairFromVersion))
                {
                    arguments.Add("--from-version");
                    arguments.Add(_repairFromVersion);
                }
                arguments.Add("--dry-run");
                result = await ExecuteAsync(operation, arguments, cancellationToken);
                RequireSuccess(result, "repair Dry Run");
                RequireStatus(result, "dry-run", "repair Dry Run");
                var sourceSet = result.TryGetString("source_set_sha256");
                var currentSet = result.TryGetString("current_set_sha256");
                var outputSet = result.TryGetString("output_set_sha256");
                var preview = result.TryGetString("preview_sha256");
                if (string.IsNullOrWhiteSpace(sourceSet)
                    || string.IsNullOrWhiteSpace(currentSet)
                    || string.IsNullOrWhiteSpace(outputSet)
                    || string.IsNullOrWhiteSpace(preview))
                {
                    throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
                }
                var detection = ReadRepairRevisionDetection(result);
                IsRepairRevisionSelectionRequired =
                    detection.IsAmbiguous && string.IsNullOrWhiteSpace(_repairFromVersion);
                RepairDetectionSummary = detection.Summary;
                _repairAuthorization = new RepairDryRunAuthorization(
                    sourceBefore,
                    currentBefore!,
                    targetBefore,
                    null,
                    _repairFromVersion,
                    sourceSet,
                    currentSet,
                    outputSet,
                    preview,
                    DateTimeOffset.UtcNow);
            }
            else
            {
                result = await ExecuteAsync(
                    operation,
                    new[] { "convert", paths.Source, "--output", paths.Target, "--dry-run" },
                    cancellationToken);
                RequireSuccess(result, "core Dry Run");
                RequireStatus(result, "dry-run", "core Dry Run");
            }

            var sourceAfter = await _fingerprints.CaptureAsync(paths.Source, cancellationToken);
            var currentAfter = IsRepairMode
                ? await _fingerprints.CaptureAsync(paths.Current!, cancellationToken)
                : null;
            var targetAfter = await _fingerprints.CaptureAsync(paths.Target, cancellationToken);
            if (!sourceBefore.Matches(sourceAfter)
                || (currentBefore is not null
                    && (currentAfter is null || !currentBefore.Matches(currentAfter)))
                || !targetBefore.Matches(targetAfter))
            {
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }
            if (IsRepairRevisionSelectionRequired)
            {
                _repairAuthorization = null;
                Stage = WorkflowStage.Inspected;
                StatusText = Copy.RepairVersionRequired;
                SetWorkflowGuidance(WorkflowGuidance.CoreInspected);
                RaiseCoreActionAvailability();
                return;
            }
            if (!IsRepairMode)
            {
                var reportSourceHash = result.TryGetHash("source");
                var reportTargetHash = result.TryGetHash("target_before");
                var targetMatchesDryRun = targetAfter.Exists
                    ? !string.IsNullOrWhiteSpace(targetAfter.Sha256)
                        && !string.IsNullOrWhiteSpace(reportTargetHash)
                        && string.Equals(targetAfter.Sha256, reportTargetHash, StringComparison.OrdinalIgnoreCase)
                    : string.IsNullOrWhiteSpace(reportTargetHash);
                if (!sourceAfter.Exists
                    || string.IsNullOrWhiteSpace(reportSourceHash)
                    || !string.Equals(sourceAfter.Sha256, reportSourceHash, StringComparison.OrdinalIgnoreCase)
                    || !targetMatchesDryRun)
                {
                    throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
                }
                _coreAuthorization = new DryRunAuthorization(
                    sourceAfter, targetAfter, reportSourceHash, DateTimeOffset.UtcNow);
            }
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            SetWorkflowGuidance(WorkflowGuidance.CoreDryRunAuthorized);
            RaiseCoreActionAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task WriteCoreAsync()
    {
        var authorization = _coreAuthorization;
        var repairAuthorization = _repairAuthorization;
        if (IsRepairMode ? repairAuthorization is null : authorization is null)
        {
            Fail(Copy.WriteUnavailable);
            return;
        }
        if (!TryRequireCorePaths(out var paths))
        {
            return;
        }

        var operation = IsRepairMode ? "repair-converted --write" : "convert --write";
        await RunOperationAsync(operation, async cancellationToken =>
        {
            var currentSource = await _fingerprints.CaptureAsync(paths.Source, cancellationToken);
            var currentReference = IsRepairMode
                ? await _fingerprints.CaptureAsync(paths.Current!, cancellationToken)
                : null;
            var currentTarget = await _fingerprints.CaptureAsync(paths.Target, cancellationToken);
            var authorizedSource = IsRepairMode ? repairAuthorization!.Source : authorization!.Source;
            var authorizedTarget = IsRepairMode ? repairAuthorization!.Output : authorization!.Target;
            if (!authorizedSource.Matches(currentSource)
                || (IsRepairMode
                    && (currentReference is null
                        || !repairAuthorization!.Current.Matches(currentReference)))
                || !authorizedTarget.Matches(currentTarget))
            {
                InvalidateCoreAuthorization();
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            Stage = WorkflowStage.Writing;
            if (IsRepairMode)
            {
                var repairArguments = new List<string>
                {
                    "repair-converted", paths.Source,
                    "--current", paths.Current!,
                    "--output", paths.Target,
                };
                if (!string.IsNullOrWhiteSpace(repairAuthorization.FromVersion))
                {
                    repairArguments.Add("--from-version");
                    repairArguments.Add(repairAuthorization.FromVersion);
                }
                repairArguments.AddRange(new[]
                {
                    "--write",
                    "--expected-source-set-sha256", repairAuthorization.SourceSetSha256,
                    "--expected-current-set-sha256", repairAuthorization.CurrentSetSha256,
                    "--expected-output-set-sha256", repairAuthorization.OutputSetSha256,
                    "--expected-preview-sha256", repairAuthorization.PreviewSha256,
                });
                var repairResult = await ExecuteAsync(operation, repairArguments, cancellationToken);
                RequireSuccess(repairResult, "repair converted save");
                if (repairResult.Status is not ("written" or "no-changes"))
                {
                    throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
                }
                _repairAuthorization = null;
                RollbackManifestPath = repairResult.TryGetString("compatibility_manifest")
                    ?? RollbackManifestPath;
                Stage = WorkflowStage.Written;
                StatusText = Copy.Written;
                SetWorkflowGuidance(WorkflowGuidance.CoreWritten);
                RaiseCoreActionAvailability();
                return;
            }
            var conversionAuthorization = authorization
                ?? throw new InvalidOperationException(Copy.WriteUnavailable);
            var arguments = new List<string>
            {
                "convert", paths.Source, "--output", paths.Target,
                "--expected-source-sha256", conversionAuthorization.SourceReportHash,
            };
            if (conversionAuthorization.Target.Exists)
            {
                var expectedTargetSha256 = conversionAuthorization.Target.Sha256
                    ?? throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
                arguments.Add("--expected-target-sha256");
                arguments.Add(expectedTargetSha256);
            }
            else
            {
                arguments.Add("--expected-target-absent");
            }
            arguments.Add("--write");
            var result = await ExecuteAsync(
                operation,
                arguments,
                cancellationToken);
            RequireSuccess(result, "write core slot");
            RequireStatus(result, "written", "write core slot");
            RollbackManifestPath = result.TryGetString("manifest") ?? RollbackManifestPath;
            _coreAuthorization = null;
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            SetWorkflowGuidance(WorkflowGuidance.CoreWritten);
            RaiseCoreActionAvailability();
            RaiseOptionalConfigurationAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task RollbackCoreAsync()
    {
        if (!CanRollbackCore)
        {
            return;
        }

        var isCompatibilityRollback = (Path.GetFileName(RollbackManifestPath) ?? string.Empty)
            .StartsWith(".mh3g-compatibility-repair-", StringComparison.Ordinal);
        var operation = isCompatibilityRollback ? "rollback-repair" : "rollback";
        await RunOperationAsync(operation, async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                operation,
                new[] { operation, "--manifest", RollbackManifestPath },
                cancellationToken);
            RequireSuccess(result, operation);
            RequireStatus(result, "rolled-back", operation);
            RollbackManifestPath = string.Empty;
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
            SetWorkflowGuidance(WorkflowGuidance.RolledBack);
            RaiseOptionalConfigurationAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task RunSystemDryRunAsync()
    {
        if (!TryRequireSystemPaths())
        {
            return;
        }

        if (IsRepairMode)
        {
            _repairSystemAuthorization = null;
            RaiseSystemActionAvailability();
            await RunOperationAsync("repair-system --dry-run", async cancellationToken =>
            {
                var result = await ExecuteAsync(
                    "repair-system --dry-run",
                    new[]
                    {
                        "repair-system", SystemSourcePath,
                        "--current", SystemCurrentPath,
                        "--output", SystemTargetPath,
                        "--dry-run",
                    },
                    cancellationToken);
                RequireSuccess(result, "system repair Dry Run");
                RequireStatus(result, "dry-run", "system repair Dry Run");
                var sourceSet = RequireReportSha256(result, "source_set_sha256");
                var currentSet = RequireReportSha256(result, "current_set_sha256");
                var outputSet = RequireReportSha256(result, "output_set_sha256");
                var preview = RequireReportSha256(result, "preview_sha256");
                _repairSystemAuthorization = new RepairSystemDryRunAuthorization(
                    Path.GetFullPath(SystemSourcePath),
                    Path.GetFullPath(SystemCurrentPath),
                    Path.GetFullPath(SystemTargetPath),
                    sourceSet,
                    currentSet,
                    outputSet,
                    preview,
                    DateTimeOffset.UtcNow);
                Stage = WorkflowStage.DryRunAuthorized;
                StatusText = Copy.DryRunAuthorized;
                SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
                RaiseSystemActionAvailability();
            }, AuthorizationDomain.System);
            return;
        }

        _systemAuthorization = null;
        RaiseSystemActionAvailability();
        await RunOperationAsync("convert-system --dry-run", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "convert-system --dry-run",
                new[] { "convert-system", SystemSourcePath, "--output", SystemTargetPath, "--dry-run" },
                cancellationToken);
            RequireSuccess(result, "system Dry Run");
            RequireStatus(result, "dry-run", "system Dry Run");

            var source = await _fingerprints.CaptureAsync(SystemSourcePath, cancellationToken);
            var target = await _fingerprints.CaptureAsync(SystemTargetPath, cancellationToken);
            var sourceHash = result.TryGetHash("source");
            var targetHash = result.TryGetHash("target_before");
            if (!source.Exists || !target.Exists
                || string.IsNullOrWhiteSpace(sourceHash)
                || string.IsNullOrWhiteSpace(targetHash)
                || !string.Equals(source.Sha256, sourceHash, StringComparison.OrdinalIgnoreCase)
                || !string.Equals(target.Sha256, targetHash, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            _systemAuthorization = new SystemDryRunAuthorization(source, target, sourceHash, DateTimeOffset.UtcNow);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseSystemActionAvailability();
        }, AuthorizationDomain.System);
    }

    public async Task WriteSystemAsync()
    {
        if (IsRepairMode)
        {
            var repairAuthorization = _repairSystemAuthorization;
            if (repairAuthorization is null || !MatchesRepairSystemAuthorization(repairAuthorization))
            {
                InvalidateSystemAuthorization();
                Fail(Copy.WriteUnavailable);
                return;
            }
            await RunOperationAsync("repair-system --write", async cancellationToken =>
            {
                var result = await ExecuteAsync(
                    "repair-system --write",
                    new[]
                    {
                        "repair-system", SystemSourcePath,
                        "--current", SystemCurrentPath,
                        "--output", SystemTargetPath,
                        "--write",
                        "--expected-source-set-sha256", repairAuthorization.SourceSetSha256,
                        "--expected-current-set-sha256", repairAuthorization.CurrentSetSha256,
                        "--expected-output-set-sha256", repairAuthorization.OutputSetSha256,
                        "--expected-preview-sha256", repairAuthorization.PreviewSha256,
                    },
                    cancellationToken);
                RequireSuccess(result, "repair system");
                if (result.Status is not ("written" or "no-changes"))
                {
                    throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
                }
                SystemRollbackManifestPath = result.TryGetString("manifest") ?? SystemRollbackManifestPath;
                _repairSystemAuthorization = null;
                _systemWriteCompleted = true;
                Stage = WorkflowStage.Written;
                StatusText = Copy.Written;
                SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
                RaiseSystemActionAvailability();
                RaiseOptionalConfigurationAvailability();
            }, AuthorizationDomain.System);
            return;
        }
        var authorization = _systemAuthorization;
        if (authorization is null || !HasSystemPaths())
        {
            Fail(Copy.WriteUnavailable);
            return;
        }

        await RunOperationAsync("convert-system --write", async cancellationToken =>
        {
            var source = await _fingerprints.CaptureAsync(SystemSourcePath, cancellationToken);
            var target = await _fingerprints.CaptureAsync(SystemTargetPath, cancellationToken);
            if (!authorization.Source.Matches(source) || !authorization.Target.Matches(target))
            {
                _systemAuthorization = null;
                RaiseSystemActionAvailability();
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            var expectedTargetHash = authorization.Target.Sha256
                ?? throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "convert-system --write",
                new[]
                {
                    "convert-system", SystemSourcePath, "--output", SystemTargetPath,
                    "--expected-source-sha256", authorization.SourceReportHash,
                    "--expected-target-sha256", expectedTargetHash,
                    "--write",
                },
                cancellationToken);
            RequireSuccess(result, "write system");
            RequireStatus(result, "written", "write system");
            SystemRollbackManifestPath = result.TryGetString("manifest") ?? SystemRollbackManifestPath;
            _systemAuthorization = null;
            _systemWriteCompleted = true;
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseSystemActionAvailability();
            RaiseOptionalConfigurationAvailability();
        }, AuthorizationDomain.System);
    }

    public async Task RollbackSystemAsync()
    {
        if (!CanRollbackSystem)
        {
            return;
        }

        await RunOperationAsync("rollback system", async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "rollback system",
                new[] { "rollback", "--manifest", SystemRollbackManifestPath },
                cancellationToken);
            RequireSuccess(result, "rollback system");
            RequireStatus(result, "rolled-back", "rollback system");
            SystemRollbackManifestPath = string.Empty;
            _systemWriteCompleted = false;
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
            SetWorkflowGuidance(WorkflowGuidance.RolledBack);
            RaiseOptionalConfigurationAvailability();
        }, AuthorizationDomain.System);
    }

    public Task RunRepairGuildCardsDryRunAsync() => RunRepairExtrasDryRunAsync("guild-cards");

    public Task WriteRepairGuildCardsAsync() => WriteRepairExtrasAsync("guild-cards");

    public Task RollbackRepairGuildCardsAsync() => RollbackRepairExtrasAsync("guild-cards");

    public Task RunRepairQuestsDryRunAsync() => RunRepairExtrasDryRunAsync("quests");

    public Task WriteRepairQuestsAsync() => WriteRepairExtrasAsync("quests");

    public Task RollbackRepairQuestsAsync() => RollbackRepairExtrasAsync("quests");

    private async Task RunRepairExtrasDryRunAsync(string group)
    {
        if (!IsRepairMode || !IsRepairGroupSelected(group) || !TryRequireRepairExtrasPaths(out var paths))
        {
            return;
        }
        SetRepairExtrasAuthorization(group, null);
        RaiseRepairExtrasActionAvailability();
        await RunOperationAsync($"repair-extras {group} --dry-run", async cancellationToken =>
        {
            var arguments = new List<string>
            {
                "repair-extras",
                "--source-dir", paths.Source,
                "--current-dir", paths.Current,
                "--output-dir", paths.Output,
                "--group", group,
            };
            if (!string.IsNullOrWhiteSpace(_repairFromVersion))
            {
                arguments.Add("--from-version");
                arguments.Add(_repairFromVersion);
            }
            arguments.Add("--dry-run");
            var result = await ExecuteAsync($"repair-extras {group} --dry-run", arguments, cancellationToken);
            RequireSuccess(result, $"{group} repair Dry Run");
            RequireStatus(result, "dry-run", $"{group} repair Dry Run");
            if (!string.Equals(result.TryGetString("group"), group, StringComparison.Ordinal))
            {
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }
            var detection = ReadRepairRevisionDetection(result);
            IsRepairRevisionSelectionRequired =
                detection.IsAmbiguous && string.IsNullOrWhiteSpace(_repairFromVersion);
            RepairDetectionSummary = detection.Summary;
            if (IsRepairRevisionSelectionRequired)
            {
                SetRepairExtrasAuthorization(group, null);
                Stage = WorkflowStage.Inspected;
                StatusText = Copy.RepairVersionRequired;
                RaiseRepairExtrasActionAvailability();
                return;
            }
            var authorization = new RepairExtrasDryRunAuthorization(
                group,
                paths.Source,
                paths.Current,
                paths.Output,
                _repairFromVersion,
                RequireReportSha256(result, "source_set_sha256"),
                RequireReportSha256(result, "current_set_sha256"),
                RequireReportSha256(result, "output_set_sha256"),
                RequireReportSha256(result, "preview_sha256"),
                DateTimeOffset.UtcNow);
            SetRepairExtrasAuthorization(group, authorization);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseRepairExtrasActionAvailability();
        }, group == "guild-cards" ? AuthorizationDomain.GuildCards : AuthorizationDomain.Quests);
    }

    private async Task WriteRepairExtrasAsync(string group)
    {
        var authorization = GetRepairExtrasAuthorization(group);
        if (authorization is null
            || !TryRequireRepairExtrasPaths(out var paths)
            || !MatchesRepairExtrasAuthorization(authorization, paths))
        {
            SetRepairExtrasAuthorization(group, null);
            RaiseRepairExtrasActionAvailability();
            Fail(Copy.WriteUnavailable);
            return;
        }
        await RunOperationAsync($"repair-extras {group} --write", async cancellationToken =>
        {
            var arguments = new List<string>
            {
                "repair-extras",
                "--source-dir", paths.Source,
                "--current-dir", paths.Current,
                "--output-dir", paths.Output,
                "--group", group,
            };
            if (!string.IsNullOrWhiteSpace(authorization.FromVersion))
            {
                arguments.Add("--from-version");
                arguments.Add(authorization.FromVersion);
            }
            arguments.AddRange(new[]
            {
                "--write",
                "--expected-source-set-sha256", authorization.SourceSetSha256,
                "--expected-current-set-sha256", authorization.CurrentSetSha256,
                "--expected-output-set-sha256", authorization.OutputSetSha256,
                "--expected-preview-sha256", authorization.PreviewSha256,
            });
            var result = await ExecuteAsync($"repair-extras {group} --write", arguments, cancellationToken);
            RequireSuccess(result, $"repair {group}");
            if (result.Status is not ("written" or "no-changes")
                || !string.Equals(result.TryGetString("group"), group, StringComparison.Ordinal))
            {
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }
            var manifest = result.TryGetString("manifest");
            if (group == "guild-cards")
            {
                RepairGuildCardsRollbackManifestPath = manifest ?? RepairGuildCardsRollbackManifestPath;
                _repairGuildCardsWriteCompleted = true;
            }
            else
            {
                RepairQuestsRollbackManifestPath = manifest ?? RepairQuestsRollbackManifestPath;
                _repairQuestsWriteCompleted = true;
            }
            SetRepairExtrasAuthorization(group, null);
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseRepairExtrasActionAvailability();
            RaiseOptionalConfigurationAvailability();
        }, group == "guild-cards" ? AuthorizationDomain.GuildCards : AuthorizationDomain.Quests);
    }

    private async Task RollbackRepairExtrasAsync(string group)
    {
        var manifest = group == "guild-cards"
            ? RepairGuildCardsRollbackManifestPath
            : RepairQuestsRollbackManifestPath;
        if (string.IsNullOrWhiteSpace(manifest))
        {
            return;
        }
        await RunOperationAsync($"rollback-extras {group}", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                $"rollback-extras {group}",
                new[] { "rollback-extras", "--manifest", manifest },
                cancellationToken);
            RequireSuccess(result, $"rollback {group}");
            RequireStatus(result, "rolled-back", $"rollback {group}");
            if (group == "guild-cards")
            {
                _repairGuildCardsWriteCompleted = false;
                RepairGuildCardsRollbackManifestPath = string.Empty;
            }
            else
            {
                _repairQuestsWriteCompleted = false;
                RepairQuestsRollbackManifestPath = string.Empty;
            }
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
            SetWorkflowGuidance(WorkflowGuidance.RolledBack);
            RaiseOptionalConfigurationAvailability();
        }, group == "guild-cards" ? AuthorizationDomain.GuildCards : AuthorizationDomain.Quests);
    }

    public async Task RunExtrasStageDryRunAsync()
    {
        if (!TryRequireExtrasStagePaths())
        {
            return;
        }

        _extrasStageAuthorization = null;
        _extrasInstallAuthorization = null;
        RaiseExtrasActionAvailability();
        await RunOperationAsync("convert-extras --dry-run", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "convert-extras --dry-run",
                ExtrasStageArguments(write: false),
                cancellationToken);
            RequireSuccess(result, "ExtData stage Dry Run");
            RequireStatus(result, "dry-run", "ExtData stage Dry Run");
            var fingerprint = result.TryGetExtrasComponentFingerprint()
                ?? throw new InvalidOperationException(Copy.ExtrasFingerprintMissing);

            if (!SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out var sourceDirectory, out var sourceError))
            {
                throw new InvalidOperationException(Copy.DescribePathError(sourceError));
            }

            _extrasStageAuthorization = new ExtrasStageDryRunAuthorization(
                Path.GetFullPath(sourceDirectory),
                Path.GetFullPath(ExtrasStagingDirectory),
                SelectedExtraGroups(),
                fingerprint,
                DateTimeOffset.UtcNow);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseExtrasActionAvailability();
        }, AuthorizationDomain.Extras);
    }

    public async Task StageExtrasAsync()
    {
        var authorization = _extrasStageAuthorization;
        if (authorization is null || !MatchesExtrasStageAuthorization(authorization))
        {
            InvalidateExtrasAuthorization();
            Fail(Copy.WriteUnavailable);
            return;
        }

        await RunOperationAsync("convert-extras --write", async cancellationToken =>
        {
            var verification = await ExecuteAsync(
                "convert-extras --dry-run verification",
                ExtrasStageArguments(write: false),
                cancellationToken);
            RequireSuccess(verification, "ExtData stage verification");
            RequireStatus(verification, "dry-run", "ExtData stage verification");
            var fingerprint = verification.TryGetExtrasComponentFingerprint();
            if (!string.Equals(fingerprint, authorization.ComponentFingerprint, StringComparison.Ordinal))
            {
                InvalidateExtrasAuthorization();
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync("convert-extras --write", ExtrasStageArguments(write: true), cancellationToken);
            RequireSuccess(result, "write ExtData staging");
            RequireStatus(result, "written", "write ExtData staging");
            InvalidateExtrasAuthorization();
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
        }, AuthorizationDomain.Extras);
    }

    public async Task RunExtrasInstallDryRunAsync()
    {
        if (!TryRequireExtrasInstallPaths())
        {
            return;
        }

        _extrasInstallAuthorization = null;
        RaiseExtrasActionAvailability();
        await RunOperationAsync("install-extras --dry-run", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "install-extras --dry-run",
                ExtrasInstallArguments(write: false, authorization: null),
                cancellationToken);
            RequireSuccess(result, "ExtData install Dry Run");
            RequireStatus(result, "dry-run", "ExtData install Dry Run");
            var reportedGroups = result.TryGetStringArrayFingerprint("groups");
            var stagingHash = result.TryGetString("staging_set_sha256");
            var targetHash = result.TryGetString("target_set_sha256_before");
            if (!string.Equals(reportedGroups, SelectedExtraGroups(), StringComparison.Ordinal)
                || string.IsNullOrWhiteSpace(stagingHash)
                || string.IsNullOrWhiteSpace(targetHash))
            {
                throw new InvalidOperationException(Copy.ExtrasFingerprintMissing);
            }

            _extrasInstallAuthorization = new ExtrasInstallDryRunAuthorization(
                Path.GetFullPath(ExtrasStagingDirectory),
                Path.GetFullPath(ExtrasTargetDirectory),
                SelectedExtraGroups(),
                stagingHash,
                targetHash,
                DateTimeOffset.UtcNow);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseExtrasActionAvailability();
        }, AuthorizationDomain.Extras);
    }

    public async Task InstallExtrasAsync()
    {
        var authorization = _extrasInstallAuthorization;
        if (authorization is null || !MatchesExtrasInstallAuthorization(authorization))
        {
            InvalidateExtrasAuthorization();
            Fail(Copy.WriteUnavailable);
            return;
        }

        await RunOperationAsync("install-extras --write", async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "install-extras --write",
                ExtrasInstallArguments(write: true, authorization: authorization),
                cancellationToken);
            RequireSuccess(result, "install ExtData");
            RequireStatus(result, "written", "install ExtData");
            ExtrasRollbackManifestPath = result.TryGetString("manifest") ?? ExtrasRollbackManifestPath;
            InvalidateExtrasAuthorization();
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            RaiseOptionalConfigurationAvailability();
        }, AuthorizationDomain.Extras);
    }

    public async Task RollbackExtrasAsync()
    {
        if (!CanRollbackExtras)
        {
            return;
        }

        await RunOperationAsync("rollback-extras", async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "rollback-extras",
                new[] { "rollback-extras", "--manifest", ExtrasRollbackManifestPath },
                cancellationToken);
            RequireSuccess(result, "rollback ExtData");
            RequireStatus(result, "rolled-back", "rollback ExtData");
            ExtrasRollbackManifestPath = string.Empty;
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
            SetWorkflowGuidance(WorkflowGuidance.RolledBack);
            RaiseOptionalConfigurationAvailability();
        }, AuthorizationDomain.Extras);
    }

    public async Task InspectCecAsync()
    {
        if (!TryRequireCecPaths())
        {
            return;
        }

        await RunOperationAsync("inspect-cec", async cancellationToken =>
        {
            var arguments = new List<string> { "inspect-cec", "--source-dir", CecSourceDirectory };
            var inspectionTarget = IsRepairMode ? CecCurrentPath : CecTargetPath;
            if (File.Exists(inspectionTarget))
            {
                arguments.Add("--target");
                arguments.Add(inspectionTarget);
            }
            if (SavePathResolver.TryResolveSource(SourcePath, SelectedSlot, out var sourceSlot, out _))
            {
                arguments.Add("--source-slot");
                arguments.Add(sourceSlot);
            }

            var result = await ExecuteAsync("inspect-cec", arguments, cancellationToken);
            RequireSuccess(result, "inspect CEC");
            Stage = WorkflowStage.Inspected;
            StatusText = Copy.Inspected;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
        }, AuthorizationDomain.Cec);
    }

    public async Task RunCecDryRunAsync()
    {
        if (!TryRequireCecPaths())
        {
            return;
        }
        _cecAuthorization = null;
        _repairCecAuthorization = null;
        OnPropertyChanged(nameof(CanWriteCec));

        if (IsRepairMode)
        {
            await RunOperationAsync("repair-cec --dry-run", async cancellationToken =>
            {
                var result = await ExecuteAsync(
                    "repair-cec --dry-run",
                    new[]
                    {
                        "repair-cec", "--source-dir", CecSourceDirectory,
                        "--current", CecCurrentPath,
                        "--output", CecTargetPath,
                        "--dry-run",
                    },
                    cancellationToken);
                RequireSuccess(result, "CEC repair Dry Run");
                RequireStatus(result, "dry-run", "CEC repair Dry Run");
                _repairCecAuthorization = new RepairCecDryRunAuthorization(
                    Path.GetFullPath(CecSourceDirectory),
                    Path.GetFullPath(CecCurrentPath),
                    Path.GetFullPath(CecTargetPath),
                    RequireReportSha256(result, "source_record_set_sha256"),
                    RequireReportSha256(result, "current_set_sha256"),
                    RequireReportSha256(result, "output_set_sha256"),
                    RequireReportSha256(result, "preview_sha256"),
                    DateTimeOffset.UtcNow);
                Stage = WorkflowStage.DryRunAuthorized;
                StatusText = Copy.DryRunAuthorized;
                SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
                OnPropertyChanged(nameof(CanWriteCec));
            }, AuthorizationDomain.Cec);
            return;
        }

        await RunOperationAsync("convert-cec --dry-run", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "convert-cec --dry-run",
                new[] { "convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath, "--dry-run" },
                cancellationToken);
            RequireSuccess(result, "CEC Dry Run");
            RequireStatus(result, "dry-run", "CEC Dry Run");
            var sourceRecordSet = result.TryGetString("source_record_set_sha256");
            var targetBefore = result.TryGetString("target_sha256_before");
            if (string.IsNullOrWhiteSpace(sourceRecordSet) || string.IsNullOrWhiteSpace(targetBefore))
            {
                throw new InvalidOperationException(Copy.CecFingerprintMissing);
            }

            _cecAuthorization = new CecDryRunAuthorization(sourceRecordSet, targetBefore, DateTimeOffset.UtcNow);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            OnPropertyChanged(nameof(CanWriteCec));
        }, AuthorizationDomain.Cec);
    }

    public async Task WriteCecAsync()
    {
        if (!IsCecEnabled)
        {
            Fail(Copy.CecDisabled);
            return;
        }
        if (!IsCecAcknowledged)
        {
            Fail(Copy.CecAcknowledgementRequired);
            return;
        }
        if (IsRepairMode)
        {
            var repairAuthorization = _repairCecAuthorization;
            if (repairAuthorization is null || !MatchesRepairCecAuthorization(repairAuthorization))
            {
                _repairCecAuthorization = null;
                OnPropertyChanged(nameof(CanWriteCec));
                Fail(Copy.WriteUnavailable);
                return;
            }
            await RunOperationAsync("repair-cec --write", async cancellationToken =>
            {
                var result = await ExecuteAsync(
                    "repair-cec --write",
                    new[]
                    {
                        "repair-cec", "--source-dir", CecSourceDirectory,
                        "--current", CecCurrentPath,
                        "--output", CecTargetPath,
                        "--expected-source-record-set-sha256", repairAuthorization.SourceRecordSetSha256,
                        "--expected-current-set-sha256", repairAuthorization.CurrentSetSha256,
                        "--expected-output-set-sha256", repairAuthorization.OutputSetSha256,
                        "--expected-preview-sha256", repairAuthorization.PreviewSha256,
                        "--write", "--experimental",
                    },
                    cancellationToken);
                RequireSuccess(result, "repair CEC");
                if (result.Status is not ("written" or "no-changes"))
                {
                    throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
                }
                CecRollbackManifestPath = result.TryGetString("manifest") ?? CecRollbackManifestPath;
                _repairCecAuthorization = null;
                Stage = WorkflowStage.Written;
                StatusText = Copy.Written;
                SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
                OnPropertyChanged(nameof(CanWriteCec));
            }, AuthorizationDomain.Cec);
            return;
        }
        var authorization = _cecAuthorization;
        if (authorization is null || !HasCecPaths())
        {
            Fail(Copy.WriteUnavailable);
            return;
        }

        await RunOperationAsync("convert-cec --write", async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            // Re-run the read-only CEC planner immediately before write because
            // its source is a mailbox directory rather than one core file.
            var verification = await ExecuteAsync(
                "convert-cec --dry-run verification",
                new[] { "convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath, "--dry-run" },
                cancellationToken);
            RequireSuccess(verification, "CEC pre-write verification");
            RequireStatus(verification, "dry-run", "CEC pre-write verification");
            var sourceRecordSet = verification.TryGetString("source_record_set_sha256");
            var target = verification.TryGetString("target_sha256_before");
            if (!string.Equals(sourceRecordSet, authorization.SourceRecordSetSha256, StringComparison.OrdinalIgnoreCase)
                || !string.Equals(target, authorization.TargetSha256Before, StringComparison.OrdinalIgnoreCase))
            {
                _cecAuthorization = null;
                OnPropertyChanged(nameof(CanWriteCec));
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            var result = await ExecuteAsync(
                "convert-cec --write",
                new[]
                {
                    "convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath,
                    "--expected-source-record-set-sha256", authorization.SourceRecordSetSha256,
                    "--expected-target-sha256", authorization.TargetSha256Before,
                    "--write", "--experimental",
                },
                cancellationToken);
            RequireSuccess(result, "write CEC");
            RequireStatus(result, "written", "write CEC");
            CecRollbackManifestPath = result.TryGetString("manifest") ?? CecRollbackManifestPath;
            _cecAuthorization = null;
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            SetWorkflowGuidance(WorkflowGuidance.OptionalStepComplete);
            OnPropertyChanged(nameof(CanWriteCec));
        }, AuthorizationDomain.Cec);
    }

    public async Task RollbackCecAsync()
    {
        if (!CanRollbackCec)
        {
            return;
        }

        await RunOperationAsync("rollback-cec", async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "rollback-cec",
                new[] { "rollback-cec", "--manifest", CecRollbackManifestPath },
                cancellationToken);
            RequireSuccess(result, "rollback CEC");
            RequireStatus(result, "rolled-back", "rollback CEC");
            CecRollbackManifestPath = string.Empty;
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
            SetWorkflowGuidance(WorkflowGuidance.RolledBack);
        }, AuthorizationDomain.Cec);
    }

    private async Task RunOperationAsync(
        string operation,
        Func<CancellationToken, Task> operationBody,
        AuthorizationDomain failureDomain)
    {
        if (IsBusy)
        {
            return;
        }

        IsBusy = true;
        LatestError = string.Empty;
        StatusText = Copy.Working;
        try
        {
            await operationBody(CancellationToken.None);
        }
        catch (Exception exception)
        {
            ClearWriteAuthorization(failureDomain);
            Fail(exception.Message, operation);
        }
        finally
        {
            IsBusy = false;
        }
    }

    private async Task<CliExecutionResult> ExecuteAsync(
        string operation,
        IEnumerable<string> arguments,
        CancellationToken cancellationToken)
    {
        var result = await _cliClient.ExecuteAsync(CliPath, arguments, cancellationToken);
        LatestReport = result.PrettyJson;
        LatestError = result.Succeeded ? string.Empty : ComposeFailure(result);
        History.Insert(0, new OperationHistoryItem(
            DateTimeOffset.Now,
            operation,
            result.Status,
            result.Succeeded,
            result.Succeeded ? result.Status : LatestError));
        return result;
    }

    private void RequireSuccess(CliExecutionResult result, string operation)
    {
        if (!result.Succeeded)
        {
            throw new InvalidOperationException($"{operation}: {ComposeFailure(result)}");
        }
    }

    private static void RequireStatus(CliExecutionResult result, string expected, string operation)
    {
        if (!string.Equals(result.Status, expected, StringComparison.Ordinal))
        {
            throw new InvalidOperationException($"{operation}: expected JSON status '{expected}', received '{result.Status}'.");
        }
    }

    private static string RequireReportSha256(CliExecutionResult result, string property)
    {
        var value = result.TryGetString(property);
        if (value is null
            || value.Length != 64
            || value.Any(character => !Uri.IsHexDigit(character)))
        {
            throw new InvalidOperationException($"converter report is missing a valid {property}");
        }
        return value;
    }

    private static string ComposeFailure(CliExecutionResult result)
    {
        var details = new[] { result.StandardError, result.JsonParseError, result.StandardOutput }
            .Where(value => !string.IsNullOrWhiteSpace(value));
        return string.Join(Environment.NewLine, details);
    }

    private RepairRevisionDetection ReadRepairRevisionDetection(CliExecutionResult result)
    {
        if (!result.Report.HasValue
            || !result.Report.Value.TryGetProperty("detection", out var detection)
            || !detection.TryGetProperty("confidence", out var confidence)
            || confidence.ValueKind != JsonValueKind.String
            || !detection.TryGetProperty("candidates", out var componentCandidates)
            || componentCandidates.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
        }

        var candidates = new SortedSet<string>(StringComparer.Ordinal);
        var ambiguous = string.Equals(
            confidence.GetString(), "ambiguous", StringComparison.Ordinal);
        foreach (var candidate in componentCandidates.EnumerateArray())
        {
            if (candidate.ValueKind == JsonValueKind.String
                && candidate.GetString() is { Length: > 0 } value)
            {
                candidates.Add(value);
            }
        }
        if (candidates.Count == 0)
        {
            throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
        }
        var joined = string.Join(", ", candidates);
        return new RepairRevisionDetection(
            ambiguous,
            ambiguous
                ? Copy.DescribeRepairVersionAmbiguous(joined)
                : Copy.DescribeRepairVersionDetected(joined));
    }

    private bool TryRequireCorePaths(out CoreSavePaths paths)
    {
        if (TryResolveCorePaths(out paths, out var error))
        {
            return true;
        }

        Fail(Copy.DescribePathError(error));
        paths = null!;
        return false;
    }

    private bool TryRequireSystemPaths()
    {
        if (HasSystemPaths())
        {
            return true;
        }

        Fail(IsRepairMode ? Copy.RepairSystemPathsRequired : Copy.SystemPathsRequired);
        return false;
    }

    private bool TryRequireExtrasStagePaths()
    {
        if (HasExtrasStagePaths())
        {
            return true;
        }

        if (HasSelectedExtraGroups()
            && !SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out _, out var error))
        {
            Fail(Copy.DescribePathError(error));
            return false;
        }

        Fail(Copy.ExtrasPathsRequired);
        return false;
    }

    private bool TryRequireExtrasInstallPaths()
    {
        if (HasExtrasInstallPaths())
        {
            return true;
        }

        if (HasSelectedExtraGroups()
            && !SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out _, out var error))
        {
            Fail(Copy.DescribePathError(error));
            return false;
        }

        Fail(Copy.ExtrasPathsRequired);
        return false;
    }

    private bool TryRequireCecPaths()
    {
        if (!IsCecEnabled)
        {
            Fail(Copy.CecDisabled);
            return false;
        }
        if (HasCecPaths())
        {
            return true;
        }

        Fail(IsRepairMode ? Copy.RepairCecPathsRequired : Copy.CecPathsRequired);
        return false;
    }

    private bool HasValidCorePaths()
    {
        return TryResolveCorePaths(out _, out _);
    }

    private bool TryResolveCorePaths(out CoreSavePaths paths, out SavePathResolutionError error)
    {
        CoreSavePaths? resolved;
        bool resolvedSuccessfully;
        if (IsRepairMode)
        {
            resolvedSuccessfully = SavePathResolver.TryResolveRepairCore(
                SourcePath,
                CurrentPath,
                TargetPath,
                SelectedSlot,
                out resolved,
                out error);
        }
        else
        {
            resolvedSuccessfully = SavePathResolver.TryResolveCore(
                SourcePath,
                TargetPath,
                SelectedSlot,
                out resolved,
                out error);
        }

        if (resolvedSuccessfully)
        {
            paths = resolved!;
            return true;
        }

        paths = null!;
        return false;
    }

    private bool HasSystemPaths()
    {
        var common = IsSystemEnabled
            && !string.IsNullOrWhiteSpace(SystemSourcePath)
            && !string.IsNullOrWhiteSpace(SystemTargetPath)
            && string.Equals(Path.GetFileName(SystemSourcePath), "system", StringComparison.Ordinal)
            && string.Equals(Path.GetFileName(SystemTargetPath), "system", StringComparison.Ordinal);
        return common
            && (!IsRepairMode
                || (!string.IsNullOrWhiteSpace(SystemCurrentPath)
                    && string.Equals(Path.GetFileName(SystemCurrentPath), "system", StringComparison.Ordinal)));
    }

    private bool HasSelectedExtraGroups() => IncludeGuildCards || IncludeQuests;

    private bool HasExtrasStagePaths()
    {
        return HasSelectedExtraGroups()
            && SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out _, out _)
            && !string.IsNullOrWhiteSpace(ExtrasStagingDirectory);
    }

    private bool HasExtrasInstallPaths()
    {
        return HasExtrasStagePaths()
            && !string.IsNullOrWhiteSpace(ExtrasTargetDirectory);
    }

    private bool HasRepairExtrasPaths()
    {
        return TryResolveRepairExtrasPaths(out _);
    }

    private bool TryRequireRepairExtrasPaths(out RepairExtrasPaths paths)
    {
        if (TryResolveRepairExtrasPaths(out paths))
        {
            return true;
        }
        Fail(Copy.RepairExtrasPathsRequired);
        paths = null!;
        return false;
    }

    private bool TryResolveRepairExtrasPaths(out RepairExtrasPaths paths)
    {
        if (IsRepairMode
            && HasSelectedExtraGroups()
            && SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out var source, out _)
            && Directory.Exists(ExtrasCurrentDirectory)
            && Directory.Exists(ExtrasTargetDirectory))
        {
            paths = new RepairExtrasPaths(
                Path.GetFullPath(source),
                Path.GetFullPath(ExtrasCurrentDirectory),
                Path.GetFullPath(ExtrasTargetDirectory));
            return true;
        }
        paths = null!;
        return false;
    }

    private bool IsRepairGroupSelected(string group) => group switch
    {
        "guild-cards" => IncludeGuildCards,
        "quests" => IncludeQuests,
        _ => false,
    };

    private RepairExtrasDryRunAuthorization? GetRepairExtrasAuthorization(string group) => group switch
    {
        "guild-cards" => _repairGuildCardsAuthorization,
        "quests" => _repairQuestsAuthorization,
        _ => null,
    };

    private void SetRepairExtrasAuthorization(string group, RepairExtrasDryRunAuthorization? authorization)
    {
        if (group == "guild-cards")
        {
            _repairGuildCardsAuthorization = authorization;
        }
        else if (group == "quests")
        {
            _repairQuestsAuthorization = authorization;
        }
    }

    private static bool MatchesRepairExtrasAuthorization(
        RepairExtrasDryRunAuthorization authorization,
        RepairExtrasPaths paths)
    {
        return string.Equals(authorization.SourceDirectory, paths.Source, StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.CurrentDirectory, paths.Current, StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.OutputDirectory, paths.Output, StringComparison.OrdinalIgnoreCase);
    }

    private bool MatchesRepairSystemAuthorization(RepairSystemDryRunAuthorization authorization)
    {
        return HasSystemPaths()
            && string.Equals(authorization.SourcePath, Path.GetFullPath(SystemSourcePath), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.CurrentPath, Path.GetFullPath(SystemCurrentPath), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.OutputPath, Path.GetFullPath(SystemTargetPath), StringComparison.OrdinalIgnoreCase);
    }

    private bool MatchesRepairCecAuthorization(RepairCecDryRunAuthorization authorization)
    {
        return HasCecPaths()
            && string.Equals(authorization.SourceDirectory, Path.GetFullPath(CecSourceDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.CurrentPath, Path.GetFullPath(CecCurrentPath), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.OutputPath, Path.GetFullPath(CecTargetPath), StringComparison.OrdinalIgnoreCase);
    }

    private string SelectedExtraGroups()
    {
        var groups = new List<string>();
        if (IncludeGuildCards)
        {
            groups.Add("guild-cards");
        }
        if (IncludeQuests)
        {
            groups.Add("quests");
        }
        return string.Join(",", groups);
    }

    private string[] ExtrasStageArguments(bool write)
    {
        if (!SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out var sourceDirectory, out var error))
        {
            throw new InvalidOperationException(Copy.DescribePathError(error));
        }

        return new[]
        {
            "convert-extras",
            "--source-dir", sourceDirectory,
            "--output-dir", ExtrasStagingDirectory,
            write ? "--write" : "--dry-run",
        };
    }

    private string[] ExtrasInstallArguments(bool write, ExtrasInstallDryRunAuthorization? authorization)
    {
        var arguments = new List<string>
        {
            "install-extras",
            "--staging-dir", ExtrasStagingDirectory,
            "--target-dir", ExtrasTargetDirectory,
            "--groups", SelectedExtraGroups(),
        };
        if (authorization is not null)
        {
            arguments.Add("--expected-staging-set-sha256");
            arguments.Add(authorization.StagingSetSha256);
            arguments.Add("--expected-target-set-sha256");
            arguments.Add(authorization.TargetSetSha256);
        }
        arguments.Add(write ? "--write" : "--dry-run");
        return arguments.ToArray();
    }

    private bool MatchesExtrasStageAuthorization(ExtrasStageDryRunAuthorization authorization)
    {
        if (!SavePathResolver.TryResolveExtDataUserDirectory(ExtrasSourceDirectory, out var sourceDirectory, out _))
        {
            return false;
        }

        return HasExtrasStagePaths()
            && string.Equals(authorization.SourceDirectory, Path.GetFullPath(sourceDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.StagingDirectory, Path.GetFullPath(ExtrasStagingDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.Groups, SelectedExtraGroups(), StringComparison.Ordinal);
    }

    private bool MatchesExtrasInstallAuthorization(ExtrasInstallDryRunAuthorization authorization)
    {
        return HasExtrasInstallPaths()
            && string.Equals(authorization.StagingDirectory, Path.GetFullPath(ExtrasStagingDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.TargetDirectory, Path.GetFullPath(ExtrasTargetDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.Groups, SelectedExtraGroups(), StringComparison.Ordinal);
    }

    private bool HasCecPaths()
    {
        return !string.IsNullOrWhiteSpace(CecSourceDirectory)
            && !string.IsNullOrWhiteSpace(CecTargetPath)
            && (!IsRepairMode || !string.IsNullOrWhiteSpace(CecCurrentPath));
    }

    private void InvalidateCoreAuthorization()
    {
        _sourceInspected = false;
        _currentInspected = false;
        _targetInspected = false;
        _inspectedSource = null;
        _inspectedCurrent = null;
        _inspectedTarget = null;
        _coreAuthorization = null;
        _repairAuthorization = null;
        if (_workflowGuidance is WorkflowGuidance.CoreInspected
            or WorkflowGuidance.CoreDryRunAuthorized
            or WorkflowGuidance.CoreWritten)
        {
            SetWorkflowGuidance(WorkflowGuidance.None);
        }
        if (!IsBusy && Stage is not WorkflowStage.Written and not WorkflowStage.RolledBack)
        {
            Stage = WorkflowStage.Input;
            StatusText = Copy.Ready;
        }
        RaiseCoreActionAvailability();
    }

    private void InvalidateSystemAuthorization()
    {
        _systemAuthorization = null;
        _repairSystemAuthorization = null;
        _systemWriteCompleted = false;
        ClearOptionalGuidance();
        RaiseSystemActionAvailability();
        RaiseOptionalConfigurationAvailability();
    }

    private void InvalidateExtrasAuthorization()
    {
        _extrasStageAuthorization = null;
        _extrasInstallAuthorization = null;
        _repairGuildCardsAuthorization = null;
        _repairQuestsAuthorization = null;
        _repairGuildCardsWriteCompleted = false;
        _repairQuestsWriteCompleted = false;
        ClearOptionalGuidance();
        RaiseExtrasActionAvailability();
        RaiseOptionalConfigurationAvailability();
    }

    private void InvalidateCecAuthorization()
    {
        _cecAuthorization = null;
        _repairCecAuthorization = null;
        ClearOptionalGuidance();
        OnPropertyChanged(nameof(CanInspectCec));
        OnPropertyChanged(nameof(CanRunCecDryRun));
        OnPropertyChanged(nameof(CanWriteCec));
        OnPropertyChanged(nameof(CanRollbackCec));
    }

    private void ClearWriteAuthorization(AuthorizationDomain domain)
    {
        switch (domain)
        {
            case AuthorizationDomain.Core:
                _coreAuthorization = null;
                _repairAuthorization = null;
                RaiseCoreActionAvailability();
                break;
            case AuthorizationDomain.System:
                _systemAuthorization = null;
                _repairSystemAuthorization = null;
                RaiseSystemActionAvailability();
                break;
            case AuthorizationDomain.Extras:
                _extrasStageAuthorization = null;
                _extrasInstallAuthorization = null;
                _repairGuildCardsAuthorization = null;
                _repairQuestsAuthorization = null;
                RaiseExtrasActionAvailability();
                RaiseRepairExtrasActionAvailability();
                break;
            case AuthorizationDomain.GuildCards:
                _repairGuildCardsAuthorization = null;
                RaiseRepairExtrasActionAvailability();
                break;
            case AuthorizationDomain.Quests:
                _repairQuestsAuthorization = null;
                RaiseRepairExtrasActionAvailability();
                break;
            case AuthorizationDomain.Cec:
                _cecAuthorization = null;
                _repairCecAuthorization = null;
                OnPropertyChanged(nameof(CanWriteCec));
                break;
            default:
                throw new ArgumentOutOfRangeException(nameof(domain), domain, null);
        }
    }

    private void RaiseCoreActionAvailability()
    {
        OnPropertyChanged(nameof(CanInspectCore));
        OnPropertyChanged(nameof(CanInspectProgress));
        OnPropertyChanged(nameof(CanInspectEvents));
        OnPropertyChanged(nameof(CanRunCoreDryRun));
        OnPropertyChanged(nameof(CanWriteCore));
        OnPropertyChanged(nameof(WriteUnavailableVisibility));
    }

    private void RaiseOptionalConfigurationAvailability()
    {
        OnPropertyChanged(nameof(SelectedOptionalDataIsConfigured));
        OnPropertyChanged(nameof(HasPendingSelectedOptionalWork));
        OnPropertyChanged(nameof(PostWriteGuidanceMessage));
        OnPropertyChanged(nameof(PostWriteGuidanceAction));
        OnPropertyChanged(nameof(CanRunCoreDryRun));
        OnPropertyChanged(nameof(CanWriteCore));
        OnPropertyChanged(nameof(WriteUnavailableVisibility));
    }

    private void SetWorkflowGuidance(WorkflowGuidance guidance)
    {
        if (_workflowGuidance == guidance)
        {
            return;
        }

        _workflowGuidance = guidance;
        OnPropertyChanged(nameof(ShowPostInspectGuidance));
        OnPropertyChanged(nameof(ShowPostDryRunGuidance));
        OnPropertyChanged(nameof(ShowPostWriteGuidance));
        OnPropertyChanged(nameof(ShowPostOptionalGuidance));
        OnPropertyChanged(nameof(ShowPostRollbackGuidance));
        OnPropertyChanged(nameof(PostWriteGuidanceMessage));
        OnPropertyChanged(nameof(PostWriteGuidanceAction));
    }

    private void ClearOptionalGuidance()
    {
        if (_workflowGuidance == WorkflowGuidance.OptionalStepComplete)
        {
            SetWorkflowGuidance(WorkflowGuidance.None);
        }
    }

    private void RaiseSystemActionAvailability()
    {
        OnPropertyChanged(nameof(CanRunSystemDryRun));
        OnPropertyChanged(nameof(CanWriteSystem));
        OnPropertyChanged(nameof(CanRollbackSystem));
    }

    private void RaiseExtrasActionAvailability()
    {
        OnPropertyChanged(nameof(CanRunExtrasStageDryRun));
        OnPropertyChanged(nameof(CanStageExtras));
        OnPropertyChanged(nameof(CanRunExtrasInstallDryRun));
        OnPropertyChanged(nameof(CanInstallExtras));
        OnPropertyChanged(nameof(CanRollbackExtras));
        RaiseRepairExtrasActionAvailability();
    }

    private void RaiseRepairExtrasActionAvailability()
    {
        OnPropertyChanged(nameof(CanRunRepairGuildCardsDryRun));
        OnPropertyChanged(nameof(CanWriteRepairGuildCards));
        OnPropertyChanged(nameof(CanRollbackRepairGuildCards));
        OnPropertyChanged(nameof(CanRunRepairQuestsDryRun));
        OnPropertyChanged(nameof(CanWriteRepairQuests));
        OnPropertyChanged(nameof(CanRollbackRepairQuests));
    }

    private void Fail(string message, string? operation = null)
    {
        SetWorkflowGuidance(WorkflowGuidance.None);
        LatestError = message;
        StatusText = Copy.Failed;
        Stage = WorkflowStage.Failed;
        if (operation is not null)
        {
            History.Insert(0, new OperationHistoryItem(DateTimeOffset.Now, operation, "failed", false, message));
        }
    }

    private string StageText(WorkflowStage stage)
    {
        return stage switch
        {
            WorkflowStage.Input => Copy.Ready,
            WorkflowStage.Inspected => Copy.Inspected,
            WorkflowStage.DryRunAuthorized => Copy.DryRunAuthorized,
            WorkflowStage.Writing => Copy.Working,
            WorkflowStage.Written => Copy.Written,
            WorkflowStage.RolledBack => Copy.RolledBack,
            WorkflowStage.Failed => Copy.Failed,
            _ => Copy.Ready,
        };
    }

    private static class ConverterExecutableLocator
    {
        public static string Locate()
        {
            var bundled = Path.Combine(AppContext.BaseDirectory, "tools", "mh3g-save-convert.exe");
            if (File.Exists(bundled))
            {
                return bundled;
            }

            var configured = Environment.GetEnvironmentVariable("MH3G_CONVERTER_CLI");
            return string.IsNullOrWhiteSpace(configured) ? bundled : configured;
        }
    }
}
