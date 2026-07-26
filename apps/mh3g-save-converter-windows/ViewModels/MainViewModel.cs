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
    private CecDryRunAuthorization? _cecAuthorization;

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
        });
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
        });
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
        });
    }

    public async Task RunCoreDryRunAsync()
    {
        if (!CanRunCoreDryRun)
        {
            Fail(Copy.WriteUnavailable);
            return;
        }

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
        });
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
        });
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
        });
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
            StatusText = Copy.Inspected;
        });
    }

    public async Task RunCecDryRunAsync()
    {
        if (!TryRequireCecPaths())
        {
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
            var sourceRecords = result.TryGetStringArrayFingerprint("source_record_sha256");
            var targetBefore = result.TryGetString("target_sha256_before");
            if (sourceRecords is null || targetBefore is null)
            {
                throw new InvalidOperationException(Copy.CecFingerprintMissing);
            }

            _cecAuthorization = new CecDryRunAuthorization(sourceRecords, targetBefore, DateTimeOffset.UtcNow);
            StatusText = Copy.DryRunAuthorized;
            OnPropertyChanged(nameof(CanWriteCec));
        });
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
        if (_cecAuthorization is null || !HasCecPaths())
        {
            Fail(Copy.WriteUnavailable);
            return;
        }

        await RunOperationAsync("convert-cec --write", async cancellationToken =>
        {
            // Re-run the read-only CEC planner immediately before write because
            // its source is a mailbox directory rather than one core file.
            var verification = await ExecuteAsync(
                "convert-cec --dry-run verification",
                new[] { "convert-cec", "--source-dir", CecSourceDirectory, "--target", CecTargetPath, "--dry-run" },
                cancellationToken);
            RequireSuccess(verification, "CEC pre-write verification");
            RequireStatus(verification, "dry-run", "CEC pre-write verification");
            var records = verification.TryGetStringArrayFingerprint("source_record_sha256");
            var target = verification.TryGetString("target_sha256_before");
            if (!string.Equals(records, _cecAuthorization.SourceRecordSetSha256, StringComparison.Ordinal)
                || !string.Equals(target, _cecAuthorization.TargetSha256Before, StringComparison.OrdinalIgnoreCase))
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
                    "--write", "--experimental",
                },
                cancellationToken);
            RequireSuccess(result, "write CEC");
            RequireStatus(result, "written", "write CEC");
            CecRollbackManifestPath = result.TryGetString("manifest") ?? CecRollbackManifestPath;
            _cecAuthorization = null;
            StatusText = Copy.Written;
            OnPropertyChanged(nameof(CanWriteCec));
        });
    }

    public async Task RollbackCecAsync()
    {
        if (!CanRollbackCec)
        {
            return;
        }

        await RunOperationAsync("rollback-cec", async cancellationToken =>
        {
            var result = await ExecuteAsync(
                "rollback-cec",
                new[] { "rollback-cec", "--manifest", CecRollbackManifestPath },
                cancellationToken);
            RequireSuccess(result, "rollback CEC");
            RequireStatus(result, "rolled-back", "rollback CEC");
            StatusText = Copy.RolledBack;
        });
    }

    private async Task RunOperationAsync(string operation, Func<CancellationToken, Task> operationBody)
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
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or InvalidOperationException)
        {
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

    private void RaiseCoreActionAvailability()
    {
        OnPropertyChanged(nameof(CanInspectCore));
        OnPropertyChanged(nameof(CanInspectProgress));
        OnPropertyChanged(nameof(CanInspectEvents));
        OnPropertyChanged(nameof(CanRunCoreDryRun));
        OnPropertyChanged(nameof(CanWriteCore));
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
