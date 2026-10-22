# Black-box proof that the AirQlality installer works (EXP29).
#
# Installs the setup silently into a scratch directory, checks what a user would
# see (exe, Start Menu shortcut, Add/Remove Programs entry), simulates the app's
# own "Run at startup" toggle (EXP12), launches the *installed* widget on the
# mock source through the existing capture script, then uninstalls silently and
# checks that nothing is left behind - while the user's settings survive.
#
# No admin rights are used or needed: the installer is per-user.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File installer\proof.ps1 `
#       -Setup dist\AirQlality-v0.2.0-windows-x64-setup.exe `
#       -PngOut experiments\EXP29\EXP29-installed-widget.png
#
# Prints one line per assertion and exits non-zero on the first failure.
param(
    [Parameter(Mandatory = $true)][string]$Setup,
    # Where to save the screenshot of the installed widget (skips it when empty).
    [string]$PngOut = "",
    # Keep the scratch directory around for inspection.
    [switch]$Keep
)

$ErrorActionPreference = 'Stop'

$failures = 0
function Check([string]$what, [bool]$ok, [string]$detail = "") {
    if ($ok) {
        Write-Output "PASS  $what$(if ($detail) { " - $detail" })"
    } else {
        Write-Output "FAIL  $what$(if ($detail) { " - $detail" })"
        $script:failures++
    }
}

