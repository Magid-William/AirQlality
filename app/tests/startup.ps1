# Exercise the EXP12 run-at-startup registration (see experiments/EXP12/README.md).
#
# The test uses a dedicated Run value name (default "AirQlalityE2E") so the real
# "AirQlality" entry is never touched, and removes it afterwards.
#
# Output lines the E2E test parses:
#   REGISTERED=1 VALUE=<command>     (mode on: the Run value now launches the app)
#   REGISTERED=0 VALUE=<command|->   (mode off: the value was removed)
#   ALIVE=1 VISIBLE=0 HWND=<n>       (mode hidden: --startup opened hidden)
#   DUMP=<json>                      (the app's own view, from AIRQLALITY_STARTUP_DUMP)
# or, on early failure:
#   PROCESS_EXITED code=<n> / NO_WINDOW
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][ValidateSet("on", "off", "hidden")][string]$Mode,
    [string]$Name = "AirQlalityE2E",
    [string]$Dump = "",
    [int]$WaitSeconds = 6,
    [int]$TimeoutSeconds = 20
)

# The app is a GNU/MinGW build; prepend the toolchain's runtime DLLs.
$mingw = Join-Path $env:LOCALAPPDATA 'mingw64\bin'
if (Test-Path $mingw) { $env:PATH = "$mingw;$env:PATH" }

$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$expected = '"' + $exePath + '" --startup'
$dumpPath = if ($Dump -ne "") { $Dump } else { Join-Path $env:TEMP "airqlality-startup-$PID.json" }
$script:dumpPath = $dumpPath
if (Test-Path $dumpPath) { Remove-Item $dumpPath -Force }

# Never start from a leftover value.
Remove-ItemProperty -Path $key -Name $Name -ErrorAction SilentlyContinue

function Cleanup { Remove-ItemProperty -Path $key -Name $Name -ErrorAction SilentlyContinue }
trap { Cleanup; break }

if ($Mode -eq "hidden") {
    Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WinFinder {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextLengthW(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static IntPtr Find(uint target) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target && GetWindowTextLengthW(h) > 0) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
    $env:AIRQLALITY_SOURCE = "mock"
    $env:AIRQLALITY_FREEZE_NOW = "1"
    $p = Start-Process -FilePath $Exe -ArgumentList "--startup" -PassThru
    $h = [IntPtr]::Zero
    for ($i = 0; $i -lt ($WaitSeconds * 4); $i++) {
        Start-Sleep -Milliseconds 250
        $p.Refresh()
        if ($p.HasExited) { Cleanup; Write-Output "PROCESS_EXITED code=$($p.ExitCode)"; exit 2 }
        $h = [WinFinder]::Find([uint32]$p.Id)
        if ($h -ne [IntPtr]::Zero) { break }
    }
    if ($h -eq [IntPtr]::Zero) { $p.Kill(); Cleanup; Write-Output "NO_WINDOW"; exit 3 }
    $alive = if ($p.HasExited) { 0 } else { 1 }
    $visible = if ([WinFinder]::IsWindowVisible($h)) { 1 } else { 0 }
    Write-Output "ALIVE=$alive VISIBLE=$visible HWND=$h"
    $p.Kill()
    Cleanup
    exit 0
}

# Modes on/off: replay the tray menu's toggle once via AIRQLALITY_STARTUP_TOGGLE.
if ($Mode -eq "off") {
    # Pretend the app is already registered, so the toggle must remove it.
    Set-ItemProperty -Path $key -Name $Name -Value $expected
}
$env:AIRQLALITY_STARTUP_NAME = $Name
$env:AIRQLALITY_STARTUP_TOGGLE = "1"
$env:AIRQLALITY_STARTUP_DUMP = $dumpPath
$env:AIRQLALITY_SOURCE = "mock"
$env:AIRQLALITY_FREEZE_NOW = "1"

$p = Start-Process -FilePath $Exe -PassThru
# Wait for the app to run the toggle (it writes the dump just afterwards).
for ($i = 0; $i -lt ($WaitSeconds * 4); $i++) {
    Start-Sleep -Milliseconds 250
    $p.Refresh()
    if ($p.HasExited) { Cleanup; Write-Output "PROCESS_EXITED code=$($p.ExitCode)"; exit 2 }
    if (Test-Path $dumpPath) { break }
}

$value = (Get-ItemProperty -Path $key -Name $Name -ErrorAction SilentlyContinue).$Name
$registered = if ($value -and ($value -ieq $expected)) { 1 } else { 0 }
$dump = if (Test-Path $dumpPath) { Get-Content -Raw -Path $dumpPath } else { "" }
Write-Output "REGISTERED=$registered VALUE=$(if ($value) { $value } else { '-' })"
Write-Output "DUMP=$dump"

$p.Kill()
Cleanup
