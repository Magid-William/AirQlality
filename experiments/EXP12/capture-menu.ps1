# Evidence capture (EXP12): right-click the tray icon and screenshot the context
# menu, once with "Run at startup" checked and once unchecked.
#
# The menu can only appear for a real click on the icon (a synthetic click from a
# background thread cannot take the foreground lock), so this moves the cursor to
# the notification area and sends a genuine right-click. A demo Run value name is
# used, so the real "AirQlality" entry is never touched.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [Parameter(Mandatory = $true)][ValidateSet("on", "off")][string]$State,
    [string]$Name = "AirQlalityDemo",
    # The tray icon's screen position. On this 2560x1440 desktop it is the last
    # notification-area icon before the clock; override if the layout differs.
    [int]$IconX = 2216,
    [int]$IconY = 1416
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

$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$value = '"' + $exePath + '" --startup'
if ($State -eq "on") {
    Set-ItemProperty -Path $key -Name $Name -Value $value
} else {
    Remove-ItemProperty -Path $key -Name $Name -ErrorAction SilentlyContinue
}

$env:AIRQLALITY_STARTUP_NAME = $Name
$env:AIRQLALITY_SOURCE = "mock"
$env:AIRQLALITY_FREEZE_NOW = "1"

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
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "SAVED $Out ($($bounds.Width) x $($bounds.Height)) state=$State"

$p.Kill()
Remove-ItemProperty -Path $key -Name $Name -ErrorAction SilentlyContinue
