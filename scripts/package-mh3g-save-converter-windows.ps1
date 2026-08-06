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
    [switch]$SkipTransactionSmoke,
    # CI/main validation mode: build and publish the WinUI folder plus native
    # sidecar, but do not create distributable ZIP/portable/installer outputs.
    # Release artifacts are produced only by the tag-driven release workflow.
    [switch]$ValidateOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$artifactsRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "artifacts"))
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot "artifacts\mh3g-save-convert-windows-x64"
}
$stage = [System.IO.Path]::GetFullPath($OutputDirectory)
$packageDirectoryName = [System.IO.Path]::GetFileName($stage.TrimEnd([char]92, [char]47))
if ([string]::IsNullOrWhiteSpace($packageDirectoryName)) {
    throw "-OutputDirectory must name a package directory below $artifactsRoot."
}
$archive = Join-Path $artifactsRoot "mh3g-save-convert-windows-x64.zip"
$archiveChecksum = "$archive.sha256"
$portableStage = Join-Path $artifactsRoot "mh3g-save-convert-windows-portable-stage"
$portableExecutable = Join-Path $artifactsRoot "MH3GSaveConverter-Portable-x64.exe"
$portableChecksum = "$portableExecutable.sha256"
$installerExecutable = Join-Path $artifactsRoot "MH3GSaveConverter-Setup-x64.exe"
$installerChecksum = "$installerExecutable.sha256"
$transcript = Join-Path $artifactsRoot "mh3g-save-convert-windows-build-transcript.txt"
$targetTriple = "x86_64-pc-windows-msvc"
$project = Join-Path $repoRoot "apps\mh3g-save-converter-windows\MH3GSaveConverter.Windows.csproj"
$launcher = Join-Path $repoRoot "scripts\mh3g-windows-launcher.ps1"
$packageReadme = Join-Path $repoRoot "packaging\mh3g-save-convert\README-Windows.txt"
$installerScript = Join-Path $repoRoot "packaging\mh3g-save-convert\MH3GSaveConverter.iss"
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
        throw "$Description failed with exit code $exitCode. See $transcript for the first failed command."
    }
    return $exitCode
}

function Initialize-ConsoleUtf8Encoding {
    # Windows PowerShell 5.1 otherwise decodes some UTF-8 native output (such
    # as winget on a Chinese Windows install) through the legacy ANSI code
    # page, which makes the transcript unreadable even though the command ran.
    try {
        $utf8 = [System.Text.UTF8Encoding]::new($false)
        [Console]::InputEncoding = $utf8
        [Console]::OutputEncoding = $utf8
        $script:OutputEncoding = $utf8
    } catch {
        Write-Warning "Could not select UTF-8 console encoding: $($_.Exception.Message)"
    }
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

function Get-DotnetEightSdkCommand {
    # A previous version of this package script installed a private SDK with
    # dotnet-install -NoPath. Reuse that valid per-user payload before asking
    # WinGet to download another SDK. Probe the normal PATH first so a managed
    # system SDK still takes precedence.
    $candidates = @()
    $pathDotnet = Get-ExternalCommand "dotnet.exe"
    if ($null -eq $pathDotnet) {
        $pathDotnet = Get-ExternalCommand "dotnet"
    }
    if ($null -ne $pathDotnet) {
        $candidates += $pathDotnet
    }

    $localAppDataRoots = @(
        [Environment]::GetEnvironmentVariable("LOCALAPPDATA", "Process"),
        [Environment]::GetEnvironmentVariable("LOCALAPPDATA", "User"),
        [Environment]::GetEnvironmentVariable("LOCALAPPDATA", "Machine"),
        [Environment]::GetFolderPath([System.Environment+SpecialFolder]::LocalApplicationData)
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim()) } |
        Select-Object -Unique
    foreach ($localAppData in $localAppDataRoots) {
        $candidates += Join-Path $localAppData "MH3GSaveConverter\BuildTools\dotnet8\dotnet.exe"
    }

    foreach ($candidate in @($candidates | Select-Object -Unique)) {
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            continue
        }
        $sdks = @(& $candidate --list-sdks)
        if ($LASTEXITCODE -eq 0 -and [bool]($sdks | Where-Object { $_ -match '^\s*8\.' })) {
            return $candidate
        }
    }
    return $null
}

