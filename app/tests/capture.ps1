# Launch the app, screenshot its window, and kill it.
#
# Generalised from experiments/EXP02/capture.ps1. Two documented reasons for the
# approach:
#   * `CopyFromScreen` is used (not `PrintWindow`) because the gpui window is
#     DirectX-rendered and `PrintWindow` misses GPU content.
#   * The GNU/MinGW build needs its runtime DLLs on PATH, which this script adds.
#
# Output lines the E2E test parses:
#   HWND=<handle>
#   RECT=(x,y,w,h)
#   SAVED <path> (<w> x <h>)
# or, on early exit:
#   PROCESS_EXITED code=<n>
param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [Parameter(Mandatory=$true)][string]$Out,
    [int]$WaitSeconds = 6,
    # Optional: keystrokes to send to the focused window before capturing, e.g.
    # "tokyo" to exercise the search field's real keyboard path.
    [string]$Type = "",
    # Optional: space-separated named keys to press before capturing, e.g.
    # "tab" or "down down enter" or "tab enter". Named keys carry no modifier,
    # so ctrl+key chords cannot be posted this way (Windows derives modifiers
    # from the real keyboard state, which PostMessage does not change).
    [string]$Press = "",
    # Post WM_GETOBJECT with the UIA root object id, which is what a screen
    # reader sends to make the platform activate accessibility. Lets the app
    # populate (and dump) its AccessKit tree under test.
    [switch]$A11y,
    # Optional: post a WM_MOUSEMOVE at this fraction (0..1) of the client area,
    # to exercise the chart's pointer hover (tooltip). Negative = skip.
    [double]$MouseFracX = -1,
    [double]$MouseFracY = -1,
    # Optional: post a left click at this fraction (0..1) of the client area,
    # to exercise a clickable control (EXP25's clock toggle). Negative = skip.
    [double]$ClickFracX = -1,
    [double]$ClickFracY = -1
)

# The app is a GNU/MinGW build; its runtime DLLs live beside the toolchain, not
# next to the exe. Prepend the known location if it exists.
$mingw = Join-Path $env:LOCALAPPDATA 'mingw64\bin'
if (Test-Path $mingw) { $env:PATH = "$mingw;$env:PATH" }

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  public struct RECT { public int Left, Top, Right, Bottom; }
  public struct POINT { public int X, Y; }
}
"@

# Raise the app window above any overlapping window before we CopyFromScreen.
# SetForegroundWindow alone can silently fail when the calling process does not
# hold the OS foreground lock, which lets an editor/chat window cover the left
# of the capture region. A TOPMOST -> NOTOPMOST toggle raises us without that
# permission. (EXP09: the verdict chip lives in the header's left corner, so a
# stray window over that corner hid it from the pixel asserts.)
function Show-AppWindow($h) {
    if ($h -eq [IntPtr]::Zero) { return }
    [void][W]::ShowWindow($h, 9)                                  # SW_RESTORE
    [void][W]::SetWindowPos($h, [IntPtr](-1), 0, 0, 0, 0, 0x0003) # HWND_TOPMOST, NOMOVE|NOSIZE
    [void][W]::SetWindowPos($h, [IntPtr](-2), 0, 0, 0, 0, 0x0003) # HWND_NOTOPMOST
    [void][W]::SetForegroundWindow($h)
}

# Park the physical pointer in the top-left corner. The widget opens **centred**,
# so a mouse left anywhere near the middle of the screen ends up over it, and
# Windows then posts a real WM_MOUSEMOVE that raises the hover tooltip - whose
# panel pixels break the "dropdown is closed" assertion and whose crosshair
# breaks "no hover". Parked before the window opens, this is deterministic; the
# hover test is unaffected because it posts its own synthetic WM_MOUSEMOVE.
try {
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point 0, 0
} catch {}   # headless / no desktop: nothing to park

$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds $WaitSeconds
$p.Refresh()
if ($p.HasExited) {
    Write-Output "PROCESS_EXITED code=$($p.ExitCode)"
    exit 2
}

$h = $p.MainWindowHandle
Write-Output "HWND=$h"
if ($h -ne [IntPtr]::Zero) {
    Show-AppWindow $h
    Start-Sleep -Milliseconds 1000
}

if ($Type -ne "") {
    # Type by posting the messages straight to the window, so it works even when
    # the OS will not hand keyboard focus to the child process (SendKeys needs
    # the window to be the active window, which this harness cannot guarantee).
    Show-AppWindow $h
    Start-Sleep -Milliseconds 300
    foreach ($ch in $Type.ToCharArray()) {
        $code = [int][char]$ch
        $vk = [int][char]([char]::ToUpper($ch))
        [void][W]::PostMessage($h, 0x0100, [IntPtr]$vk, [IntPtr]0)   # WM_KEYDOWN
        [void][W]::PostMessage($h, 0x0102, [IntPtr]$code, [IntPtr]0) # WM_CHAR
        [void][W]::PostMessage($h, 0x0101, [IntPtr]$vk, [IntPtr]0)   # WM_KEYUP
        Start-Sleep -Milliseconds 80
    }
    Start-Sleep -Milliseconds 800
}

# Raise before pressing keys, and do not raise again afterwards: a later
# activation/raise can emit a mouse event that flips the app's input modality
# back to Mouse and hides a keyboard focus ring. EXP10.
if ($h -ne [IntPtr]::Zero) {
    Show-AppWindow $h
    Start-Sleep -Milliseconds 300
}

