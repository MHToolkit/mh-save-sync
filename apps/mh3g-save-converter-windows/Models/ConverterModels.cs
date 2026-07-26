using System.Text.Json;

namespace MHToolkit.MH3GSaveConverter.Windows.Models;

public enum AppLanguageOverride
{
    System,
    Chinese,
    English,
}

public enum WorkflowStage
{
    Input,
    Inspected,
    DryRunAuthorized,
    Writing,
    Written,
    RolledBack,
    Failed,
}

public sealed record FileFingerprint(string Path, bool Exists, string? Sha256)
{
    public static readonly string MissingHash = "<missing>";

    public bool Matches(FileFingerprint other)
    {
        return string.Equals(Path, other.Path, StringComparison.OrdinalIgnoreCase)
            && Exists == other.Exists
            && string.Equals(Sha256 ?? MissingHash, other.Sha256 ?? MissingHash, StringComparison.OrdinalIgnoreCase);
    }
}

public sealed record DryRunAuthorization(
    FileFingerprint Source,
    FileFingerprint Target,
    string SourceReportHash,
    DateTimeOffset CompletedAt);

public sealed record CecDryRunAuthorization(
    string SourceRecordSetSha256,
    string TargetSha256Before,
    DateTimeOffset CompletedAt);

public sealed record OperationHistoryItem(
    DateTimeOffset Timestamp,
    string Operation,
    string Status,
    bool Succeeded,
    string Detail);

public sealed class CliExecutionResult
{
    public required string Executable { get; init; }
    public required IReadOnlyList<string> Arguments { get; init; }
    public required int ExitCode { get; init; }
    public required string StandardOutput { get; init; }
    public required string StandardError { get; init; }
    public JsonElement? Report { get; init; }
    public string? JsonParseError { get; init; }

    public bool Succeeded => ExitCode == 0 && Report.HasValue;

    public string Status => TryGetString("status") ?? (ExitCode == 0 ? "completed" : "failed");

    public string PrettyJson => Report.HasValue
        ? JsonSerializer.Serialize(Report.Value, new JsonSerializerOptions { WriteIndented = true })
        : StandardOutput;

    public string? TryGetString(string property)
    {
        if (!Report.HasValue || Report.Value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        return Report.Value.TryGetProperty(property, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;
    }

    public string? TryGetHash(string name)
    {
        if (!Report.HasValue || Report.Value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        if (!Report.Value.TryGetProperty("hashes", out var hashes) || hashes.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        return hashes.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;
    }

    public string? TryGetStringArrayFingerprint(string property)
    {
        if (!Report.HasValue || Report.Value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        if (!Report.Value.TryGetProperty(property, out var values) || values.ValueKind != JsonValueKind.Array)
        {
            return null;
        }

        return string.Join(",", values.EnumerateArray()
            .Where(value => value.ValueKind == JsonValueKind.String)
            .Select(value => value.GetString())
            .Where(value => !string.IsNullOrWhiteSpace(value)));
    }
}
