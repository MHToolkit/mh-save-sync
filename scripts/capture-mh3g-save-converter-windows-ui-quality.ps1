param(
    [Parameter(Mandatory = $true)][string]$AppPath,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [ValidateSet("1120x760", "920x600")][string]$WindowSize = "1120x760",
    [ValidateSet("normal", "reduced")][string]$Motion = "normal"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NativeWindowSizing {
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);
}
"@

$fixtures = @(
    "first-run", "input.empty", "components.optional-missing", "components.optional-skipped",
    "dry-run.ready", "dry-run.blocked", "write.authorized", "write.confirmation",
    "conversion.success", "conversion.failure", "history.empty", "history.result"
)
$requiredIds = @(
    "mh3g.converter.windows.navigation.convert",
    "mh3g.converter.windows.navigation.history",
    "mh3g.converter.windows.navigation.experimentalCEC",
    "mh3g.converter.windows.navigation.settings"
)

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$width, $height = $WindowSize.Split("x") | ForEach-Object { [int]$_ }
$results = @()

if ($Motion -eq "reduced") {
    $animationKey = "HKCU:\Control Panel\Desktop\WindowMetrics"
    $previousMinAnimate = (Get-ItemProperty -Path $animationKey -Name MinAnimate -ErrorAction SilentlyContinue).MinAnimate
    Set-ItemProperty -Path $animationKey -Name MinAnimate -Value "0"
}

foreach ($fixture in $fixtures) {
    $process = Start-Process -FilePath $AppPath -ArgumentList @("--ui-fixture", $fixture) -PassThru
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        do {
            Start-Sleep -Milliseconds 250
            $process.Refresh()
        } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)
        if ($process.MainWindowHandle -eq 0) { throw "No interactive window for fixture $fixture" }

        [NativeWindowSizing]::MoveWindow($process.MainWindowHandle, 80, 80, $width, $height, $true) | Out-Null
        Start-Sleep -Milliseconds 350

        $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $windowPattern = $root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
        $windowPattern.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Normal)

        if ($fixture -eq "dry-run.ready") {
            $detailsCondition = [System.Windows.Automation.PropertyCondition]::new(
                [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
                "mh3g.converter.windows.details.dryRun"
            )
            $details = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $detailsCondition)
            if ($null -eq $details) { throw "Missing Dry Run technical-details expander" }
            $expandPattern = $details.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
            $expandPattern.Expand()
            Start-Sleep -Milliseconds 200
        }

        $bounds = $root.Current.BoundingRectangle
        if ([Math]::Abs($bounds.Width - $width) -gt 16 -or [Math]::Abs($bounds.Height - $height) -gt 16) {
            throw "Window did not reach requested $WindowSize for $fixture: $($bounds.Width)x$($bounds.Height)"
        }
        $bitmap = New-Object System.Drawing.Bitmap([int]$bounds.Width, [int]$bounds.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen([int]$bounds.X, [int]$bounds.Y, 0, 0, $bitmap.Size)
        $png = Join-Path $OutputDirectory "$fixture-$WindowSize-$Motion.png"
        $bitmap.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
        $graphics.Dispose(); $bitmap.Dispose()

        $nodes = $root.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition
        ) | ForEach-Object {
            $value = $null
            try {
                $valuePattern = $_.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
                $value = $valuePattern.Current.Value
            }
            catch { }
            [ordered]@{
                id = $_.Current.AutomationId
                name = $_.Current.Name
                value = $value
                type = $_.Current.ControlType.ProgrammaticName
                enabled = $_.Current.IsEnabled
                focusable = $_.Current.IsKeyboardFocusable
                offscreen = $_.Current.IsOffscreen
            }
        }
        $ids = @($nodes | ForEach-Object { $_.id } | Where-Object { $_ })
        if (($ids | Select-Object -Unique).Count -ne $ids.Count) { throw "Duplicate AutomationId in $fixture" }
        foreach ($id in $requiredIds) { if ($ids -notcontains $id) { throw "Missing $id in $fixture" } }
        if ($fixture -eq "dry-run.ready") {
            $report = $nodes | Where-Object { $_.id -eq "mh3g.converter.windows.details.dryRun.report" }
            if ($null -eq $report -or $report.value -notmatch '"status":"dry-run"') {
                throw "Expanded Dry Run technical details did not expose the synthetic report"
            }
        }

        $uiaPath = Join-Path $OutputDirectory "$fixture-$WindowSize-$Motion-uia.json"
        $nodes | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $uiaPath
        $results += [ordered]@{ fixture=$fixture; screenshot=$png; uia=$uiaPath; processId=$process.Id }
    }
    finally {
        if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}

$results | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $OutputDirectory "capture-index.json")
if ($Motion -eq "reduced" -and $null -ne $previousMinAnimate) {
    Set-ItemProperty -Path $animationKey -Name MinAnimate -Value $previousMinAnimate
}
Write-Host "Captured $($results.Count) deterministic Windows UI fixtures."
