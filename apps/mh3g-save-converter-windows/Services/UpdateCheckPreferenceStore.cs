using System.Globalization;
using System.Text.Json;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

public sealed class UpdateCheckPreferenceStore
{
    private const string StateFileName = "update-check.json";
    private readonly string _statePath;
    private bool _attemptedInProcess;

    public UpdateCheckPreferenceStore(string? settingsDirectory = null)
    {
        var directory = settingsDirectory
            ?? Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "MHToolkit",
                "MH3GSaveConverter");
        _statePath = Path.Combine(directory, StateFileName);
    }

    public bool ShouldCheckToday(DateTimeOffset? now = null)
    {
        if (_attemptedInProcess)
        {
            return false;
        }

        var localDate = (now ?? DateTimeOffset.Now).LocalDateTime.Date;
        try
        {
            if (!File.Exists(_statePath))
            {
                return true;
            }
            var stored = JsonSerializer.Deserialize<StoredState>(File.ReadAllText(_statePath));
            return !DateTime.TryParseExact(
                stored?.LastAttemptLocalDate,
                "yyyy-MM-dd",
                CultureInfo.InvariantCulture,
                DateTimeStyles.None,
                out var lastAttempt)
                || lastAttempt.Date != localDate;
        }
        catch (IOException)
        {
            return true;
        }
        catch (UnauthorizedAccessException)
        {
            return true;
        }
        catch (JsonException)
        {
            return true;
        }
    }

    public void MarkAttempt(DateTimeOffset? now = null)
    {
        _attemptedInProcess = true;
        var directory = Path.GetDirectoryName(_statePath)!;
        var temporaryPath = Path.Combine(directory, $".{StateFileName}.{Guid.NewGuid():N}.tmp");
        try
        {
            Directory.CreateDirectory(directory);
            var state = new StoredState
            {
                LastAttemptLocalDate = (now ?? DateTimeOffset.Now).LocalDateTime.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture),
            };
            File.WriteAllBytes(temporaryPath, JsonSerializer.SerializeToUtf8Bytes(state));
            File.Move(temporaryPath, _statePath, overwrite: true);
        }
        catch (IOException)
        {
            // The in-process gate still prevents repeated launch checks. A
            // read-only profile may retry on the next application launch.
        }
        catch (UnauthorizedAccessException)
        {
            // Update checks are advisory and never block the converter.
        }
        finally
        {
            try
            {
                File.Delete(temporaryPath);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
    }

    private sealed class StoredState
    {
        public string? LastAttemptLocalDate { get; init; }
    }
}
