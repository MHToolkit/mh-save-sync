using System.Diagnostics;
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

        var startInfo = new ProcessStartInfo
        {
            FileName = executable,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
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
