$ErrorActionPreference = "Stop"

$executable = Join-Path $PSScriptRoot "mh3g-save-convert.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Converter executable is missing: $executable"
}
$checksumFile = Join-Path $PSScriptRoot "mh3g-save-convert.exe.sha256"
if (-not (Test-Path -LiteralPath $checksumFile -PathType Leaf)) {
    throw "Converter checksum is missing: $checksumFile"
}
$checksumLine = (Get-Content -LiteralPath $checksumFile -Raw).Trim()
if ($checksumLine -notmatch '^(?<hash>[0-9a-fA-F]{64})\s+mh3g-save-convert\.exe$') {
    throw "Converter checksum file is invalid: $checksumFile"
}
$expectedHash = $Matches.hash.ToLowerInvariant()
$actualHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
    throw "Converter checksum mismatch: expected $expectedHash, got $actualHash"
}

# ZIPs downloaded by browsers or chat clients can propagate Mark-of-the-Web to
# extracted executables. Removing that per-file download marker does not bypass
# Windows application-control policy; it only makes an explicitly downloaded
# local package runnable under the current user's normal permissions.
try {
    Unblock-File -LiteralPath $executable -ErrorAction Stop
} catch {
    Write-Warning "Could not remove the download marker from '$executable': $($_.Exception.Message)"
}

& $executable @args
$exitCode = $LASTEXITCODE
if ($null -eq $exitCode) {
    throw "Converter did not return a process exit code."
}
exit $exitCode
