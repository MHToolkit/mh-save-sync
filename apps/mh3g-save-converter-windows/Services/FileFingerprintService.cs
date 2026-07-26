using System.Security.Cryptography;
using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

/// <summary>
/// UI-side write authorization only. This does not parse or transform a save;
/// the Rust CLI remains the only conversion implementation.
/// </summary>
public sealed class FileFingerprintService
{
    public async Task<FileFingerprint> CaptureAsync(string path, CancellationToken cancellationToken)
    {
        var normalizedPath = Path.GetFullPath(path);
        if (!File.Exists(normalizedPath))
        {
            return new FileFingerprint(normalizedPath, Exists: false, Sha256: null);
        }

        await using var stream = new FileStream(
            normalizedPath,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            bufferSize: 128 * 1024,
            useAsync: true);
        using var hash = SHA256.Create();
        var bytes = await hash.ComputeHashAsync(stream, cancellationToken);
        return new FileFingerprint(normalizedPath, Exists: true, Convert.ToHexString(bytes).ToLowerInvariant());
    }
}
