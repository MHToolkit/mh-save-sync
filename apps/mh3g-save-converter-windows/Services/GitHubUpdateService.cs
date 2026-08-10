using System.Net.Http.Headers;
using System.Reflection;
using System.Text.Json;
using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

public sealed class GitHubUpdateService
{
    private static readonly Uri LatestReleaseEndpoint = new(
        "https://api.github.com/repos/MHToolkit/mh-save-sync/releases/latest");

    private readonly HttpClient _httpClient;

    public GitHubUpdateService(HttpClient? httpClient = null, string? currentVersion = null)
    {
        _httpClient = httpClient ?? new HttpClient { Timeout = TimeSpan.FromSeconds(8) };
        CurrentVersion = currentVersion ?? ResolveCurrentVersion();
    }

    public string CurrentVersion { get; }

    public async Task<UpdateCheckResult> CheckAsync(CancellationToken cancellationToken = default)
    {
        using var request = new HttpRequestMessage(HttpMethod.Get, LatestReleaseEndpoint);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
        request.Headers.Add("X-GitHub-Api-Version", "2022-11-28");
        request.Headers.UserAgent.ParseAdd($"MH3GSaveConverter/{SafeUserAgentVersion(CurrentVersion)}");

        using var response = await _httpClient.SendAsync(request, cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        if (response.Content.Headers.ContentLength is > 2 * 1024 * 1024)
        {
            throw new InvalidDataException("GitHub Release API response exceeded the accepted size.");
        }

        var payload = await response.Content.ReadAsByteArrayAsync(cancellationToken).ConfigureAwait(false);
        if (payload.Length > 2 * 1024 * 1024)
        {
            throw new InvalidDataException("GitHub Release API response exceeded the accepted size.");
        }

        var release = JsonSerializer.Deserialize<GitHubReleaseInfo>(payload)
            ?? throw new InvalidDataException("GitHub Release API returned an empty payload.");
        ValidateRelease(release);

        if (!TryParseVersion(CurrentVersion, out var current)
            || !TryParseVersion(release.TagName, out var latest))
        {
            throw new InvalidDataException("The current or latest release version could not be compared safely.");
        }

        return new UpdateCheckResult(CurrentVersion, release, latest > current);
    }

    internal static bool TryParseVersion(string value, out Version version)
    {
        var normalized = value.Trim().TrimStart('v', 'V').Split('+', 2)[0].Split('-', 2)[0];
        if (Version.TryParse(normalized, out var parsed))
        {
            version = parsed;
            return true;
        }
        version = new Version(0, 0);
        return false;
    }

    private static void ValidateRelease(GitHubReleaseInfo release)
    {
        if (release.Draft
            || release.Prerelease
            || !Uri.TryCreate(release.HtmlUrl, UriKind.Absolute, out var releaseUri)
            || !releaseUri.Scheme.Equals(Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase)
            || !releaseUri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase)
            || !releaseUri.AbsolutePath.StartsWith("/MHToolkit/mh-save-sync/releases/", StringComparison.Ordinal))
        {
            throw new InvalidDataException("GitHub Release metadata did not match the official MHToolkit repository.");
        }
    }

    private static string ResolveCurrentVersion()
    {
        var assembly = Assembly.GetEntryAssembly() ?? typeof(GitHubUpdateService).Assembly;
        var informational = assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion;
        if (!string.IsNullOrWhiteSpace(informational))
        {
            return informational.Split('+', 2)[0];
        }
        return assembly.GetName().Version?.ToString(3) ?? "development";
    }

    private static string SafeUserAgentVersion(string version)
    {
        return TryParseVersion(version, out var parsed) ? parsed.ToString() : "0.0.0";
    }
}