function Test-DotnetEightSdk {
    return $null -ne (Get-DotnetEightSdkCommand)
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
    $rustc = Get-RustToolCommand "rustc"
    if ($null -eq $rustc) {
        return $false
    }

    # rustc --version emits one line. Do not pipe it through Select-Object
    # because Windows PowerShell can stop an upstream native process early and
    # overwrite its successful exit code with -1.
    $versionLine = @(& $rustc --version)
    $rustcExitCode = $LASTEXITCODE
    if ($rustcExitCode -ne 0 -or $versionLine.Count -eq 0) {
        return $false
    }
    if ($versionLine[0] -match '^rustc\s+(?<version>\d+\.\d+\.\d+)') {
        $versionText = $Matches["version"]
    } else {
        return $false
    }
    try {
        $version = [Version]$versionText
        return $version -ge ([Version]"1.95.0")
    } catch {
        return $false
    }
}

function Get-InnoSetupCompiler {
    # Inno Setup is used only to wrap the already self-contained folder into a
    # conventional per-user installer. The portable EXE does not depend on it.
    $candidates = @()
    foreach ($root in @(
        [Environment]::GetEnvironmentVariable("ProgramFiles(x86)", "Process"),
        [Environment]::GetEnvironmentVariable("ProgramFiles", "Process")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) {
        $candidates += Join-Path $root "Inno Setup 6\ISCC.exe"
    }
    foreach ($candidate in @($candidates | Select-Object -Unique)) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }

    $command = Get-ExternalCommand "ISCC.exe"
    if ($null -ne $command) {
        return $command
    }
    return $null
}

function Test-InnoSetup {
    return $null -ne (Get-InnoSetupCompiler)
}

function Get-ConverterVersion {
    $manifest = Join-Path $repoRoot "crates\mh3g-save-convert\Cargo.toml"
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        throw "Converter Cargo manifest is missing: $manifest"
    }
    $versionMatch = [regex]::Match((Get-Content -LiteralPath $manifest -Raw), '(?m)^version\s*=\s*"(?<version>[^"\r\n]+)"\s*$')
    if (-not $versionMatch.Success) {
        throw "Could not resolve the mh3g-save-convert package version from $manifest"
    }
    return $versionMatch.Groups["version"].Value
}

function Get-MissingPrerequisites {
    $missing = @()
    if (-not (Test-DotnetEightSdk)) {
        $missing += ".NET 8 SDK"
    }
    if ($null -eq (Get-RustToolCommand "cargo")) {
        $missing += "Rust cargo"
    }
    if ($null -eq (Get-RustToolCommand "rustup")) {
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
    if (-not (Test-InnoSetup)) {
        $missing += "Inno Setup 6"
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
        ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim()) } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        Select-Object -Unique) -join ";"
}

function Get-CargoHomeCandidates {
    # Rustup normally uses %USERPROFILE%\.cargo\bin. A caller can override
    # that with CARGO_HOME at process, user, or machine scope. Keep the probe
    # order stable because it is also the location used by the safe fallback
    # installer below.
    $cargoHomes = @(
        @(
            [Environment]::GetEnvironmentVariable("CARGO_HOME", "Process"),
            [Environment]::GetEnvironmentVariable("CARGO_HOME", "User"),
            [Environment]::GetEnvironmentVariable("CARGO_HOME", "Machine")
        ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim()) }
    )

    $userProfile = [Environment]::GetEnvironmentVariable("USERPROFILE", "Process")
    if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
        $cargoHomes += Join-Path $userProfile ".cargo"
    }
    if (-not [string]::IsNullOrWhiteSpace($HOME)) {
        $cargoHomes += Join-Path $HOME ".cargo"
    }

    return @($cargoHomes | Select-Object -Unique)
}

function Get-RustupHomeCandidates {
    $rustupHomes = @(
        @(
            [Environment]::GetEnvironmentVariable("RUSTUP_HOME", "Process"),
            [Environment]::GetEnvironmentVariable("RUSTUP_HOME", "User"),
            [Environment]::GetEnvironmentVariable("RUSTUP_HOME", "Machine")
        ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim()) }
    )

    $userProfile = [Environment]::GetEnvironmentVariable("USERPROFILE", "Process")
    if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
        $rustupHomes += Join-Path $userProfile ".rustup"
    }
    if (-not [string]::IsNullOrWhiteSpace($HOME)) {
        $rustupHomes += Join-Path $HOME ".rustup"
    }

    return @($rustupHomes | Select-Object -Unique)
}

