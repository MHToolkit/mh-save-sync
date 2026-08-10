using System.Globalization;
using System.Net;
using System.Net.Http.Headers;
using System.Reflection;
using System.Text.RegularExpressions;
using System.Text.Json;
using System.Xml;
using System.Xml.Linq;
using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

public sealed class GitHubUpdateService
{
    private static readonly Uri LatestReleaseEndpoint = new(
        "https://api.github.com/repos/MHToolkit/mh-save-sync/releases/latest");
    private static readonly Uri LatestReleaseWebEndpoint = new(
        "https://github.com/MHToolkit/mh-save-sync/releases/latest");
    private static readonly Uri ReleaseAtomEndpoint = new(
        "https://github.com/MHToolkit/mh-save-sync/releases.atom");

    private readonly HttpClient _httpClient;

    public GitHubUpdateService(HttpClient? httpClient = null, string? currentVersion = null)
    {
        _httpClient = httpClient ?? new HttpClient { Timeout = TimeSpan.FromSeconds(8) };
        CurrentVersion = currentVersion ?? ResolveCurrentVersion();
    }

    public string CurrentVersion { get; }

    public async Task<UpdateCheckResult> CheckAsync(CancellationToken cancellationToken = default)
    {
        GitHubReleaseInfo release;
        try
        {
            release = await FetchFromWebFeedAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception webFailure) when (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                release = await FetchFromApiAsync(cancellationToken).ConfigureAwait(false);
            }
            catch (Exception apiFailure) when (!cancellationToken.IsCancellationRequested)
            {
                throw new InvalidDataException(
                    $"GitHub release page/feed and Release API were unavailable. Web: {webFailure.Message} API: {apiFailure.Message}",
                    new AggregateException(webFailure, apiFailure));
            }
        }

