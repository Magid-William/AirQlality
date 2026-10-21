# Exercise the EXP11 tray window lifecycle: hide-on-close and quit (see README).
#
# Output lines the E2E test parses:
#   STARTED <hwnd>
#   ALIVE=1 VISIBLE=0 DUMP=1     (mode hide: window hidden, process + icon alive)
#   EXITED code=<n>              (mode quit: the app exited on its own)
# or, on early failure:
#   PROCESS_EXITED code=<n> / STILL_RUNNING
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][ValidateSet("hide", "quit")][string]$Mode,
    # Path the app writes its tray state to (mode hide asserts it still exists).
    [string]$Dump = "",
    [int]$WaitSeconds = 5,
    [int]$TimeoutSeconds = 20
)

# The app is a GNU/MinGW build; prepend the toolchain's runtime DLLs.
$mingw = Join-Path $env:LOCALAPPDATA 'mingw64\bin'
if (Test-Path $mingw) { $env:PATH = "$mingw;$env:PATH" }

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Tray {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
"@

if ($Dump -ne "") { $env:AIRQLALITY_TRAY_DUMP = $Dump }

$p = Start-Process -FilePath $Exe -PassThru

if ($Mode -eq "quit") {
    # The AIRQLALITY_TRAY_QUIT_MS seam asks the app to quit the same way the tray
    # menu does; a real app must exit on its own.
    if ($p.WaitForExit($TimeoutSeconds * 1000)) {
        Write-Output "EXITED code=$($p.ExitCode)"
        exit 0
    }
    Write-Output "STILL_RUNNING"
    $p.Kill()
    exit 3
}

Start-Sleep -Seconds $WaitSeconds
$p.Refresh()
if ($p.HasExited) {
    Write-Output "PROCESS_EXITED code=$($p.ExitCode)"
    exit 2
}

$h = $p.MainWindowHandle
Write-Output "STARTED $h"
[void][Tray]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)  # WM_CLOSE
Start-Sleep -Milliseconds 1200

$p.Refresh()
$alive = if ($p.HasExited) { 0 } else { 1 }
$visible = if ([Tray]::IsWindowVisible($h)) { 1 } else { 0 }
$dumpExists = if ($Dump -ne "" -and (Test-Path $Dump)) { 1 } else { 0 }
Write-Output "ALIVE=$alive VISIBLE=$visible DUMP=$dumpExists"

$p.Kill()
