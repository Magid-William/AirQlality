# Evidence capture (EXP28): open the tray context menu with a real right-click
# (a synthetic click from a background thread cannot take the foreground lock),
# walk down to "Notify me on" and open its submenu with the keyboard, then
# screenshot the notification-area corner.
#
# The icon is located by its verdict colour, because its slot moves between runs
# (ghost icons, other apps). The shot is reproducible: an isolated %APPDATA%
# supplies the saved choice, and the mock is pinned to an OPEN hour so no balloon
# covers the corner while the menu is open.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    # What the submenu should show as checked.
    [string]$NotifyOn = "closed,vent",
    # 0 = find the tray icon by the verdict colour.
    [int]$IconX = 0,
    [int]$IconY = 0
)

$mingw = Join-Path $env:LOCALAPPDATA 'mingw64\bin'
if (Test-Path $mingw) { $env:PATH = "$mingw;$env:PATH" }

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Mouse2 {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  public static void RightClick(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(250);
    mouse_event(0x0008, 0, 0, 0, UIntPtr.Zero); // RIGHTDOWN
    mouse_event(0x0010, 0, 0, 0, UIntPtr.Zero); // RIGHTUP
  }
}
"@

function Find-TrayIcon([int]$argb) {
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $w = 500; $h = 60
    $strip = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($strip)
    $g.CopyFromScreen($bounds.Width - $w, $bounds.Height - $h, 0, 0, $strip.Size)
    $g.Dispose()
    # Count matching pixels per column; our solid square is the tallest run.
    $cols = New-Object int[] $w
    for ($y = 0; $y -lt $h; $y++) {
        for ($x = 0; $x -lt $w; $x++) {
            if (($strip.GetPixel($x, $y).ToArgb() -band 0xFFFFFF) -eq $argb) { $cols[$x]++ }
        }
    }
    $strip.Dispose()
    $best = 0; $bestX = -1
    for ($x = 0; $x -lt $w; $x++) { if ($cols[$x] -gt $best) { $best = $cols[$x]; $bestX = $x } }
    if ($bestX -lt 0 -or $best -lt 6) { return $null }
    # Centre of the winning columns.
    $x0 = $bestX; while ($x0 -gt 0 -and $cols[$x0 - 1] -ge 6) { $x0-- }
    $x1 = $bestX; while ($x1 -lt $w - 1 -and $cols[$x1 + 1] -ge 6) { $x1++ }
    return @{ X = $bounds.Width - $w + [int](($x0 + $x1) / 2); Y = $bounds.Height - $h + 30 }
}

# Isolated settings: no location.txt means Cairo, and notify.txt is ours.
$appdata = Join-Path $env:TEMP ("airqlality-menu-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path (Join-Path $appdata "AirQlality") -Force | Out-Null
Set-Content -Path (Join-Path $appdata "AirQlality\notify.txt") -Value $NotifyOn -NoNewline
$env:APPDATA = $appdata
$env:AIRQLALITY_SOURCE = "mock"
$env:AIRQLALITY_FREEZE_NOW = "1"
$env:AIRQLALITY_MOCK_NOW = "36"   # OPEN: green icon, and no balloon over the menu

$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 6

$green = 0x3FB950
if ($IconX -eq 0 -or $IconY -eq 0) {
    $found = Find-TrayIcon $green
    if ($null -eq $found) {
        Write-Output "TRAY_ICON_NOT_FOUND (pass -IconX/-IconY)"
        $p.Kill()
        exit 3
    }
    $IconX = $found.X; $IconY = $found.Y
    Write-Output "ICON=$IconX,$IconY"
}

[Mouse2]::RightClick($IconX, $IconY)
Start-Sleep -Milliseconds 900

# The popup owns the keyboard: four items down (Show/Hide, Refresh, API keys...,
# Notify me on), then Right opens the submenu.
[System.Windows.Forms.SendKeys]::SendWait("{DOWN}{DOWN}{DOWN}{DOWN}{RIGHT}")
Start-Sleep -Milliseconds 900

$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.X, $bounds.Y, 0, 0, $bounds.Size)
$g.Dispose()

$w = 1140; $h = 330
if ($w -gt $bounds.Width) { $w = $bounds.Width }
if ($h -gt $bounds.Height) { $h = $bounds.Height }
$zoom = New-Object System.Drawing.Bitmap $w, $h
$zg = [System.Drawing.Graphics]::FromImage($zoom)
$zg.DrawImage(
    $bmp,
    (New-Object System.Drawing.Rectangle 0, 0, $w, $h),
    (New-Object System.Drawing.Rectangle ($bounds.Width - $w), ($bounds.Height - $h), $w, $h),
    [System.Drawing.GraphicsUnit]::Pixel
)
$zg.Dispose()
$zoom.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$zoom.Dispose()
$bmp.Dispose()

Write-Output "SAVED $Out ($w x $h) notify_on=$NotifyOn"
# Leave the menus so the app is not stuck in the modal loop.
[System.Windows.Forms.SendKeys]::SendWait("{ESC}{ESC}")
Start-Sleep -Milliseconds 300
$p.Kill()
Remove-Item -Recurse -Force $appdata -ErrorAction SilentlyContinue