        return CreateResult(release);
    }

    private async Task<GitHubReleaseInfo> FetchFromWebFeedAsync(CancellationToken cancellationToken)
    {
        using var latestRequest = CreateRequest(HttpMethod.Get, LatestReleaseWebEndpoint, "text/html");
        // HttpClient follows GitHub's stable /releases/latest redirect. One
        // byte is enough to retain the final tag URL without downloading the
        // full HTML release page.
        latestRequest.Headers.Range = new RangeHeaderValue(0, 0);
        using var latestResponse = await _httpClient.SendAsync(
            latestRequest,
            HttpCompletionOption.ResponseHeadersRead,
            cancellationToken).ConfigureAwait(false);

        Uri? stableReleaseUri;
        if (latestResponse.StatusCode is HttpStatusCode.MovedPermanently
            or HttpStatusCode.Found
            or HttpStatusCode.SeeOther
            or HttpStatusCode.TemporaryRedirect
            or HttpStatusCode.PermanentRedirect)
        {
            stableReleaseUri = latestResponse.Headers.Location is { } location
                ? new Uri(LatestReleaseWebEndpoint, location)
                : null;
        }
        else
        {
            latestResponse.EnsureSuccessStatusCode();
            stableReleaseUri = latestResponse.RequestMessage?.RequestUri;
        }

        if (stableReleaseUri is null || !IsOfficialReleaseTagUri(stableReleaseUri))
        {
            throw new InvalidDataException("GitHub latest-release redirect did not resolve to the official repository tag.");
        }

        using var feedRequest = CreateRequest(HttpMethod.Get, ReleaseAtomEndpoint, "application/atom+xml");
        using var feedResponse = await _httpClient.SendAsync(feedRequest, cancellationToken).ConfigureAwait(false);
        feedResponse.EnsureSuccessStatusCode();
        var payload = await ReadBoundedContentAsync(
            feedResponse,
            "GitHub release feed",
            cancellationToken).ConfigureAwait(false);
        return ParseAtomRelease(payload, stableReleaseUri);
    }

    private async Task<GitHubReleaseInfo> FetchFromApiAsync(CancellationToken cancellationToken)
    {
        using var request = CreateRequest(
            HttpMethod.Get,
            LatestReleaseEndpoint,
            "application/vnd.github+json");
        request.Headers.Add("X-GitHub-Api-Version", "2022-11-28");

        using var response = await _httpClient.SendAsync(request, cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var payload = await ReadBoundedContentAsync(
            response,
            "GitHub Release API",
            cancellationToken).ConfigureAwait(false);

        var release = JsonSerializer.Deserialize<GitHubReleaseInfo>(payload)
            ?? throw new InvalidDataException("GitHub Release API returned an empty payload.");
        ValidateRelease(release);
        return release;
    }

    private HttpRequestMessage CreateRequest(HttpMethod method, Uri endpoint, string accept)
    {
        var request = new HttpRequestMessage(method, endpoint);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue(accept));
        request.Headers.UserAgent.ParseAdd($"MH3GSaveConverter/{SafeUserAgentVersion(CurrentVersion)}");
        return request;
    }

    private static async Task<byte[]> ReadBoundedContentAsync(
        HttpResponseMessage response,
        string source,
        CancellationToken cancellationToken)
    {
        if (response.Content.Headers.ContentLength is > 2 * 1024 * 1024)
        {
            throw new InvalidDataException($"{source} response exceeded the accepted size.");
        }

        var payload = await response.Content.ReadAsByteArrayAsync(cancellationToken).ConfigureAwait(false);
        if (payload.Length > 2 * 1024 * 1024)
        {
            throw new InvalidDataException($"{source} response exceeded the accepted size.");
        }
        return payload;
    }

    private static GitHubReleaseInfo ParseAtomRelease(byte[] payload, Uri stableReleaseUri)
    {
        var settings = new XmlReaderSettings
        {
            DtdProcessing = DtdProcessing.Prohibit,
            XmlResolver = null,
            MaxCharactersInDocument = 2 * 1024 * 1024,
        };
        using var input = new MemoryStream(payload, writable: false);
        using var reader = XmlReader.Create(input, settings);
        var document = XDocument.Load(reader, LoadOptions.None);
        XNamespace atom = "http://www.w3.org/2005/Atom";
        var entry = document.Root?
            .Elements(atom + "entry")
            .FirstOrDefault(candidate =>
            {
                var href = candidate
                    .Elements(atom + "link")
                    .FirstOrDefault(link => string.Equals(
                        (string?)link.Attribute("rel"),
                        "alternate",
                        StringComparison.Ordinal))?
                    .Attribute("href")?
                    .Value;
                return Uri.TryCreate(href, UriKind.Absolute, out var entryUri)
                    && string.Equals(
                        entryUri.AbsoluteUri,
                        stableReleaseUri.AbsoluteUri,
                        StringComparison.Ordinal);
            })
            ?? throw new InvalidDataException("The stable GitHub release was not present in the official release feed.");

        var tagName = Uri.UnescapeDataString(stableReleaseUri.Segments[^1]);
        DateTimeOffset? publishedAt = null;
        if (DateTimeOffset.TryParse(
            entry.Element(atom + "updated")?.Value,
            CultureInfo.InvariantCulture,
            DateTimeStyles.AssumeUniversal | DateTimeStyles.AdjustToUniversal,
            out var parsedPublishedAt))
        {
            publishedAt = parsedPublishedAt;
        }

        var release = new GitHubReleaseInfo
        {
            TagName = tagName,
            Name = entry.Element(atom + "title")?.Value.Trim(),
            Body = PlainTextFromHtml(entry.Element(atom + "content")?.Value ?? string.Empty),
            HtmlUrl = stableReleaseUri.AbsoluteUri,
            PublishedAt = publishedAt,
            Draft = false,
            Prerelease = false,
        };
        ValidateRelease(release);
        return release;
    }

    private static string PlainTextFromHtml(string html)
    {
        var text = Regex.Replace(html, @"(?i)<li(?:\s[^>]*)?>", "• ");
        text = Regex.Replace(
            text,
            @"(?i)<br\s*/?>|</li\s*>|</p\s*>|</h[1-6]\s*>|</blockquote\s*>|</pre\s*>",
            "\n");
        text = Regex.Replace(text, @"<[^>]+>", string.Empty);
        text = WebUtility.HtmlDecode(text);
        var normalized = string.Join(
            Environment.NewLine,
            text.Split('\n', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries));
        return normalized.Length <= 32_000 ? normalized : normalized[..32_000];
    }

    private UpdateCheckResult CreateResult(GitHubReleaseInfo release)
    {
        ValidateRelease(release);

        if (!TryParseVersion(CurrentVersion, out var current)
            || !TryParseVersion(release.TagName, out var latest))
        {
            throw new InvalidDataException("The current or latest release version could not be compared safely.");
        }

        return new UpdateCheckResult(CurrentVersion, release, latest > current);
    }

    private static bool IsOfficialReleaseTagUri(Uri releaseUri)
    {
        return releaseUri.Scheme.Equals(Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase)
            && releaseUri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase)
            && string.IsNullOrEmpty(releaseUri.Query)
            && string.IsNullOrEmpty(releaseUri.Fragment)
            && releaseUri.AbsolutePath.StartsWith(
                "/MHToolkit/mh-save-sync/releases/tag/",
                StringComparison.Ordinal)
            && releaseUri.Segments.Length == 6
            && TryParseVersion(Uri.UnescapeDataString(releaseUri.Segments[^1]), out _);
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