function Get-RustupProbePaths {
    $probes = @()
    foreach ($cargoHome in @(Get-CargoHomeCandidates)) {
        foreach ($proxy in @("rustup.exe", "cargo.exe", "rustc.exe")) {
            $probes += Join-Path (Join-Path $cargoHome "bin") $proxy
        }
    }
    return @($probes | Select-Object -Unique)
}

function Get-RustupRecoveryHomes {
    $cargoHomes = @(Get-CargoHomeCandidates)
    $rustupHomes = @(Get-RustupHomeCandidates)
    if ($cargoHomes.Count -eq 0 -or $rustupHomes.Count -eq 0) {
        throw "Could not resolve a CARGO_HOME/RUSTUP_HOME for Rustup recovery."
    }

    # CARGO_HOME and RUSTUP_HOME are independent Rustup settings. Respect the
    # same process → user → machine → default precedence used by Rustup rather
    # than silently choosing an older proxy from a different home.
    return [PSCustomObject]@{
        CargoHome = $cargoHomes[0]
        RustupHome = $rustupHomes[0]
    }
}

function Set-RustupHomesForCurrentProcess {
    # This only affects the package script's PowerShell process. It lets a
    # shell launched before a user-level CARGO_HOME/RUSTUP_HOME change execute
    # the proxies from the same homes that Add-RustupBinToProcessPath probes.
    $homes = Get-RustupRecoveryHomes
    $env:CARGO_HOME = $homes.CargoHome
    $env:RUSTUP_HOME = $homes.RustupHome
    return $homes
}

function Add-RustupBinToProcessPath {
    # Qoder and a PowerShell started before Rustup was installed can omit the
    # current user's bin directory from inherited PATH. Wrap candidate results
    # in @() because Windows PowerShell turns a one-item pipeline into a scalar
    # String, and subsequent += would concatenate instead of append.
    $cargoHomes = @(Get-CargoHomeCandidates)

    $pathSegments = @($env:Path -split ";") |
        ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim()) } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    foreach ($cargoHome in $cargoHomes | Select-Object -Unique) {
        $bin = Join-Path $cargoHome "bin"
        if (-not (Test-Path -LiteralPath $bin -PathType Container)) {
            continue
        }
        $hasRustTool = @("cargo.exe", "rustup.exe", "rustc.exe") |
            Where-Object { Test-Path -LiteralPath (Join-Path $bin $_) -PathType Leaf } |
            Select-Object -First 1
        if ($null -eq $hasRustTool) {
            continue
        }
        if ($pathSegments -notcontains $bin) {
            $pathSegments = @($bin) + $pathSegments
        }
    }
    $env:Path = $pathSegments -join ";"
}

function Get-RustToolCommand {
    param([Parameter(Mandatory = $true)][string]$Name)

    Add-RustupBinToProcessPath
    $command = Get-ExternalCommand "$Name.exe"
    if ($null -eq $command) {
        $command = Get-ExternalCommand $Name
    }
    return $command
}

function Install-WithWinget {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [string]$Override,
        [switch]$ForceReinstall,
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
    if ($ForceReinstall) {
        # `--force` asks WinGet to execute the package installer even when its
        # installed-package registry says the current version is present.
        $arguments += "--force"
    }
    return Invoke-External -FilePath $winget -Arguments $arguments -Description "winget install $Id" -AcceptedExitCodes $AcceptedExitCodes
}

function Repair-RustupWithWinget {
    $winget = Get-ExternalCommand "winget.exe"
    if ($null -eq $winget) {
        $winget = Get-ExternalCommand "winget"
    }
    if ($null -eq $winget) {
        Write-Warning "winget is unavailable, so Rustup repair will use the official HTTPS fallback with SHA-256 sidecar integrity check if needed."
        return $false
    }

    $arguments = @(
        "repair", "--id", "Rustlang.Rustup", "--exact", "--source", "winget", "--force", "--silent",
        "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"
    )
    try {
        Invoke-External -FilePath $winget -Arguments $arguments -Description "winget repair Rustlang.Rustup" | Out-Null
        return $true
    } catch {
        # Older App Installer versions or a manifest without a repair action
        # can reject this verb. The next recovery rung is intentionally tried.
        Write-Warning ("winget repair Rustlang.Rustup did not restore the current user payload: {0}" -f $_.Exception.Message)
        return $false
    }
}

