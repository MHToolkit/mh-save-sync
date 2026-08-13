using System.Diagnostics;
using System.Text;
using System.Text.Json;
using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

/// <summary>
/// A deliberately narrow bridge to the bundled Rust CLI. Arguments are added
/// one-by-one and stdout is treated as a structured JSON report; this layer
/// never constructs a shell command or interprets save bytes.
/// </summary>
public sealed class ConverterCliClient
{
    private static readonly Encoding StrictUtf8 = new UTF8Encoding(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private static readonly byte[] LegacyWrapperMarker =
        "mh3g-save-convert-core.exe"u8.ToArray();

    public async Task<CliExecutionResult> ExecuteAsync(
        string executable,
        IEnumerable<string> arguments,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(executable);

        var argumentList = arguments.ToArray();
        if (argumentList.Any(string.IsNullOrWhiteSpace))
        {
            throw new ArgumentException("CLI arguments cannot be empty.", nameof(arguments));
        }

        if (!File.Exists(executable))
        {
            return new CliExecutionResult
            {
                Executable = executable,
                Arguments = argumentList,
                ExitCode = -1,
                StandardOutput = string.Empty,
                StandardError = $"The converter CLI was not found: {executable}",
                JsonParseError = "CLI sidecar is missing.",
            };
        }

        try
        {
            var executableBytes = await File.ReadAllBytesAsync(executable, cancellationToken);
            if (ContainsLegacyWrapperMarker(executableBytes))
            {
                return LaunchFailure(
                    executable,
                    argumentList,
                    "Legacy compatibility wrapper detected. Rebuild the Windows package from 0.0.4 or newer so tools\\mh3g-save-convert.exe is the native Rust CLI. / 检测到旧版兼容包装器，请使用 0.0.4 或更高版本源码重新打包，确保 tools\\mh3g-save-convert.exe 为原生 Rust CLI。");
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            return LaunchFailure(executable, argumentList, $"The converter CLI could not be inspected: {exception.Message}");
        }

        var startInfo = new ProcessStartInfo
        {
            FileName = executable,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = StrictUtf8,
            StandardErrorEncoding = StrictUtf8,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(executable) ?? AppContext.BaseDirectory,
        };

        foreach (var argument in argumentList)
        {
            startInfo.ArgumentList.Add(argument);
        }

        using var process = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
        try
        {
            if (!process.Start())
            {
                return LaunchFailure(executable, argumentList, "The converter process did not start.");
            }
        }
        catch (Exception exception) when (exception is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            return LaunchFailure(executable, argumentList, exception.Message);
        }

        var stdoutTask = process.StandardOutput.ReadToEndAsync(cancellationToken);
        var stderrTask = process.StandardError.ReadToEndAsync(cancellationToken);
        await process.WaitForExitAsync(cancellationToken);
        var stdout = await stdoutTask;
        var stderr = await stderrTask;

        var report = TryParseLastJsonLine(stdout, out var parseError);
        return new CliExecutionResult
        {
            Executable = executable,
            Arguments = argumentList,
            ExitCode = process.ExitCode,
            StandardOutput = stdout,
            StandardError = stderr,
            Report = report,
            JsonParseError = parseError,
        };
    }

    private static CliExecutionResult LaunchFailure(string executable, IReadOnlyList<string> arguments, string message)
    {
        return new CliExecutionResult
        {
            Executable = executable,
            Arguments = arguments,
            ExitCode = -1,
            StandardOutput = string.Empty,
            StandardError = message,
            JsonParseError = "The converter process could not be launched.",
        };
    }

    private static bool ContainsLegacyWrapperMarker(byte[] bytes)
    {
        if (bytes.Length < LegacyWrapperMarker.Length)
        {
            return false;
        }

        for (var offset = 0; offset <= bytes.Length - LegacyWrapperMarker.Length; offset++)
        {
            if (bytes.AsSpan(offset, LegacyWrapperMarker.Length).SequenceEqual(LegacyWrapperMarker))
            {
                return true;
            }
        }

        return false;
    }

    private static JsonElement? TryParseLastJsonLine(string stdout, out string? parseError)
    {
        parseError = null;
        var candidate = stdout
            .Split(new[] { "\r\n", "\n" }, StringSplitOptions.RemoveEmptyEntries)
            .Reverse()
            .FirstOrDefault(line => line.TrimStart().StartsWith('{'));

        if (candidate is null)
        {
            parseError = "The CLI did not emit a JSON report on stdout.";
            return null;
        }

        try
        {
            using var document = JsonDocument.Parse(candidate);
            return document.RootElement.Clone();
        }
        catch (JsonException exception)
        {
            parseError = exception.Message;
            return null;
        }
    }
}
