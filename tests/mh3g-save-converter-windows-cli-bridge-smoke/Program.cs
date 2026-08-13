using System.Text;
using System.Text.Json;
using MHToolkit.MH3GSaveConverter.Windows.Models;
using MHToolkit.MH3GSaveConverter.Windows.Services;

const string EmitArgument = "--emit-utf8-json";
// This exact synthetic path is also a regression vector: interpreting its
// UTF-8 JSON bytes as CP936 consumes one of the two separators before `user1`
// and produces the original invalid `\u` escape failure.
const string ExpectedPath = @"F:\存档转换·枫叶峰\3ds存档·镰版峰\user1";
const string ExpectedError = "辅助进程 UTF-8 错误流";

if (args is [EmitArgument])
{
    WriteUtf8(Console.OpenStandardOutput(), BuildPayload() + "\n");
    WriteUtf8(Console.OpenStandardError(), ExpectedError + "\n");
    return 0;
}

var executable = Environment.ProcessPath
    ?? throw new InvalidOperationException("Could not resolve the smoke-test process path.");
var childArguments = new List<string>();
if (string.Equals(Path.GetFileNameWithoutExtension(executable), "dotnet", StringComparison.OrdinalIgnoreCase))
{
    childArguments.Add(typeof(Program).Assembly.Location);
}
childArguments.Add(EmitArgument);

Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
var cp936 = Encoding.GetEncoding(
    936,
    EncoderFallback.ReplacementFallback,
    DecoderFallback.ReplacementFallback);
var corruptedJson = cp936.GetString(Encoding.UTF8.GetBytes(BuildPayload()));
var regressionVectorRejected = false;
try
{
    using var _ = JsonDocument.Parse(corruptedJson);
}
catch (JsonException)
{
    regressionVectorRejected = true;
}
Require(
    regressionVectorRejected,
    "The CP936 regression vector no longer reproduces the invalid JSON \\u escape.");

CliExecutionResult result;
// The child emits raw UTF-8 regardless of the parent console locale. This is
// the same protocol used by the Rust sidecar. On a CP936 Windows desktop the
// old bridge decoded that byte stream with the console code page; the product
// client must now select UTF-8 explicitly before Process.Start.
var originalConsoleEncoding = Console.OutputEncoding;
try
{
    if (OperatingSystem.IsWindows())
    {
        Console.OutputEncoding = cp936;
    }

    result = await new ConverterCliClient().ExecuteAsync(
        executable,
        childArguments,
        CancellationToken.None);
}
finally
{
    Console.OutputEncoding = originalConsoleEncoding;
}

Require(result.Succeeded, $"UTF-8 JSON report was not parsed: {result.JsonParseError}");
Require(
    string.Equals(result.TryGetString("source"), ExpectedPath, StringComparison.Ordinal),
    $"UTF-8 path changed during process transport: {result.TryGetString("source")}");
Require(
    string.Equals(result.StandardError.TrimEnd(), ExpectedError, StringComparison.Ordinal),
    $"UTF-8 stderr changed during process transport: {result.StandardError}");

Console.WriteLine("Windows CLI UTF-8 bridge smoke passed.");
return 0;

static string BuildPayload() => JsonSerializer.Serialize(new
{
    operation = "repair-converted",
    status = "dry-run",
    source = ExpectedPath,
}, new JsonSerializerOptions
{
    Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
});

static void WriteUtf8(Stream stream, string value)
{
    var bytes = Encoding.UTF8.GetBytes(value);
    stream.Write(bytes);
    stream.Flush();
}

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}
