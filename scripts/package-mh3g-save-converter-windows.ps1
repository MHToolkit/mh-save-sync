[CmdletBinding()]
param(
    # The script owns and clears only a stage beneath <repo>\artifacts.
    [string]$OutputDirectory,
    # Explicit opt-in: package installation can require elevation and must never
    # happen merely because somebody ran a normal build.
    [switch]$Bootstrap,
    # The normal path tests the Rust converter before packaging. This is useful
    # for a faster iteration after a test run has already passed.
    [switch]$SkipTests,
    # Never starts the GUI. This only skips the synthetic CLI write/rollback
    # package smoke when a caller is intentionally iterating on layout.
    [switch]$SkipTransactionSmoke
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$artifactsRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "artifacts"))
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot "artifacts\mh3g-save-convert-windows-x64"
}
$stage = [System.IO.Path]::GetFullPath($OutputDirectory)
$archive = Join-Path $artifactsRoot "mh3g-save-convert-windows-x64.zip"
$archiveChecksum = "$archive.sha256"
$transcript = Join-Path $artifactsRoot "mh3g-save-convert-windows-build-transcript.txt"
$targetTriple = "x86_64-pc-windows-msvc"
$project = Join-Path $repoRoot "apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj"
$launcher = Join-Path $repoRoot "scripts\mh3g-windows-launcher.ps1"
$packageReadme = Join-Path $repoRoot "packaging\mh3g-save-convert\README-Windows.txt"
$uiReadme = Join-Path $repoRoot "apps\mh3g-save-converter-windows\README.md"
$uiChineseReadme = Join-Path $repoRoot "apps\mh3g-save-converter-windows\README.zh-CN.md"
$transcriptStarted = $false
$script:BootstrapRestartRequired = $false

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Description,
        [int[]]$AcceptedExitCodes = @(0)
    )

    Write-Host (">> {0} {1}" -f $FilePath, ($Arguments -join " "))
    & $FilePath @Arguments | Out-Host
    $exitCode = $LASTEXITCODE
    if ($AcceptedExitCodes -notcontains $exitCode) {
        throw "$Description failed with exit code $exitCode. See $transcript for the first compiler error."
    }
    return $exitCode
}

function Get-ExternalCommand {
    param([Parameter(Mandatory = $true)][string]$Name)
    $command = Get-Command -Name $Name -CommandType Application -ErrorAction SilentlyContinue
    if ($null -eq $command -or [string]::IsNullOrWhiteSpace($command.Path)) {
        return $null
    }
    return $command.Path
}

function Get-VsWherePath {
    $candidates = @()
    $programFilesX86 = [Environment]::GetEnvironmentVariable("ProgramFiles(x86)", "Process")
    $programFiles = [Environment]::GetEnvironmentVariable("ProgramFiles", "Process")
    if (-not [string]::IsNullOrWhiteSpace($programFilesX86)) {
        $candidates += Join-Path $programFilesX86 "Microsoft Visual Studio\Installer\vswhere.exe"
    }
    if (-not [string]::IsNullOrWhiteSpace($programFiles)) {
        $candidates += Join-Path $programFiles "Microsoft Visual Studio\Installer\vswhere.exe"
    }
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }

    $command = Get-ExternalCommand "vswhere.exe"
    if ($null -ne $command) {
        return $command
    }
    return $null
}

function Get-VisualStudioInstallation {
    $vswhere = Get-VsWherePath
    if ($null -eq $vswhere) {
        return $null
    }

    $installation = @(& $vswhere -latest -products * -requires "Microsoft.VisualStudio.Component.VC.Tools.x86.x64" -property installationPath | Select-Object -First 1)
    if ($LASTEXITCODE -ne 0 -or $installation.Count -eq 0) {
        return $null
    }

    $path = $installation[0].ToString().Trim()
    if ([string]::IsNullOrWhiteSpace($path) -or -not (Test-Path -LiteralPath $path -PathType Container)) {
        return $null
    }
    return $path
}