function Install-RustupFromOfficialSource {
    # Final recovery rung for a stale WinGet registration. Rustup publishes the
    # Windows MSVC bootstrap executable and a SHA-256 sidecar at official HTTPS
    # static URLs. The sidecar catches corrupted downloads; it is not a second
    # independent source of provenance. Do not use this path until both WinGet
    # repair paths failed.
    $target = "x86_64-pc-windows-msvc"
    $installerUrl = "https://static.rust-lang.org/rustup/dist/$target/rustup-init.exe"
    $checksumUrl = "$installerUrl.sha256"
    $temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("mh3g-save-convert-rustup-" + [Guid]::NewGuid().ToString("N"))
    $installer = Join-Path $temporaryRoot "rustup-init.exe"
    $checksum = Join-Path $temporaryRoot "rustup-init.exe.sha256"

    $recoveryHomes = Get-RustupRecoveryHomes

    $hadCargoHome = Test-Path -LiteralPath "Env:CARGO_HOME"
    $hadRustupHome = Test-Path -LiteralPath "Env:RUSTUP_HOME"
    $previousCargoHome = $env:CARGO_HOME
    $previousRustupHome = $env:RUSTUP_HOME
    try {
        New-Item -ItemType Directory -Force $temporaryRoot | Out-Null
        Write-Host "WinGet did not restore Rustup; downloading the official HTTPS Rustup bootstrapper with its SHA-256 sidecar."
        $webClient = New-Object System.Net.WebClient
        try {
            $webClient.DownloadFile($installerUrl, $installer)
            $webClient.DownloadFile($checksumUrl, $checksum)
        } finally {
            $webClient.Dispose()
        }

        if (-not (Test-Path -LiteralPath $installer -PathType Leaf) -or -not (Test-Path -LiteralPath $checksum -PathType Leaf)) {
            throw "Official Rustup download did not create both installer and SHA-256 files."
        }
        $expectedHash = ((Get-Content -LiteralPath $checksum -Raw).Trim() -split '\s+')[0]
        if ($expectedHash -notmatch '^[0-9a-fA-F]{64}$') {
            throw "Official Rustup SHA-256 sidecar has an unexpected format."
        }
        $actualHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
        if (-not [string]::Equals($actualHash, $expectedHash, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Official Rustup SHA-256 verification failed."
        }

        # Rebuild rustup/cargo/rustc proxies in the same homes without deleting
        # cached toolchains, selecting a default toolchain, or changing PATH.
        $env:CARGO_HOME = $recoveryHomes.CargoHome
        $env:RUSTUP_HOME = $recoveryHomes.RustupHome
        Invoke-External -FilePath $installer -Arguments @("-y", "--no-update-default-toolchain", "--no-modify-path") -Description "official Rustup bootstrap" | Out-Null
    } finally {
        if ($hadCargoHome) {
            $env:CARGO_HOME = $previousCargoHome
        } else {
            Remove-Item -LiteralPath "Env:CARGO_HOME" -ErrorAction SilentlyContinue
        }
        if ($hadRustupHome) {
            $env:RUSTUP_HOME = $previousRustupHome
        } else {
            Remove-Item -LiteralPath "Env:RUSTUP_HOME" -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
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

    Set-RustupHomesForCurrentProcess | Out-Null
    if ($Missing -contains ".NET 8 SDK") {
        Install-WithWinget -Id "Microsoft.DotNet.SDK.8"
    }
    if ($Missing -contains "Rust cargo" -or $Missing -contains "Rust rustup" -or $Missing -contains "Rust 1.95.0 or newer") {
        Refresh-ProcessPath
        Add-RustupBinToProcessPath
        $rustup = Get-RustToolCommand "rustup"
        $cargo = Get-RustToolCommand "cargo"
        $rustc = Get-RustToolCommand "rustc"
        if ($null -eq $rustup) {
            # 0x8A15002B (-1978335189) means winget recognizes an installed
            # package but has no applicable update. It is only non-fatal here;
            # the executable must still be found by the recheck below.
            try {
                Install-WithWinget -Id "Rustlang.Rustup" -AcceptedExitCodes @(0, -1978335189) | Out-Null
            } catch {
                Write-Warning ("winget install Rustlang.Rustup did not restore the current user payload: {0}" -f $_.Exception.Message)
            }
            Refresh-ProcessPath
            Add-RustupBinToProcessPath
            $rustup = Get-RustToolCommand "rustup"
            $cargo = Get-RustToolCommand "cargo"
            $rustc = Get-RustToolCommand "rustc"
        }
        if ($null -eq $rustup -or $null -eq $cargo -or $null -eq $rustc) {
            # First prefer the package manager's own in-place repair. It keeps
            # .cargo/.rustup and any downloaded toolchains intact.
            Repair-RustupWithWinget | Out-Null
            Refresh-ProcessPath
            Add-RustupBinToProcessPath
            $rustup = Get-RustToolCommand "rustup"
            $cargo = Get-RustToolCommand "cargo"
            $rustc = Get-RustToolCommand "rustc"
        }
        if ($null -eq $rustup -or $null -eq $cargo -or $null -eq $rustc) {
            # A stale installed-package record can cause ordinary `install` to
            # stop at 0x8A15002B. Force only this known Rustup package to run
            # its installer again; do not uninstall or remove existing homes.
            try {
                Install-WithWinget -Id "Rustlang.Rustup" -ForceReinstall -Override "-y --no-update-default-toolchain --no-modify-path" | Out-Null
            } catch {
                Write-Warning ("winget force reinstall Rustlang.Rustup did not restore the current user payload: {0}" -f $_.Exception.Message)
            }
            Refresh-ProcessPath
            Add-RustupBinToProcessPath
            $rustup = Get-RustToolCommand "rustup"
            $cargo = Get-RustToolCommand "cargo"
            $rustc = Get-RustToolCommand "rustc"
        }
        if ($null -eq $rustup -or $null -eq $cargo -or $null -eq $rustc) {
            try {
                Install-RustupFromOfficialSource
            } catch {
                Write-Warning ("official HTTPS Rustup recovery with SHA-256 sidecar integrity check did not restore the current user payload: {0}" -f $_.Exception.Message)
            }
            Refresh-ProcessPath
            Add-RustupBinToProcessPath
            $rustup = Get-RustToolCommand "rustup"
            $cargo = Get-RustToolCommand "cargo"
            $rustc = Get-RustToolCommand "rustc"
        }
        if ($null -eq $rustup -or $null -eq $cargo -or $null -eq $rustc) {
            $probePaths = @(Get-RustupProbePaths)
            $probeSummary = if ($probePaths.Count -gt 0) { $probePaths -join "; " } else { "(CARGO_HOME and USERPROFILE could not be resolved)" }
            $missingProxies = @()
            if ($null -eq $rustup) { $missingProxies += "rustup.exe" }
            if ($null -eq $cargo) { $missingProxies += "cargo.exe" }
            if ($null -eq $rustc) { $missingProxies += "rustc.exe" }
            throw "Rustup recovery failed: WinGet reports Rustlang.Rustup as installed, but this user's Rustup proxy payload is incomplete ($($missingProxies -join ', ')) after normal install, repair, force reinstall, and official HTTPS recovery with SHA-256 sidecar integrity check. Probed proxy paths: $probeSummary"
        }
        if ($Missing -contains "Rust cargo" -or $Missing -contains "Rust 1.95.0 or newer") {
            # Build against the exact host needed by this package without
            # changing the tester's persistent Rust default toolchain.
            $buildToolchain = "stable-x86_64-pc-windows-msvc"
            Invoke-External -FilePath $rustup -Arguments @("toolchain", "install", $buildToolchain, "--profile", "minimal") -Description "Rust MSVC x64 toolchain setup" | Out-Null
            $env:RUSTUP_TOOLCHAIN = $buildToolchain
        }
    }
    if ($Missing -contains "Visual Studio 2022 C++ Build Tools with Windows SDK" -or $Missing -contains "Windows 10/11 SDK") {
        Install-VisualStudioBuildComponents
    }
    if ($Missing -contains "Inno Setup 6") {
        Install-WithWinget -Id "JRSoftware.InnoSetup"
    }
    Refresh-ProcessPath
    Set-RustupHomesForCurrentProcess | Out-Null
    Add-RustupBinToProcessPath
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
    param([string]$CandidatePath = $stage)

    $fullCandidatePath = [System.IO.Path]::GetFullPath($CandidatePath)
    $rootWithSeparator = $artifactsRoot.TrimEnd([char]92, [char]47) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullCandidatePath.StartsWith($rootWithSeparator, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "A packaging stage must be below $artifactsRoot so the package script cannot erase an arbitrary directory."
    }

    # GetFullPath only normalizes lexical segments; it does not resolve Windows
    # junctions or symlinks. Remove-Item -Recurse must never traverse a
    # reparse point that redirects a permitted artifacts path to a different
    # directory. Check every existing component now and again immediately
    # before deleting the staging directory.
    $relativeStage = $fullCandidatePath.Substring($rootWithSeparator.Length)
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
            throw "Packaging stage path contains a reparse point at $candidate. Refusing to delete through a junction or symlink."
        }
        if (-not $item.PSIsContainer) {
            throw "Packaging stage path component is not a directory: $candidate"
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

function Assert-NativeConverterSidecar {
    param([Parameter(Mandatory = $true)][string]$FilePath)

    $legacyMarker = "mh3g-save-convert-core.exe"
    $binaryText = [System.Text.Encoding]::ASCII.GetString(
        [System.IO.File]::ReadAllBytes($FilePath)
    )
    if ($binaryText.Contains($legacyMarker)) {
        throw "Refusing the legacy MH3G compatibility wrapper: $FilePath. Build and package the native Rust converter from 0.0.4 or newer."
    }
}

function Get-RunningSupportedEmulators {
    # The Rust integration suite intentionally exercises guarded writes against
    # synthetic files. Its process probe must see no real emulator, just as a
    # real conversion must. Never terminate a user process from a build script.
    return @(Get-Process -Name "Cemu", "Cemu_release", "Nemessix", "Azahar" -ErrorAction SilentlyContinue)
}

function Assert-EmulatorsStoppedForRustTests {
    $activeEmulators = @(Get-RunningSupportedEmulators)
    if ($activeEmulators.Count -eq 0) {
        return
    }
    $names = @($activeEmulators | ForEach-Object { $_.ProcessName } | Sort-Object -Unique)
    throw ("Close these emulator processes before the mandatory Rust test suite: {0}. No process was stopped; after they exit, rerun this exact package command. Use -SkipTests only for an isolated publish/ZIP diagnosis, never for a final distribution build." -f ($names -join ", "))
}

function Publish-PortableExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$Dotnet,
        [Parameter(Mandatory = $true)][string]$ProjectPath,
        [Parameter(Mandatory = $true)][string]$NativeSidecar,
        [Parameter(Mandatory = $true)][string]$PortableStageDirectory,
        [Parameter(Mandatory = $true)][string]$PortableOutput
    )

    # The UI invokes the Rust converter from AppContext.BaseDirectory\tools.
    # Single-file .NET applications extract content to a private cache at first
    # start, so place the same native sidecar in the project just for this
    # Publish invocation and ask .NET to include all content in that extraction.
    # Restore the source tree exactly afterwards even when publishing fails.
    $projectDirectory = Split-Path -Parent $ProjectPath
    $projectToolsDirectory = Join-Path $projectDirectory "tools"
    $embeddedSidecar = Join-Path $projectToolsDirectory "mh3g-save-convert.exe"
    $backup = Join-Path ([System.IO.Path]::GetTempPath()) ("mh3g-save-convert-sidecar-" + [Guid]::NewGuid().ToString("N") + ".exe")
    $hadExistingSidecar = Test-Path -LiteralPath $embeddedSidecar -PathType Leaf

    try {
        New-Item -ItemType Directory -Force $projectToolsDirectory | Out-Null
        if ($hadExistingSidecar) {
            Copy-Item -LiteralPath $embeddedSidecar -Destination $backup -Force
        }
        Copy-Item -LiteralPath $NativeSidecar -Destination $embeddedSidecar -Force

        Assert-StageWithinArtifacts -CandidatePath $PortableStageDirectory
        Remove-Item -LiteralPath $PortableStageDirectory -Recurse -Force -ErrorAction SilentlyContinue
        Invoke-External -FilePath $Dotnet -Arguments @(
            "publish", $ProjectPath, "-c", "Release", "-r", "win-x64", "--self-contained", "true", "--no-restore",
            "-p:Platform=x64", "-p:WindowsAppSDKSelfContained=true", "-p:PublishSingleFile=true",
            "-p:IncludeAllContentForSelfExtract=true", "-p:PublishReadyToRun=false", "-o", $PortableStageDirectory
        ) -Description "WinUI portable single-file dotnet publish"

        $publishedPortable = Join-Path $PortableStageDirectory "MH3GSaveConverter.exe"
        if (-not (Test-Path -LiteralPath $publishedPortable -PathType Leaf)) {
            throw "Portable dotnet publish completed without MH3GSaveConverter.exe. See $transcript."
        }
        if ((Get-Item -LiteralPath $publishedPortable).Length -le 0) {
            throw "Portable dotnet publish produced an empty MH3GSaveConverter.exe."
        }
        Copy-Item -LiteralPath $publishedPortable -Destination $PortableOutput -Force
    } finally {
        if ($hadExistingSidecar) {
            Copy-Item -LiteralPath $backup -Destination $embeddedSidecar -Force -ErrorAction SilentlyContinue
        } else {
            Remove-Item -LiteralPath $embeddedSidecar -Force -ErrorAction SilentlyContinue
        }
        Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
    }
}

function Build-InstallerExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$InstallerCompiler,
        [Parameter(Mandatory = $true)][string]$InstallerDefinition,
        [Parameter(Mandatory = $true)][string]$SourceDirectory,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$InstallerOutput
    )

    if (-not (Test-Path -LiteralPath $InstallerDefinition -PathType Leaf)) {
        throw "Inno Setup definition is missing: $InstallerDefinition"
    }
    Remove-Item -LiteralPath $InstallerOutput -Force -ErrorAction SilentlyContinue
    Invoke-External -FilePath $InstallerCompiler -Arguments @(
        "/Qp", "/DSourceDir=$SourceDirectory", "/DOutputDir=$artifactsRoot", "/DAppVersion=$Version", $InstallerDefinition
    ) -Description "Inno Setup installer build"
    if (-not (Test-Path -LiteralPath $InstallerOutput -PathType Leaf)) {
        throw "Inno Setup completed without $InstallerOutput. See $transcript."
    }
    if ((Get-Item -LiteralPath $InstallerOutput).Length -le 0) {
        throw "Inno Setup produced an empty installer executable."
    }
}

