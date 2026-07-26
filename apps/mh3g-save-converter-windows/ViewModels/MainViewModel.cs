using System.Collections.ObjectModel;
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
    private readonly ConverterCliClient _cliClient;
    private readonly FileFingerprintService _fingerprints;
    private readonly LanguagePreferenceStore _languageStore;

    private AppLanguageOverride _languageOverride;
    private string _sourcePath = string.Empty;
    private string _targetPath = string.Empty;
    private string _cliPath;
    private string _rollbackManifestPath = string.Empty;
    private bool _isSystemEnabled;
    private string _systemSourcePath = string.Empty;
    private string _systemTargetPath = string.Empty;
    private string _systemRollbackManifestPath = string.Empty;
    private bool _includeGuildCards;
    private bool _includeQuests;
    private string _extrasSourceDirectory = string.Empty;
    private string _extrasStagingDirectory = string.Empty;
    private string _extrasTargetDirectory = string.Empty;
    private string _extrasRollbackManifestPath = string.Empty;
    private string _cecSourceDirectory = string.Empty;
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
    private bool _targetInspected;
    private DryRunAuthorization? _coreAuthorization;
    private SystemDryRunAuthorization? _systemAuthorization;
    private ExtrasStageDryRunAuthorization? _extrasStageAuthorization;
    private ExtrasInstallDryRunAuthorization? _extrasInstallAuthorization;
    private CecDryRunAuthorization? _cecAuthorization;

    private enum AuthorizationDomain
    {
        Core,
        System,
        Extras,
        Cec,
    }

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

    public AppLanguageOverride LanguageOverride
    {
        get => _languageOverride;
        private set => SetProperty(ref _languageOverride, value);
    }

    public string SourcePath
    {
        get => _sourcePath;
        set
        {
            if (SetProperty(ref _sourcePath, value))
            {
                InvalidateCoreAuthorization();
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
            }
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
            }
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

    public bool IsCecEnabled
    {
        get => _isCecEnabled;
        set
        {
            if (SetProperty(ref _isCecEnabled, value))
            {
                _cecAuthorization = null;
                OnPropertyChanged(nameof(CanInspectCec));
                OnPropertyChanged(nameof(CanRunCecDryRun));
                OnPropertyChanged(nameof(CanWriteCec));
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
                _cecAuthorization = null;
                OnPropertyChanged(nameof(CanInspectCec));
                OnPropertyChanged(nameof(CanRunCecDryRun));
                OnPropertyChanged(nameof(CanWriteCec));
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
                _cecAuthorization = null;
                OnPropertyChanged(nameof(CanInspectCec));
                OnPropertyChanged(nameof(CanRunCecDryRun));
                OnPropertyChanged(nameof(CanWriteCec));
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
                OnPropertyChanged(nameof(CanRollbackCore));
                OnPropertyChanged(nameof(CanRunSystemDryRun));
                OnPropertyChanged(nameof(CanWriteSystem));
                OnPropertyChanged(nameof(CanRollbackSystem));
                OnPropertyChanged(nameof(CanRunExtrasStageDryRun));
                OnPropertyChanged(nameof(CanStageExtras));
                OnPropertyChanged(nameof(CanRunExtrasInstallDryRun));
                OnPropertyChanged(nameof(CanInstallExtras));
                OnPropertyChanged(nameof(CanRollbackExtras));
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
    public bool HasLatestError => !string.IsNullOrWhiteSpace(LatestError);
    public bool CanInspectCore => !IsBusy && HasValidCorePaths();
    public bool CanInspectProgress => !IsBusy && HasValidCorePaths();
    public bool CanInspectEvents => !IsBusy && HasValidCorePaths();
    public bool CanRunCoreDryRun => !IsBusy && _sourceInspected && _targetInspected && HasValidCorePaths();
    public bool CanWriteCore => !IsBusy && _coreAuthorization is not null && HasValidCorePaths();
    public bool CanRollbackCore => !IsBusy && !string.IsNullOrWhiteSpace(RollbackManifestPath);
    public bool CanRunSystemDryRun => !IsBusy && IsSystemEnabled && HasSystemPaths();
    public bool CanWriteSystem => !IsBusy && IsSystemEnabled && _systemAuthorization is not null && HasSystemPaths();
    public bool CanRollbackSystem => !IsBusy && IsSystemEnabled && !string.IsNullOrWhiteSpace(SystemRollbackManifestPath);
    public bool CanRunExtrasStageDryRun => !IsBusy && HasExtrasPaths();
    public bool CanStageExtras => !IsBusy && _extrasStageAuthorization is not null && HasExtrasPaths();
    public bool CanRunExtrasInstallDryRun => !IsBusy && HasExtrasPaths();
    public bool CanInstallExtras => !IsBusy && _extrasInstallAuthorization is not null && HasExtrasPaths();
    public bool CanRollbackExtras => !IsBusy && HasSelectedExtraGroups() && !string.IsNullOrWhiteSpace(ExtrasRollbackManifestPath);
    public bool CanInspectCec => !IsBusy && IsCecEnabled && HasCecPaths();
    public bool CanRunCecDryRun => !IsBusy && IsCecEnabled && HasCecPaths();
    public bool CanWriteCec => !IsBusy && IsCecEnabled && IsCecAcknowledged && _cecAuthorization is not null && HasCecPaths();
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
        if (!TryRequireCorePaths())
        {
            return;
        }

        await RunOperationAsync("inspect", async cancellationToken =>
        {
            var source = await ExecuteAsync("inspect", new[] { "inspect", SourcePath }, cancellationToken);
            RequireSuccess(source, "inspect source");
            var target = await ExecuteAsync("inspect", new[] { "inspect", TargetPath }, cancellationToken);
            RequireSuccess(target, "inspect target");
            _sourceInspected = true;
            _targetInspected = true;
            _coreAuthorization = null;
            Stage = WorkflowStage.Inspected;
            StatusText = Copy.Inspected;
            RaiseCoreActionAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task InspectProgressAsync()
    {
        if (!TryRequireCorePaths())
        {
            return;
        }

        await RunOperationAsync("inspect-progress", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "inspect-progress",
                new[] { "inspect-progress", SourcePath, "--target", TargetPath },
                cancellationToken);
            RequireSuccess(result, "inspect progress");
            StatusText = Copy.Inspected;
        }, AuthorizationDomain.Core);
    }

    public async Task InspectEventsAsync()
    {
        if (!TryRequireCorePaths())
        {
            return;
        }

        await RunOperationAsync("inspect-events", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "inspect-events",
                new[] { "inspect-events", SourcePath, "--target", TargetPath },
                cancellationToken);
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
        _coreAuthorization = null;
        OnPropertyChanged(nameof(CanWriteCore));

        await RunOperationAsync("convert --dry-run", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "convert --dry-run",
                new[] { "convert", SourcePath, "--output", TargetPath, "--dry-run" },
                cancellationToken);
            RequireSuccess(result, "core Dry Run");
            RequireStatus(result, "dry-run", "core Dry Run");

            var source = await _fingerprints.CaptureAsync(SourcePath, cancellationToken);
            var target = await _fingerprints.CaptureAsync(TargetPath, cancellationToken);
            var reportSourceHash = result.TryGetHash("source");
            var reportTargetHash = result.TryGetHash("target_before");
            if (!source.Exists || !target.Exists
                || string.IsNullOrWhiteSpace(reportSourceHash)
                || string.IsNullOrWhiteSpace(target.Sha256)
                || string.IsNullOrWhiteSpace(reportTargetHash)
                || !string.Equals(source.Sha256, reportSourceHash, StringComparison.OrdinalIgnoreCase)
                || !string.Equals(target.Sha256, reportTargetHash, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            _coreAuthorization = new DryRunAuthorization(source, target, reportSourceHash, DateTimeOffset.UtcNow);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
            RaiseCoreActionAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task WriteCoreAsync()
    {
        var authorization = _coreAuthorization;
        if (authorization is null)
        {
            Fail(Copy.WriteUnavailable);
            return;
        }

        await RunOperationAsync("convert --write", async cancellationToken =>
        {
            var currentSource = await _fingerprints.CaptureAsync(SourcePath, cancellationToken);
            var currentTarget = await _fingerprints.CaptureAsync(TargetPath, cancellationToken);
            if (!authorization.Source.Matches(currentSource) || !authorization.Target.Matches(currentTarget))
            {
                InvalidateCoreAuthorization();
                throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            }

            var expectedTargetSha256 = authorization.Target.Sha256
                ?? throw new InvalidOperationException(Copy.FileChangedAfterDryRun);
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "convert --write",
                new[]
                {
                    "convert", SourcePath, "--output", TargetPath,
                    "--expected-source-sha256", authorization.SourceReportHash,
                    "--expected-target-sha256", expectedTargetSha256,
                    "--write",
                },
                cancellationToken);
            RequireSuccess(result, "write core slot");
            RequireStatus(result, "written", "write core slot");
            RollbackManifestPath = result.TryGetString("manifest") ?? RollbackManifestPath;
            _coreAuthorization = null;
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            RaiseCoreActionAvailability();
        }, AuthorizationDomain.Core);
    }

    public async Task RollbackCoreAsync()
    {
        if (!CanRollbackCore)
        {
            return;
        }

        await RunOperationAsync("rollback", async cancellationToken =>
        {
            Stage = WorkflowStage.Writing;
            var result = await ExecuteAsync(
                "rollback",
                new[] { "rollback", "--manifest", RollbackManifestPath },
                cancellationToken);
            RequireSuccess(result, "rollback");
            RequireStatus(result, "rolled-back", "rollback");
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
        }, AuthorizationDomain.Core);
    }

    public async Task RunSystemDryRunAsync()
    {
        if (!TryRequireSystemPaths())
        {
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
            RaiseSystemActionAvailability();
        }, AuthorizationDomain.System);
    }

    public async Task WriteSystemAsync()
    {
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
            Stage = WorkflowStage.Written;
            StatusText = Copy.Written;
            RaiseSystemActionAvailability();
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
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
        }, AuthorizationDomain.System);
    }

    public async Task RunExtrasStageDryRunAsync()
    {
        if (!TryRequireExtrasPaths())
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

            _extrasStageAuthorization = new ExtrasStageDryRunAuthorization(
                Path.GetFullPath(ExtrasSourceDirectory),
                Path.GetFullPath(ExtrasStagingDirectory),
                SelectedExtraGroups(),
                fingerprint,
                DateTimeOffset.UtcNow);
            Stage = WorkflowStage.DryRunAuthorized;
            StatusText = Copy.DryRunAuthorized;
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
        }, AuthorizationDomain.Extras);
    }

    public async Task RunExtrasInstallDryRunAsync()
    {
        if (!TryRequireExtrasPaths())
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
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
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
            if (File.Exists(CecTargetPath))
            {
                arguments.Add("--target");
                arguments.Add(CecTargetPath);
            }
            if (File.Exists(SourcePath))
            {
                arguments.Add("--source-slot");
                arguments.Add(SourcePath);
            }

            var result = await ExecuteAsync("inspect-cec", arguments, cancellationToken);
            RequireSuccess(result, "inspect CEC");
            Stage = WorkflowStage.Inspected;
            StatusText = Copy.Inspected;
        }, AuthorizationDomain.Cec);
    }

    public async Task RunCecDryRunAsync()
    {
        if (!TryRequireCecPaths())
        {
            return;
        }
        _cecAuthorization = null;
        OnPropertyChanged(nameof(CanWriteCec));

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
            Stage = WorkflowStage.RolledBack;
            StatusText = Copy.RolledBack;
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

    private static string ComposeFailure(CliExecutionResult result)
    {
        var details = new[] { result.StandardError, result.JsonParseError, result.StandardOutput }
            .Where(value => !string.IsNullOrWhiteSpace(value));
        return string.Join(Environment.NewLine, details);
    }

    private bool TryRequireCorePaths()
    {
        if (HasValidCorePaths())
        {
            return true;
        }

        Fail(Copy.InvalidCorePaths);
        return false;
    }

    private bool TryRequireSystemPaths()
    {
        if (HasSystemPaths())
        {
            return true;
        }

        Fail(Copy.SystemPathsRequired);
        return false;
    }

    private bool TryRequireExtrasPaths()
    {
        if (HasExtrasPaths())
        {
            return true;
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

        Fail(Copy.CecPathsRequired);
        return false;
    }

    private bool HasValidCorePaths()
    {
        if (string.IsNullOrWhiteSpace(SourcePath) || string.IsNullOrWhiteSpace(TargetPath))
        {
            return false;
        }

        var sourceName = Path.GetFileName(SourcePath);
        var targetName = Path.GetFileName(TargetPath);
        return sourceName is "user1" or "user2" or "user3"
            && string.Equals(sourceName, targetName, StringComparison.Ordinal);
    }

    private bool HasSystemPaths()
    {
        return IsSystemEnabled
            && !string.IsNullOrWhiteSpace(SystemSourcePath)
            && !string.IsNullOrWhiteSpace(SystemTargetPath)
            && string.Equals(Path.GetFileName(SystemSourcePath), "system", StringComparison.Ordinal)
            && string.Equals(Path.GetFileName(SystemTargetPath), "system", StringComparison.Ordinal);
    }

    private bool HasSelectedExtraGroups() => IncludeGuildCards || IncludeQuests;

    private bool HasExtrasPaths()
    {
        return HasSelectedExtraGroups()
            && !string.IsNullOrWhiteSpace(ExtrasSourceDirectory)
            && !string.IsNullOrWhiteSpace(ExtrasStagingDirectory)
            && !string.IsNullOrWhiteSpace(ExtrasTargetDirectory);
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
        return new[]
        {
            "convert-extras",
            "--source-dir", ExtrasSourceDirectory,
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
        return HasExtrasPaths()
            && string.Equals(authorization.SourceDirectory, Path.GetFullPath(ExtrasSourceDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.StagingDirectory, Path.GetFullPath(ExtrasStagingDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.Groups, SelectedExtraGroups(), StringComparison.Ordinal);
    }

    private bool MatchesExtrasInstallAuthorization(ExtrasInstallDryRunAuthorization authorization)
    {
        return HasExtrasPaths()
            && string.Equals(authorization.StagingDirectory, Path.GetFullPath(ExtrasStagingDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.TargetDirectory, Path.GetFullPath(ExtrasTargetDirectory), StringComparison.OrdinalIgnoreCase)
            && string.Equals(authorization.Groups, SelectedExtraGroups(), StringComparison.Ordinal);
    }

    private bool HasCecPaths()
    {
        return !string.IsNullOrWhiteSpace(CecSourceDirectory)
            && !string.IsNullOrWhiteSpace(CecTargetPath);
    }

    private void InvalidateCoreAuthorization()
    {
        _sourceInspected = false;
        _targetInspected = false;
        _coreAuthorization = null;
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
        RaiseSystemActionAvailability();
    }

    private void InvalidateExtrasAuthorization()
    {
        _extrasStageAuthorization = null;
        _extrasInstallAuthorization = null;
        RaiseExtrasActionAvailability();
    }

    private void ClearWriteAuthorization(AuthorizationDomain domain)
    {
        switch (domain)
        {
            case AuthorizationDomain.Core:
                _coreAuthorization = null;
                RaiseCoreActionAvailability();
                break;
            case AuthorizationDomain.System:
                _systemAuthorization = null;
                RaiseSystemActionAvailability();
                break;
            case AuthorizationDomain.Extras:
                _extrasStageAuthorization = null;
                _extrasInstallAuthorization = null;
                RaiseExtrasActionAvailability();
                break;
            case AuthorizationDomain.Cec:
                _cecAuthorization = null;
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
    }

    private void Fail(string message, string? operation = null)
    {
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