function Get-AnyVisualStudioInstallation {
    $vswhere = Get-VsWherePath
    if ($null -eq $vswhere) {
        return $null
    }

    $installation = @(& $vswhere -latest -products * -property installationPath | Select-Object -First 1)
    if ($LASTEXITCODE -ne 0 -or $installation.Count -eq 0) {
        return $null
    }

    $path = $installation[0].ToString().Trim()
    if ([string]::IsNullOrWhiteSpace($path) -or -not (Test-Path -LiteralPath $path -PathType Container)) {
        return $null
    }
    return $path
}

function Get-VisualStudioInstallerPath {
    $vswhere = Get-VsWherePath
    if ($null -eq $vswhere) {
        return $null
    }
    $setup = Join-Path (Split-Path -Parent $vswhere) "setup.exe"
    if (Test-Path -LiteralPath $setup -PathType Leaf) {
        return $setup
    }
    return $null
}

function Test-DotnetEightSdk {
    $dotnet = Get-ExternalCommand "dotnet.exe"
    if ($null -eq $dotnet) {
        $dotnet = Get-ExternalCommand "dotnet"
    }
    if ($null -eq $dotnet) {
        return $false
    }

    $sdks = @(& $dotnet --list-sdks)
    if ($LASTEXITCODE -ne 0) {
        return $false
    }
    return [bool]($sdks | Where-Object { $_ -match '^\s*8\.' })
}