function Test-PackagedLayout {
    param(
        [Parameter(Mandatory = $true)][string]$ArchivePath,
        [Parameter(Mandatory = $true)][string]$PackageDirectoryName,
        [Parameter(Mandatory = $true)][string]$PortableExecutable,
        [Parameter(Mandatory = $true)][string]$PortableChecksum,
        [Parameter(Mandatory = $true)][string]$InstallerExecutable,
        [Parameter(Mandatory = $true)][string]$InstallerChecksum
    )

    foreach ($standaloneArtifact in @(
        [PSCustomObject]@{ Executable = $PortableExecutable; Checksum = $PortableChecksum },
        [PSCustomObject]@{ Executable = $InstallerExecutable; Checksum = $InstallerChecksum }
    )) {
        if (-not (Test-Path -LiteralPath $standaloneArtifact.Executable -PathType Leaf)) {
            throw "Windows package self-check is missing $($standaloneArtifact.Executable)"
        }
        if (-not (Test-Path -LiteralPath $standaloneArtifact.Checksum -PathType Leaf)) {
            throw "Windows package self-check is missing $($standaloneArtifact.Checksum)"
        }
        if ((Get-Item -LiteralPath $standaloneArtifact.Executable).Length -le 0) {
            throw "Windows package self-check found an empty standalone executable: $($standaloneArtifact.Executable)"
        }
        $expectedStandaloneHash = ((Get-Content -LiteralPath $standaloneArtifact.Checksum -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
        $actualStandaloneHash = (Get-FileHash -LiteralPath $standaloneArtifact.Executable -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualStandaloneHash -ne $expectedStandaloneHash) {
            throw "Windows package self-check found a standalone executable checksum mismatch: $($standaloneArtifact.Executable)"
        }
    }

    $verifyRoot = Join-Path $artifactsRoot ("mh3g-save-convert-windows-verify-" + [Guid]::NewGuid().ToString("N"))
    try {
        Expand-Archive -LiteralPath $ArchivePath -DestinationPath $verifyRoot -Force
        $packageRoot = Join-Path $verifyRoot $PackageDirectoryName
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
        Assert-NativeConverterSidecar -FilePath $sidecar

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
        $activeEmulators = @(Get-RunningSupportedEmulators)
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
    foreach ($required in @($launcher, $packageReadme, $installerScript, $uiReadme, $uiChineseReadme)) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Required packaging file is missing: $required"
        }
    }
    Assert-StageWithinArtifacts
    Assert-StageWithinArtifacts -CandidatePath $portableStage
    New-Item -ItemType Directory -Force $artifactsRoot | Out-Null
    Initialize-ConsoleUtf8Encoding
    Start-Transcript -LiteralPath $transcript -Force | Out-Null
    $transcriptStarted = $true

    Refresh-ProcessPath
    Set-RustupHomesForCurrentProcess | Out-Null
    Add-RustupBinToProcessPath
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
    $dotnet = Get-DotnetEightSdkCommand
    $cargo = Get-RustToolCommand "cargo"
    $rustup = Get-RustToolCommand "rustup"
    $rustc = Get-RustToolCommand "rustc"
    $innoSetup = Get-InnoSetupCompiler
    if ($null -eq $dotnet -or $null -eq $cargo -or $null -eq $rustup -or $null -eq $rustc -or $null -eq $innoSetup) {
        throw "A required executable disappeared after preflight. Reopen 64-bit PowerShell and rerun with -Bootstrap if needed."
    }
    if (-not $SkipTests) {
        Assert-EmulatorsStoppedForRustTests
    }

    Write-Host "=== Toolchain ==="
    Write-Host ("dotnet: {0}" -f $dotnet)
    & $dotnet --version
    & $cargo --version
    & $rustc --version
    Invoke-External -FilePath $rustup -Arguments @("target", "add", $targetTriple) -Description "Rust target setup"

    Push-Location $repoRoot
    try {
        # Do not clear NuGet/Cargo/target caches: restore and rustup are naturally cache-aware.
        # Self-contained publish with --no-restore requires the exact runtime
        # pack to have been resolved during restore. A framework-only restore
        # succeeds locally but leaves Microsoft.NETCore.App.Runtime.win-x64
        # absent on a clean hosted runner.
        Invoke-External -FilePath $dotnet -Arguments @("restore", $project, "-r", "win-x64") -Description "dotnet restore for win-x64"

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
        Assert-NativeConverterSidecar -FilePath $sidecar
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

        if ($ValidateOnly) {
            Write-Host ""
            Write-Host "Windows x64 package validation complete."
            Write-Host "Validated WinUI publish directory: $stage"
            Write-Host "Validated sidecar: $packagedSidecar"
            Write-Host "Transcript: $transcript"
            return
        }

        # Build all three user-facing formats from the same self-contained WinUI
        # folder and the same native Rust sidecar. The single-file form bundles
        # that sidecar as extracted app content; the installer wraps the folder
        # form so its tools\ path remains intact.
        Remove-Item -LiteralPath $portableExecutable, $portableChecksum, $installerExecutable, $installerChecksum -Force -ErrorAction SilentlyContinue
        Publish-PortableExecutable -Dotnet $dotnet -ProjectPath $project -NativeSidecar $sidecar -PortableStageDirectory $portableStage -PortableOutput $portableExecutable
        Write-Sha256File -FilePath $portableExecutable -OutputPath $portableChecksum -DisplayName "MH3GSaveConverter-Portable-x64.exe" | Out-Null

        $converterVersion = Get-ConverterVersion
        Build-InstallerExecutable -InstallerCompiler $innoSetup -InstallerDefinition $installerScript -SourceDirectory $stage -Version $converterVersion -InstallerOutput $installerExecutable
        Write-Sha256File -FilePath $installerExecutable -OutputPath $installerChecksum -DisplayName "MH3GSaveConverter-Setup-x64.exe" | Out-Null

        Remove-Item -LiteralPath $archive, $archiveChecksum -Force -ErrorAction SilentlyContinue
        Compress-Archive -LiteralPath $stage -DestinationPath $archive -Force
        Write-Sha256File -FilePath $archive -OutputPath $archiveChecksum -DisplayName "mh3g-save-convert-windows-x64.zip" | Out-Null
        Test-PackagedLayout -ArchivePath $archive -PackageDirectoryName $packageDirectoryName -PortableExecutable $portableExecutable -PortableChecksum $portableChecksum -InstallerExecutable $installerExecutable -InstallerChecksum $installerChecksum
        Assert-StageWithinArtifacts -CandidatePath $portableStage
        Remove-Item -LiteralPath $portableStage -Recurse -Force -ErrorAction SilentlyContinue
    } finally {
        Pop-Location
    }

    Write-Host ""
    Write-Host "Windows x64 package complete."
    Write-Host "ZIP: $archive"
    Write-Host "ZIP SHA-256: $archiveChecksum"
    Write-Host "Portable EXE: $portableExecutable"
    Write-Host "Portable EXE SHA-256: $portableChecksum"
    Write-Host "Installer EXE: $installerExecutable"
    Write-Host "Installer EXE SHA-256: $installerChecksum"
    Write-Host "Transcript: $transcript"
} catch {
    Write-Error ("Windows package failed: {0}" -f $_.Exception.Message)
    throw
} finally {
    if ($transcriptStarted) {
        Stop-Transcript | Out-Null
    }
}
