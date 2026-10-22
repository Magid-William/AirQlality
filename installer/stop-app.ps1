# Stop the AirQlality instance that runs from the path the installer is about to
# replace (EXP29).
#
# Restart Manager cannot do this: the widget treats a window close as "hide to
# the tray" (EXP11), so RM's WM_CLOSE never ends the process and setup gives up
# with exit code 5. A portable copy elsewhere is *not* touched.
#
# Extracted to {tmp} by the installer (`ExtractTemporaryFile`) and run from
# `CurStepChanged`'s PrepareToInstall, before any file is replaced.
#
# Exit codes: 0 = nothing to stop, or every matching copy is gone;
#             1 = a matching copy survived (the setup aborts and says so).
param(
    [Parameter(Mandatory = $true)][string]$Path
)

$ErrorActionPreference = 'Stop'
$target = [System.IO.Path]::GetFullPath($Path)
$stopped = @()

foreach ($p in @(Get-Process -Name 'airqlality' -ErrorAction SilentlyContinue)) {
    try {
        if (-not $p.Path) { continue }
        if ([System.IO.Path]::GetFullPath($p.Path) -ine $target) { continue }
        $p.Kill()
        $stopped += $p
    } catch {
        # Access denied or already gone; the wait below reports a survivor.
    }
}

foreach ($p in $stopped) {
    if (-not $p.WaitForExit(10000)) {
        Write-Output "still running: pid=$($p.Id)"
        exit 1
    }
}

Write-Output "stopped $($stopped.Count) instance(s) from $target"
exit 0