function Test-WindowsSdk {
    $roots = @(
        [Environment]::GetEnvironmentVariable("ProgramFiles(x86)", "Process"),
        [Environment]::GetEnvironmentVariable("ProgramFiles", "Process")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    foreach ($root in $roots) {
        if (Test-Path -LiteralPath (Join-Path $root "Windows Kits\10\Lib") -PathType Container) {
            return $true
        }
    }
    return $false
}

function Test-RustMinimumVersion {
    $rustc = Get-ExternalCommand "rustc.exe"
    if ($null -eq $rustc) {
        $rustc = Get-ExternalCommand "rustc"
    }
    if ($null -eq $rustc) {
        return $false
    }

    $versionLine = @(& $rustc --version | Select-Object -First 1)
    if ($LASTEXITCODE -ne 0 -or $versionLine.Count -eq 0 -or $versionLine[0] -notmatch '^rustc\s+(?<version>\d+\.\d+\.\d+)') {
        return $false
    }
    try {
        $version = [Version]($Matches["version"])
        return $version -ge ([Version]"1.95.0")
    } catch {
        return $false
    }
}

function Get-MissingPrerequisites {
    $missing = @()
    if (-not (Test-DotnetEightSdk)) {
        $missing += ".NET 8 SDK"
    }
    if ($null -eq (Get-ExternalCommand "cargo.exe") -and $null -eq (Get-ExternalCommand "cargo")) {
        $missing += "Rust cargo"
    }
    if ($null -eq (Get-ExternalCommand "rustup.exe") -and $null -eq (Get-ExternalCommand "rustup")) {
        $missing += "Rust rustup"
    }
    if (-not (Test-RustMinimumVersion)) {
        $missing += "Rust 1.95.0 or newer"
    }
    if ($null -eq (Get-VisualStudioInstallation)) {
        $missing += "Visual Studio 2022 C++ Build Tools with Windows SDK"
    }
    if (-not (Test-WindowsSdk)) {
        $missing += "Windows 10/11 SDK"
    }
    return $missing
}

function Refresh-ProcessPath {
    $allSegments = @(
        [Environment]::GetEnvironmentVariable("Path", "Machine"),
        [Environment]::GetEnvironmentVariable("Path", "User"),
        $env:Path
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

    $env:Path = (($allSegments -join ";").Split(";") |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        Select-Object -Unique) -join ";"
}

function Install-WithWinget {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [string]$Override,
        [int[]]$AcceptedExitCodes = @(0)
    )

    $winget = Get-ExternalCommand "winget.exe"
    if ($null -eq $winget) {
        $winget = Get-ExternalCommand "winget"
    }
    if ($null -eq $winget) {
        throw "-Bootstrap needs winget (App Installer). Install the missing prerequisite manually, then rerun this script."
    }

    $arguments = @(
        "install", "--id", $Id, "--exact", "--source", "winget",
        "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"
    )
    if (-not [string]::IsNullOrWhiteSpace($Override)) {
        $arguments += @("--override", $Override)
    }
    return Invoke-External -FilePath $winget -Arguments $arguments -Description "winget install $Id" -AcceptedExitCodes $AcceptedExitCodes
}

function Install-VisualStudioBuildComponents {
    $components = @(
        "--add", "Microsoft.VisualStudio.Workload.VCTools",
        "--add", "Microsoft.VisualStudio.Component.Windows10SDK.19041",
        "--includeRecommended", "--passive"
    )
    $existingInstallation = Get-AnyVisualStudioInstallation
    if ($null -ne $existingInstallation) {
        $setup = Get-VisualStudioInstallerPath
        if ($null -eq $setup) {
            throw "A Visual Studio instance exists at $existingInstallation, but setup.exe was not found. Use Visual Studio Installer to add VC Tools and a Windows SDK, then rerun this script."
        }
        $exitCode = Invoke-External -FilePath $setup -Arguments (@("modify", "--installPath", $existingInstallation) + $components) -Description "Visual Studio component modify" -AcceptedExitCodes @(0, 3010, 1641)
    } else {
        $override = "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows10SDK.19041 --includeRecommended"
        $exitCode = Install-WithWinget -Id "Microsoft.VisualStudio.2022.BuildTools" -Override $override -AcceptedExitCodes @(0, 3010, 1641)
    }
    if ($exitCode -eq 3010 -or $exitCode -eq 1641) {
        $script:BootstrapRestartRequired = $true
    }
}

function Install-MissingPrerequisites {
    param([Parameter(Mandatory = $true)][string[]]$Missing)

    if ($Missing -contains ".NET 8 SDK") {
        Install-WithWinget -Id "Microsoft.DotNet.SDK.8"
    }
    if ($Missing -contains "Rust cargo" -or $Missing -contains "Rust rustup" -or $Missing -contains "Rust 1.95.0 or newer") {
        Install-WithWinget -Id "Rustlang.Rustup"
        Refresh-ProcessPath
        $rustup = Get-ExternalCommand "rustup.exe"
        if ($null -eq $rustup) {
            $rustup = Get-ExternalCommand "rustup"
        }
        if ($null -ne $rustup -and $Missing -contains "Rust 1.95.0 or newer") {
            Invoke-External -FilePath $rustup -Arguments @("update", "stable") -Description "Rust stable update"
        }
    }
    if ($Missing -contains "Visual Studio 2022 C++ Build Tools with Windows SDK" -or $Missing -contains "Windows 10/11 SDK") {
        Install-VisualStudioBuildComponents
    }
    Refresh-ProcessPath
}

function Initialize-MsvcBuildEnvironment {
    $installation = Get-VisualStudioInstallation
    if ($null -eq $installation) {
        throw "MSVC x64 Build Tools were not found. Install Microsoft.VisualStudio.Component.VC.Tools.x86.x64 and a Windows 10/11 SDK."
    }

    $developerCommand = Join-Path $installation "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path -LiteralPath $developerCommand -PathType Leaf)) {
        throw "VsDevCmd.bat is missing from $installation. Repair the Visual Studio Build Tools installation."
    }

    $cmd = [Environment]::GetEnvironmentVariable("ComSpec", "Process")
    if ([string]::IsNullOrWhiteSpace($cmd)) {
        $systemRoot = [Environment]::GetEnvironmentVariable("SystemRoot", "Process")
        if ([string]::IsNullOrWhiteSpace($systemRoot)) {
            throw "ComSpec and SystemRoot are both unavailable; cannot load the MSVC build environment."
        }
        $cmd = Join-Path $systemRoot "System32\cmd.exe"
    }
    $environmentLines = @(& $cmd /d /s /c "call `"$developerCommand`" -no_logo -arch=x64 -host_arch=x64 >nul && set")
    if ($LASTEXITCODE -ne 0) {
        throw "VsDevCmd.bat failed with exit code $LASTEXITCODE. See $transcript for its output."
    }

    foreach ($line in $environmentLines) {
        if ($line -match '^(?<key>[^=]+)=(?<value>.*)$') {
            Set-Item -Path ("Env:" + $Matches["key"]) -Value $Matches["value"]
        }
    }

    if ($null -eq (Get-ExternalCommand "link.exe")) {
        throw "link.exe is still unavailable after VsDevCmd.bat. Install the MSVC x64/x86 Build Tools component."
    }
    if ([string]::IsNullOrWhiteSpace($env:WindowsSdkDir)) {
        throw "WindowsSdkDir is missing after VsDevCmd.bat. Install a Windows 10/11 SDK through Visual Studio Build Tools."
    }
}

function Assert-StageWithinArtifacts {
    $rootWithSeparator = $artifactsRoot.TrimEnd([char]92, [char]47) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $stage.StartsWith($rootWithSeparator, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "-OutputDirectory must be below $artifactsRoot so the package script cannot erase an arbitrary directory."
    }

    # GetFullPath only normalizes lexical segments; it does not resolve Windows
    # junctions or symlinks. Remove-Item -Recurse must never traverse a
    # reparse point that redirects a permitted artifacts path to a different
    # directory. Check every existing component now and again immediately
    # before deleting the staging directory.
    $relativeStage = $stage.Substring($rootWithSeparator.Length)
    $pathChain = @($artifactsRoot)
    $current = $artifactsRoot
    foreach ($segment in ($relativeStage -split '[\\/]')) {
        if ([string]::IsNullOrWhiteSpace($segment)) {
            continue
        }
        $current = Join-Path $current $segment
        $pathChain += $current
    }
    foreach ($candidate in $pathChain) {
        if (-not (Test-Path -LiteralPath $candidate)) {
            continue
        }
        $item = Get-Item -LiteralPath $candidate -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "-OutputDirectory path contains a reparse point at $candidate. Refusing to delete through a junction or symlink."
        }
        if (-not $item.PSIsContainer) {
            throw "-OutputDirectory path component is not a directory: $candidate"
        }
    }
}

function Write-Sha256File {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string]$OutputPath,
        [Parameter(Mandatory = $true)][string]$DisplayName
    )

    $hash = (Get-FileHash -LiteralPath $FilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $DisplayName" | Set-Content -LiteralPath $OutputPath -NoNewline -Encoding ascii
    return $hash
}

function Test-PackagedLayout {
    param([Parameter(Mandatory = $true)][string]$ArchivePath)

    $verifyRoot = Join-Path $artifactsRoot ("mh3g-save-convert-windows-verify-" + [Guid]::NewGuid().ToString("N"))
    try {
        Expand-Archive -LiteralPath $ArchivePath -DestinationPath $verifyRoot -Force
        $packageRoot = Join-Path $verifyRoot "mh3g-save-convert-windows-x64"
        $gui = Join-Path $packageRoot "MH3GSaveConverter.exe"
        $sidecar = Join-Path $packageRoot "tools\mh3g-save-convert.exe"
        $sidecarChecksum = "$sidecar.sha256"
        $packageLauncher = Join-Path $packageRoot "Run-Converter.ps1"
        $packageReadmePath = Join-Path $packageRoot "README-Windows.txt"
        foreach ($required in @($gui, $sidecar, $sidecarChecksum, $packageLauncher, $packageReadmePath)) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                throw "Package self-check is missing $required"
            }
        }
        if ((Get-Item -LiteralPath $gui).Length -le 0) {
            throw "Package self-check found an empty WinUI executable."
        }

        $expectedSidecarHash = ((Get-Content -LiteralPath $sidecarChecksum -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
        $actualSidecarHash = (Get-FileHash -LiteralPath $sidecar -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualSidecarHash -ne $expectedSidecarHash) {
            throw "Package self-check found a sidecar checksum mismatch."
        }

        $powershell = Get-ExternalCommand "powershell.exe"
        if ($null -eq $powershell) {
            throw "Windows PowerShell powershell.exe is required for the packaged launcher self-check."
        }
        $markOfTheWebAttached = $false
        try {
            Set-Content -LiteralPath "${sidecar}:Zone.Identifier" -Encoding ascii -Value "[ZoneTransfer]`r`nZoneId=3"
            $markOfTheWebAttached = $null -ne (Get-Item -LiteralPath $sidecar -Stream Zone.Identifier -ErrorAction SilentlyContinue)
        } catch {
            # A package built on FAT/exFAT or a network share may not support
            # alternate data streams. The launcher still performs Unblock-File
            # for normal NTFS browser/chat downloads; only this injected test is
            # skipped when the filesystem cannot represent a Zone.Identifier.
            Write-Warning "Could not attach a synthetic Mark-of-the-Web stream; continuing the launcher self-check."
        }
        Invoke-External -FilePath $powershell -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $packageLauncher, "--help") -Description "packaged launcher --help"
        if ($markOfTheWebAttached -and (Get-Item -LiteralPath $sidecar -Stream Zone.Identifier -ErrorAction SilentlyContinue)) {
            throw "Packaged launcher did not remove the synthetic Mark-of-the-Web stream."
        }

        if ($SkipTransactionSmoke) {
            Write-Warning "Skipping synthetic transaction smoke by explicit -SkipTransactionSmoke."
            return
        }
        $activeEmulators = @(Get-Process -Name "Cemu", "Cemu_release", "Nemessix", "Azahar" -ErrorAction SilentlyContinue)
        if ($activeEmulators.Count -gt 0) {
            Write-Warning "Skipping synthetic transaction smoke because an emulator is running. No process was stopped and no real save was touched."
            return
        }

        $smokeRoot = Join-Path $verifyRoot "synthetic-transaction-smoke"
        $sourceDirectory = Join-Path $smokeRoot "source"
        $targetDirectory = Join-Path $smokeRoot "target"
        New-Item -ItemType Directory -Force $sourceDirectory, $targetDirectory | Out-Null
        $source = Join-Path $sourceDirectory "user2"
        $target = Join-Path $targetDirectory "user2"
        $sourceBytes = [byte[]]::new(0x8A00)
        $sourceBytes[0] = 0x2B
        [System.IO.File]::WriteAllBytes($source, $sourceBytes)
        $targetBytes = [byte[]]::new(0x8A24)
        $targetBytes[0] = 0xA5
        [System.IO.File]::WriteAllBytes($target, $targetBytes)
        $sourceHashBefore = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
        $targetHashBefore = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash

        Invoke-External -FilePath $sidecar -Arguments @("convert", $source, "--output", $target, "--write") -Description "synthetic converter write smoke"
        $manifest = Join-Path $targetDirectory ".user2.mh3g-install.json"
        $backups = @(Get-ChildItem -LiteralPath $targetDirectory -Filter ".user2.mh3g-backup-*" -ErrorAction SilentlyContinue)
        if (-not (Test-Path -LiteralPath $manifest -PathType Leaf) -or $backups.Count -ne 1) {
            throw "Synthetic write did not create exactly one manifest and one backup."
        }
        Invoke-External -FilePath $sidecar -Arguments @("rollback", "--manifest", $manifest) -Description "synthetic converter rollback smoke"
        $sourceHashAfter = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
        $targetHashAfter = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        if ($sourceHashAfter -ne $sourceHashBefore -or $targetHashAfter -ne $targetHashBefore) {
            throw "Synthetic write/rollback changed the source or failed to restore the target."
        }
        if (Test-Path -LiteralPath $manifest -PathType Leaf) {
            throw "Synthetic rollback left its manifest behind."
        }
        if (@(Get-ChildItem -LiteralPath $targetDirectory -Filter ".user2.mh3g-backup-*" -ErrorAction SilentlyContinue).Count -ne 0) {
            throw "Synthetic rollback left a backup behind."
        }
    } finally {
        Remove-Item -LiteralPath $verifyRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "This package script must run on Windows, not through a macOS/Linux cross-targeting build."
    }
    if (-not [Environment]::Is64BitOperatingSystem -or -not [Environment]::Is64BitProcess) {
        throw "Use 64-bit Windows PowerShell on a 64-bit Windows installation. The release is win-x64 only."
    }
    if (-not (Test-Path -LiteralPath $project -PathType Leaf)) {
        throw "WinUI project is missing: $project"
    }
    foreach ($required in @($launcher, $packageReadme, $uiReadme, $uiChineseReadme)) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Required packaging file is missing: $required"
        }
    }
    Assert-StageWithinArtifacts
    New-Item -ItemType Directory -Force $artifactsRoot | Out-Null
    Start-Transcript -LiteralPath $transcript -Force | Out-Null
    $transcriptStarted = $true

    $missing = @(Get-MissingPrerequisites)
    if ($missing.Count -gt 0) {
        if (-not $Bootstrap) {
            throw ("Missing Windows build prerequisites: {0}.`nRun the same command once with -Bootstrap, or install them manually and rerun. No cache was cleared." -f ($missing -join ", "))
        }
        Write-Host ("Bootstrapping missing prerequisites: {0}" -f ($missing -join ", "))
        Install-MissingPrerequisites -Missing $missing
        if ($script:BootstrapRestartRequired) {
            throw "A prerequisite installer returned 3010/1641 and requested a Windows restart. Restart Windows, then rerun this exact command."
        }
        $missing = @(Get-MissingPrerequisites)
        if ($missing.Count -gt 0) {
            throw ("Bootstrap finished but these prerequisites are still unavailable: {0}. Reopen 64-bit PowerShell if an installer changed PATH, then rerun this exact command." -f ($missing -join ", "))
        }
    }

    Initialize-MsvcBuildEnvironment
    $dotnet = Get-ExternalCommand "dotnet.exe"
    if ($null -eq $dotnet) { $dotnet = Get-ExternalCommand "dotnet" }
    $cargo = Get-ExternalCommand "cargo.exe"
    if ($null -eq $cargo) { $cargo = Get-ExternalCommand "cargo" }
    $rustup = Get-ExternalCommand "rustup.exe"
    if ($null -eq $rustup) { $rustup = Get-ExternalCommand "rustup" }
    $rustc = Get-ExternalCommand "rustc.exe"
    if ($null -eq $rustc) { $rustc = Get-ExternalCommand "rustc" }
    if ($null -eq $dotnet -or $null -eq $cargo -or $null -eq $rustup -or $null -eq $rustc) {
        throw "A required executable disappeared after preflight. Reopen 64-bit PowerShell and rerun with -Bootstrap if needed."
    }

    Write-Host "=== Toolchain ==="
    & $dotnet --version
    & $cargo --version
    & $rustc --version
    Invoke-External -FilePath $rustup -Arguments @("target", "add", $targetTriple) -Description "Rust target setup"

    Push-Location $repoRoot
    try {
        # Do not clear NuGet/Cargo/target caches: restore and rustup are naturally cache-aware.
        Invoke-External -FilePath $dotnet -Arguments @("restore", $project) -Description "dotnet restore"

        $previousRustFlags = [Environment]::GetEnvironmentVariable("RUSTFLAGS", "Process")
        if ([string]::IsNullOrWhiteSpace($previousRustFlags)) {
            $env:RUSTFLAGS = "-C target-feature=+crt-static"
        } elseif ($previousRustFlags -notmatch "target-feature=\+crt-static") {
            $env:RUSTFLAGS = "$previousRustFlags -C target-feature=+crt-static"
        }
        try {
            if (-not $SkipTests) {
                Invoke-External -FilePath $cargo -Arguments @("test", "--locked", "--target", $targetTriple, "-p", "mh3g-save-convert") -Description "cargo test --locked"
            } else {
                Write-Warning "Skipping cargo test by explicit -SkipTests."
            }
            Invoke-External -FilePath $cargo -Arguments @("build", "--locked", "--release", "--target", $targetTriple, "-p", "mh3g-save-convert", "--bin", "mh3g-save-convert") -Description "cargo build --locked --release"
        } finally {
            if ($null -eq $previousRustFlags) {
                Remove-Item -Path Env:RUSTFLAGS -ErrorAction SilentlyContinue
            } else {
                $env:RUSTFLAGS = $previousRustFlags
            }
        }

        $sidecar = Join-Path $repoRoot "target\$targetTriple\release\mh3g-save-convert.exe"
        if (-not (Test-Path -LiteralPath $sidecar -PathType Leaf)) {
            throw "Rust x64 sidecar is missing after cargo build: $sidecar"
        }
        Invoke-External -FilePath $sidecar -Arguments @("--help") -Description "release sidecar smoke"

        # Recheck directly before recursive deletion in case an existing stage
        # component was replaced by a junction/symlink after initial preflight.
        Assert-StageWithinArtifacts
        Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
        Invoke-External -FilePath $dotnet -Arguments @(
            "publish", $project, "-c", "Release", "-r", "win-x64", "--self-contained", "true", "--no-restore",
            "-p:Platform=x64", "-p:WindowsAppSDKSelfContained=true", "-o", $stage
        ) -Description "WinUI dotnet publish"

        $gui = Join-Path $stage "MH3GSaveConverter.exe"
        if (-not (Test-Path -LiteralPath $gui -PathType Leaf)) {
            throw "dotnet publish completed without MH3GSaveConverter.exe. See $transcript."
        }
        $toolsDirectory = Join-Path $stage "tools"
        New-Item -ItemType Directory -Force $toolsDirectory | Out-Null
        $packagedSidecar = Join-Path $toolsDirectory "mh3g-save-convert.exe"
        Copy-Item -LiteralPath $sidecar -Destination $packagedSidecar -Force
        $sidecarHash = Write-Sha256File -FilePath $packagedSidecar -OutputPath "$packagedSidecar.sha256" -DisplayName "mh3g-save-convert.exe"
        "$sidecarHash  mh3g-save-convert.exe" | Set-Content -LiteralPath (Join-Path $artifactsRoot "mh3g-save-convert.exe.sha256") -NoNewline -Encoding ascii
        Copy-Item -LiteralPath $launcher -Destination (Join-Path $stage "Run-Converter.ps1") -Force
        Copy-Item -LiteralPath $uiReadme -Destination (Join-Path $stage "README-Windows-UI.md") -Force
        Copy-Item -LiteralPath $uiChineseReadme -Destination (Join-Path $stage "README-Windows-UI.zh-CN.md") -Force
        Copy-Item -LiteralPath $packageReadme -Destination (Join-Path $stage "README-Windows.txt") -Force

        Remove-Item -LiteralPath $archive, $archiveChecksum -Force -ErrorAction SilentlyContinue
        Compress-Archive -LiteralPath $stage -DestinationPath $archive -Force
        Write-Sha256File -FilePath $archive -OutputPath $archiveChecksum -DisplayName "mh3g-save-convert-windows-x64.zip" | Out-Null
        Test-PackagedLayout -ArchivePath $archive
    } finally {
        Pop-Location
    }

    Write-Host ""
    Write-Host "Windows x64 package complete."
    Write-Host "ZIP: $archive"
    Write-Host "SHA-256: $archiveChecksum"
    Write-Host "Transcript: $transcript"
} catch {
    Write-Error ("Windows package failed: {0}" -f $_.Exception.Message)
    throw
} finally {
    if ($transcriptStarted) {
        Stop-Transcript | Out-Null
    }
}
