using System.Globalization;
using System.Text.Json;
using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

public sealed class LanguagePreferenceStore
{
    private const string SettingsFileName = "settings.json";
    private readonly string _settingsPath;

    public LanguagePreferenceStore(string? settingsDirectory = null)
    {
        var directory = settingsDirectory
            ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "MHToolkit", "MH3GSaveConverter");
        _settingsPath = Path.Combine(directory, SettingsFileName);
    }

    public AppLanguageOverride Load()
    {
        try
        {
            if (!File.Exists(_settingsPath))
            {
                return AppLanguageOverride.System;
            }

            var stored = JsonSerializer.Deserialize<StoredSettings>(File.ReadAllText(_settingsPath));
            return stored?.LanguageOverride ?? AppLanguageOverride.System;
        }
        catch (IOException)
        {
            return AppLanguageOverride.System;
        }
        catch (JsonException)
        {
            return AppLanguageOverride.System;
        }
    }

    public void Save(AppLanguageOverride languageOverride)
    {
        var directory = Path.GetDirectoryName(_settingsPath)!;
        Directory.CreateDirectory(directory);
        var temporaryPath = Path.Combine(directory, $".{SettingsFileName}.{Guid.NewGuid():N}.tmp");
        var bytes = JsonSerializer.SerializeToUtf8Bytes(new StoredSettings { LanguageOverride = languageOverride });
        File.WriteAllBytes(temporaryPath, bytes);
        File.Move(temporaryPath, _settingsPath, overwrite: true);
    }

    public static AppLanguageOverride Resolve(AppLanguageOverride languageOverride, CultureInfo? systemCulture = null)
    {
        if (languageOverride != AppLanguageOverride.System)
        {
            return languageOverride;
        }

        return (systemCulture ?? CultureInfo.CurrentUICulture).TwoLetterISOLanguageName.Equals("zh", StringComparison.OrdinalIgnoreCase)
            ? AppLanguageOverride.Chinese
            : AppLanguageOverride.English;
    }

    private sealed class StoredSettings
    {
        public AppLanguageOverride LanguageOverride { get; init; } = AppLanguageOverride.System;
    }
}
