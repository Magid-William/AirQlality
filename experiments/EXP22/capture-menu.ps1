# Evidence capture (EXP22): right-click the tray icon and screenshot the context
# menu, in real mode (where "Refresh" is enabled) and under the mock seam
# (where it is greyed, because no fetch loop is listening).
#
# The menu only appears for a real click on the icon (a synthetic click from a
# background thread cannot take the foreground lock), so this moves the cursor to
# the notification area and sends a genuine right-click - the EXP12 approach.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [Parameter(Mandatory = $true)][ValidateSet("real", "mock")][string]$Mode,
    # The tray icon's screen position. It is NOT stable across runs: killing the
    # app leaves a ghost icon that the shell only clears on hover, and other
    # apps' icons come and go, so the slot moves. Take a 900x80 strip screenshot
    # of the bottom-right corner, flip the verdict colour (mock OPEN green vs
    # mock KEEP CLOSED red) to identify *our* square, then pass its centre here.
    [int]$IconX = 2140,
    [int]$IconY = 1412
)

$mingw = Join-Path $env:LOCALAPPDATA 'mingw64\bin'
if (Test-Path $mingw) { $env:PATH = "$mingw;$env:PATH" }

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Mouse {
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

if ($Mode -eq "mock") {
    $env:AIRQLALITY_SOURCE = "mock"
    $env:AIRQLALITY_FREEZE_NOW = "1"
} else {
    Remove-Item Env:AIRQLALITY_SOURCE -ErrorAction SilentlyContinue
    Remove-Item Env:AIRQLALITY_FREEZE_NOW -ErrorAction SilentlyContinue
}

$p = Start-Process -FilePath $Exe -PassThru
# Wait for the tray icon to be added and the window to settle.
Start-Sleep -Seconds 6
[Mouse]::RightClick($IconX, $IconY)
Start-Sleep -Milliseconds 900

$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.X, $bounds.Y, 0, 0, $bounds.Size)
$g.Dispose()

# Zoom on the notification-area corner where a popup menu expands.
$w = 1140; $h = 330
if ($w -gt $bounds.Width) { $w = $bounds.Width }
if ($h -gt $bounds.Height) { $h = $bounds.Height }
$zoomPath = [System.IO.Path]::ChangeExtension($Out, $null).TrimEnd('.') + "-zoom.png"
$zoom = New-Object System.Drawing.Bitmap $w, $h
$zg = [System.Drawing.Graphics]::FromImage($zoom)
$zg.DrawImage($bmp, (New-Object System.Drawing.Rectangle 0, 0, $w, $h), (New-Object System.Drawing.Rectangle ($bounds.Width - $w), ($bounds.Height - $h), $w, $h), [System.Drawing.GraphicsUnit]::Pixel)
$zg.Dispose()
$zoom.Save($zoomPath, [System.Drawing.Imaging.ImageFormat]::Png)
$zoom.Dispose()

$bmp.Dispose()
Write-Output "SAVED $zoomPath ($w x $h) mode=$Mode"

$p.Kill()
