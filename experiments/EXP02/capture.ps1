param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Out
)
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 6
$p.Refresh()
if ($p.HasExited) {
    Write-Output "PROCESS_EXITED code=$($p.ExitCode)"
    exit 2
}

$h = $p.MainWindowHandle
Write-Output "HWND=$h"
if ($h -ne [IntPtr]::Zero) {
    [W]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 1000
}

$r = New-Object W+RECT
[W]::GetWindowRect($h, [ref]$r) | Out-Null
$x = $r.Left; $y = $r.Top
$w = $r.Right - $r.Left; $hh = $r.Bottom - $r.Top
Write-Output "RECT=($x,$y,$w,$hh)"
if ($w -le 0 -or $hh -le 0) {
    $x = 0; $y = 0
    $w = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width
    $hh = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height
}

$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size $w, $hh))
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "SAVED $Out ($w x $hh)"

Start-Sleep -Milliseconds 300
$p.Kill()
Write-Output "KILLED"