if ($Press -ne "") {
    # Post named keys (no modifiers) to the window. VK codes from WinUser.h.
    Show-AppWindow $h
    Start-Sleep -Milliseconds 300
    foreach ($tok in $Press.Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        $vk = switch ($tok) {
            "tab"   { 0x09 }
            "enter" { 0x0D }
            "space" { 0x20 }
            "esc"   { 0x1B }
            "up"    { 0x26 }
            "down"  { 0x28 }
            "home"  { 0x24 }
            "end"   { 0x23 }
            default { $null }
        }
        if ($null -ne $vk) {
            [void][W]::PostMessage($h, 0x0100, [IntPtr]$vk, [IntPtr]0)  # WM_KEYDOWN
            [void][W]::PostMessage($h, 0x0101, [IntPtr]$vk, [IntPtr]0)  # WM_KEYUP
            Start-Sleep -Milliseconds 150
        }
    }
    Start-Sleep -Milliseconds 500
}

if ($A11y) {
    # WM_GETOBJECT (0x003D) with lParam = UiaRootObjectId (-25): what a UIA
    # client sends to request the root provider, which activates AccessKit.
    Show-AppWindow $h
    Start-Sleep -Milliseconds 300
    [void][W]::PostMessage($h, 0x003D, [IntPtr]::Zero, [IntPtr](-25))
    Start-Sleep -Milliseconds 1200
}

if ($MouseFracX -ge 0 -and $MouseFracY -ge 0 -and $h -ne [IntPtr]::Zero) {
    # Post a mouse move straight to the window at a fraction of its client area.
    # lParam is client coordinates packed as (y << 16) | x; the backend reads
    # them directly, so this works without a physical pointer (EXP13 hover).
    Show-AppWindow $h
    Start-Sleep -Milliseconds 300
    $cr = New-Object W+RECT
    [W]::GetClientRect($h, [ref]$cr) | Out-Null
    $cw = $cr.Right - $cr.Left
    $ch = $cr.Bottom - $cr.Top
    $mx = [int]($MouseFracX * $cw)
    $my = [int]($MouseFracY * $ch)
    $lp = [IntPtr]((($my -band 0xFFFF) -shl 16) -bor ($mx -band 0xFFFF))
    [void][W]::PostMessage($h, 0x0200, [IntPtr]0, $lp)   # WM_MOUSEMOVE
    Start-Sleep -Milliseconds 600
}

if ($ClickFracX -ge 0 -and $ClickFracY -ge 0 -and $h -ne [IntPtr]::Zero) {
    # A click needs the pointer to be over the control first (the backend tracks
    # hover from WM_MOUSEMOVE), then WM_LBUTTONDOWN/UP with MK_LBUTTON set on the
    # move that precedes them, as Windows itself sends it.
    Show-AppWindow $h
    Start-Sleep -Milliseconds 300
    $cr = New-Object W+RECT
    [W]::GetClientRect($h, [ref]$cr) | Out-Null
    $cw = $cr.Right - $cr.Left
    $ch = $cr.Bottom - $cr.Top
    $mx = [int]($ClickFracX * $cw)
    $my = [int]($ClickFracY * $ch)
    $lp = [IntPtr]((($my -band 0xFFFF) -shl 16) -bor ($mx -band 0xFFFF))
    [void][W]::PostMessage($h, 0x0200, [IntPtr]0, $lp)         # WM_MOUSEMOVE
    Start-Sleep -Milliseconds 100
    [void][W]::PostMessage($h, 0x0201, [IntPtr]1, $lp)         # WM_LBUTTONDOWN (MK_LBUTTON)
    Start-Sleep -Milliseconds 80
    [void][W]::PostMessage($h, 0x0202, [IntPtr]0, $lp)         # WM_LBUTTONUP
    Start-Sleep -Milliseconds 600
}

$r = New-Object W+RECT
[W]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Output "RECT=($($r.Left),$($r.Top),$($r.Right - $r.Left),$($r.Bottom - $r.Top))"

# The window's own icon (EXP14): WM_SETICON stores it, WM_GETICON reads it back.
# Non-zero means the app installed its four-quadrant icon on the title bar.
if ($h -ne [IntPtr]::Zero) {
    $big = [W]::SendMessage($h, 0x007F, [IntPtr]1, [IntPtr]::Zero)   # WM_GETICON, ICON_BIG
    $small = [W]::SendMessage($h, 0x007F, [IntPtr]0, [IntPtr]::Zero) # WM_GETICON, ICON_SMALL
    Write-Output "ICON_BIG=$big"
    Write-Output "ICON_SMALL=$small"
}

# Capture only the **client area**: the widget's pixels live there. The OS title
# bar now carries the app icon (EXP14) and the frame carries borders, either of
# which would pollute the colour analysis every pixel assertion depends on.
$cr = New-Object W+RECT
[W]::GetClientRect($h, [ref]$cr) | Out-Null
$w = $cr.Right - $cr.Left; $hh = $cr.Bottom - $cr.Top
$origin = New-Object W+POINT
[W]::ClientToScreen($h, [ref]$origin) | Out-Null
$x = $origin.X; $y = $origin.Y
Write-Output "CLIENT=($x,$y,$w,$hh)"
if ($h -eq [IntPtr]::Zero -or $w -le 0 -or $hh -le 0) {
    # Fallback only; the E2E test treats an implausible rect as a failure.
    $x = 0; $y = 0
    $w = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width
    $hh = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height
    Write-Output "RECT_FALLBACK=1"
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