$setupPath = (Resolve-Path $Setup).Path
$root = Join-Path $env:TEMP ("airqlality-install-proof-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
$dir = Join-Path $root 'app'
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\AirQlality.lnk'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$uninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
$marker = Join-Path $env:APPDATA 'AirQlality\exp29-proof.txt'
New-Item -ItemType Directory -Force -Path $root | Out-Null

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$elevated = (New-Object Security.Principal.WindowsPrincipal $identity).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
Write-Output "INFO  elevated=$elevated (the install must not need it)"
Write-Output "INFO  setup=$setupPath"
Write-Output "INFO  dir=$dir"

function Invoke-Installer([string]$exe, [string[]]$extra) {
    $args = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') + $extra
    $p = Start-Process -FilePath $exe -ArgumentList $args -Wait -PassThru
    return $p.ExitCode
}

function Invoke-Setup([string]$exe, [string]$log) {
    return Invoke-Installer $exe @("/DIR=`"$dir`"", "/LOG=`"$log`"")
}

function Get-ArpEntry {
    Get-ChildItem $uninstallRoot -ErrorAction SilentlyContinue | ForEach-Object {
        $props = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
        if ($props.DisplayName -eq 'AirQlality') { $props }
    }
}

# A hosted runner's %TEMP% is an 8.3 short path (`...\RUNNER~1\...`) while the
# shortcut and the registry store the long one, so compare canonical paths - not
# the strings this script happens to hold.
function Get-LongPath([string]$p) {
    if (-not $p -or -not (Test-Path $p)) { return $p }
    try { return (New-Object -ComObject Scripting.FileSystemObject).GetFile($p).Path }
    catch { return $p }
}

# ---------------------------------------------------------------- install (x2)
# Twice on purpose: the second run is the in-place upgrade path (same AppId,
# same directory) and must not stack a second entry or shortcut.
foreach ($round in 1, 2) {
    $log = Join-Path $root "install-$round.log"
    $code = Invoke-Setup $setupPath $log
    Check "silent install #$round exits 0" ($code -eq 0) "exit=$code log=$log"
}

$exe = Join-Path $dir 'airqlality.exe'
Check 'the installed exe exists' (Test-Path $exe) $exe
if (Test-Path $exe) {
    $mb = [math]::Round((Get-Item $exe).Length / 1MB, 1)
    Check 'the installed exe is a real binary' ($mb -gt 5) "$mb MiB"
}
$dataKept = Join-Path $env:APPDATA 'AirQlality'
New-Item -ItemType Directory -Force -Path $dataKept | Out-Null
Set-Content -Path $marker -Value 'survives an uninstall (EXP29 proof)' -Encoding ascii

# ------------------------------------------------------------------- shortcuts
Check 'Start Menu shortcut exists' (Test-Path $startMenu) $startMenu
if (Test-Path $startMenu) {
    $shell = New-Object -ComObject WScript.Shell
    $target = $shell.CreateShortcut($startMenu).TargetPath
    $same = (Get-LongPath $target) -ieq (Get-LongPath $exe)
    Check 'Start Menu shortcut points at the installed exe' $same $target
}
$desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'AirQlality.lnk'
Check 'the desktop icon is opt-in (absent)' (-not (Test-Path $desktop)) $desktop

# --------------------------------------------------------- Add/Remove Programs
$entry = @(Get-ArpEntry)
Check 'exactly one Add/Remove Programs entry after the upgrade' ($entry.Count -eq 1) "count=$($entry.Count)"
if ($entry.Count -ge 1) {
    $e = $entry[0]
    Check 'the entry names the version' ($e.DisplayVersion -match '^\d+\.\d+\.\d+') $e.DisplayVersion
    Check 'the entry has an uninstaller' ([bool]$e.UninstallString) $e.UninstallString
    Check 'the entry shows the app icon' ($e.DisplayIcon -like "*$exe*") $e.DisplayIcon
    $uninstallKey = $e.PSPath
}

# --------------------------------------------------- EXP12 autostart, then undo
# The installer never writes this key; the app's tray toggle does. Simulate it so
# the uninstaller's cleanup is actually exercised.
$expected = '"' + $exe + '" --startup'
New-ItemProperty -Path $runKey -Name 'AirQlality' -Value $expected -PropertyType String -Force | Out-Null
Check 'the run-at-startup value is in place' ((Get-ItemProperty $runKey).AirQlality -eq $expected)

# ------------------------------------------------------------- the widget runs
if ($PngOut) {
    $capture = Join-Path $PSScriptRoot '..\app\tests\capture.ps1'
    $env:AIRQLALITY_SOURCE = 'mock'
    $env:AIRQLALITY_FREEZE_NOW = '1'
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $capture -Exe $exe -Out $PngOut 2>&1 | Out-String
    Remove-Item Env:AIRQLALITY_SOURCE, Env:AIRQLALITY_FREEZE_NOW
    Check 'the installed widget starts and renders' ($out -match 'SAVED') ($out.Trim() -split "`n" | Select-Object -First 1)
    Check 'the screenshot was written' (Test-Path $PngOut) $PngOut
}

# --------------------------------------- upgrade over a running tray app (EXP29)
# The real-world update path: the widget is running (hidden in the tray), the user
# runs the new setup. `CloseApplications=yes` must close it instead of failing on
# the locked exe. Mock seams only: no network, no wait for real data.
$env:AIRQLALITY_SOURCE = 'mock'
$env:AIRQLALITY_FREEZE_NOW = '1'
$running = Start-Process -FilePath $exe -PassThru
$deadline = (Get-Date).AddSeconds(15)
while ($running.MainWindowHandle -eq 0 -and -not $running.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 250
    $running.Refresh()
}
Check 'the widget is running before the upgrade' (-not $running.HasExited) "pid=$($running.Id)"
# A portable copy running from *another* directory belongs to the user, not to
# this install: the upgrade must not touch it.
$portable = Join-Path $root 'portable'
New-Item -ItemType Directory -Force -Path $portable | Out-Null
Copy-Item $exe (Join-Path $portable 'airqlality.exe')
$portableProc = Start-Process -FilePath (Join-Path $portable 'airqlality.exe') -PassThru
Start-Sleep -Seconds 2
$code = Invoke-Setup $setupPath (Join-Path $root 'install-running.log')
$deadline = (Get-Date).AddSeconds(30)
while (-not $running.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 250
    $running.Refresh()
}
Check 'the upgrade succeeds while the app runs' ($code -eq 0) "exit=$code"
Check 'the upgrade closes the running app' ($running.HasExited)
$portableProc.Refresh()
Check 'the upgrade leaves a portable copy alone' (-not $portableProc.HasExited)
Check 'the upgrade leaves a working exe' (Test-Path $exe)
Check 'the running upgrade keeps one Add/Remove Programs entry' (@(Get-ArpEntry).Count -eq 1)
if (-not $running.HasExited) { Stop-Process -Id $running.Id -Force -ErrorAction SilentlyContinue }
if (-not $portableProc.HasExited) { Stop-Process -Id $portableProc.Id -Force -ErrorAction SilentlyContinue }
Remove-Item Env:AIRQLALITY_SOURCE, Env:AIRQLALITY_FREEZE_NOW

# ------------------------------------------------------------------- uninstall
$unins = Join-Path $dir 'unins000.exe'
Check 'the uninstaller was installed' (Test-Path $unins) $unins
if (Test-Path $unins) {
    $log = Join-Path $root 'uninstall.log'
    # The uninstaller copies itself to %TEMP% and returns at once, so wait for
    # the exe to disappear rather than for the process.
    $code = Invoke-Installer $unins @("/LOG=`"$log`"")
    $deadline = (Get-Date).AddSeconds(90)
    while ((Test-Path $exe) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
    Check 'the uninstaller unpacks and exits 0' ($code -eq 0) "exit=$code log=$log"
    Check 'the exe is gone' (-not (Test-Path $exe)) $exe
    Check 'the install directory is gone' (-not (Test-Path $dir)) $dir
    Check 'the Start Menu shortcut is gone' (-not (Test-Path $startMenu)) $startMenu
    Start-Sleep -Milliseconds 500
    Check 'the Add/Remove Programs entry is gone' (@(Get-ArpEntry).Count -eq 0)
    Check 'the run-at-startup value is gone' ($null -eq (Get-ItemProperty $runKey -Name 'AirQlality' -ErrorAction SilentlyContinue)) $runKey
    Check 'the user settings survive' (Test-Path $marker) $marker
}

Remove-Item $marker -ErrorAction SilentlyContinue
if (-not $Keep) { Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue }

if ($failures -gt 0) {
    Write-Output "RESULT FAIL ($failures assertion(s))"
    exit 1
}
Write-Output 'RESULT PASS'
exit 0
