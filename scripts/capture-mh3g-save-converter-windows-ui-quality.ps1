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

foreach ($fixture in $fixtures) {
    $process = Start-Process -FilePath $AppPath -ArgumentList @("--ui-fixture", $fixture) -PassThru
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        do {
            Start-Sleep -Milliseconds 250
            $process.Refresh()
        } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)
        if ($process.MainWindowHandle -eq 0) { throw "No interactive window for fixture $fixture" }

        $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $windowPattern = $root.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
        $windowPattern.SetWindowVisualState([System.Windows.Automation.WindowVisualState]::Normal)

        $bounds = $root.Current.BoundingRectangle
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
            [ordered]@{
                id = $_.Current.AutomationId
                name = $_.Current.Name
                type = $_.Current.ControlType.ProgrammaticName
                enabled = $_.Current.IsEnabled
                focusable = $_.Current.IsKeyboardFocusable
                offscreen = $_.Current.IsOffscreen
            }
        }
        $ids = @($nodes | ForEach-Object { $_.id } | Where-Object { $_ })
        if (($ids | Select-Object -Unique).Count -ne $ids.Count) { throw "Duplicate AutomationId in $fixture" }
        foreach ($id in $requiredIds) { if ($ids -notcontains $id) { throw "Missing $id in $fixture" } }

        $uiaPath = Join-Path $OutputDirectory "$fixture-$WindowSize-$Motion-uia.json"
        $nodes | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $uiaPath
        $results += [ordered]@{ fixture=$fixture; screenshot=$png; uia=$uiaPath; processId=$process.Id }
    }
    finally {
        if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
}

$results | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $OutputDirectory "capture-index.json")
Write-Host "Captured $($results.Count) deterministic Windows UI fixtures."
