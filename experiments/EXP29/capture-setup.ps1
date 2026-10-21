# Screenshot the AirQlality installer's first page (EXP29 evidence).
#
# `app/tests/capture.ps1` finds the *started* process's main window, which does
# not work for Inno Setup: the setup exe unpacks itself to %TEMP% and the wizard
# lives in a child process. So this finds any top-level window of class
# `TSetupForm` whose title mentions AirQlality, raises it and copies the screen
# (CopyFromScreen, not PrintWindow - the same rule the widget's capture uses).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File `
#       experiments\EXP29\capture-setup.ps1 -Setup dist\AirQlality-...-setup.exe `
#       -Out experiments\EXP29\EXP29-setup.png
param(
    [Parameter(Mandatory = $true)][string]$Setup,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$WaitSeconds = 10
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class W {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$p = Start-Process -FilePath (Resolve-Path $Setup).Path -PassThru
$h = [IntPtr]::Zero
$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ($h -eq [IntPtr]::Zero -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 400
    $script:found = [IntPtr]::Zero
    $cb = [W+EnumProc]{
        param($hwnd, $l)
        if (-not [W]::IsWindowVisible($hwnd)) { return $true }
        $cls = New-Object System.Text.StringBuilder 64
        [void][W]::GetClassName($hwnd, $cls, 64)
        if ($cls.ToString() -eq 'TSetupForm' -or $cls.ToString() -eq 'TWizardForm') {
            $txt = New-Object System.Text.StringBuilder 256
            [void][W]::GetWindowText($hwnd, $txt, 256)
            if ($txt.ToString() -like '*AirQlality*') { $script:found = $hwnd; return $false }
        }
        return $true
    }
    [void][W]::EnumWindows($cb, [IntPtr]::Zero)
    $h = $script:found
}
if ($h -eq [IntPtr]::Zero) { throw "no setup window appeared within $WaitSeconds s" }

[void][W]::ShowWindow($h, 9)          # SW_RESTORE
[void][W]::SetForegroundWindow($h)
Start-Sleep -Milliseconds 900
$r = New-Object W+RECT
[void][W]::GetWindowRect($h, [ref]$r)
$w = $r.Right - $r.Left
$hgt = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $hgt
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$bmp.Save((Join-Path (Get-Location) $Out), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

# Inno runs the wizard in a child process (the setup exe it was started from is
# only the extractor), so close the window's owner and then the extractor.
$owner = Get-Process | Where-Object { $_.MainWindowHandle -eq $h } | Select-Object -First 1
if ($owner) { Stop-Process -Id $owner.Id -Force -ErrorAction SilentlyContinue }
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Write-Output "SAVED $Out ($w x $hgt)"
