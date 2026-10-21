# Read the icon resource embedded in the built executable (EXP14) and print the
# colour of each quadrant centre, so the E2E can assert the four-quadrant design
# is actually baked into the .exe (independent of the running window).
#
# Output lines:
#   ICON_COUNT=<n>            # icons in the file's resource table
#   ICON_SIZE=<w>x<h>
#   QUAD_TL=#rrggbb QUAD_TR=#rrggbb QUAD_BL=#rrggbb QUAD_BR=#rrggbb
param(
    [Parameter(Mandatory=$true)][string]$Exe
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Ico {
  [DllImport("shell32.dll", CharSet=CharSet.Unicode)] public static extern uint ExtractIconEx(string file, int index, IntPtr[] large, IntPtr[] small, uint n);
  [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr h);
}
"@

$large = New-Object IntPtr[] 1
$count = [Ico]::ExtractIconEx($Exe, 0, $large, $null, 1)
Write-Output "ICON_COUNT=$count"
if ($count -eq 0 -or $large[0] -eq [IntPtr]::Zero) {
    exit 1
}

$bmp = [System.Drawing.Icon]::FromHandle($large[0]).ToBitmap()
Write-Output "ICON_SIZE=$($bmp.Width)x$($bmp.Height)"

# Sample the centre of each quadrant (the layout is flush, so 25%/75% is always
# well inside a quadrant whatever size the shell hands back).
foreach ($q in @(@("TL", 0.25, 0.25), @("TR", 0.75, 0.25), @("BL", 0.25, 0.75), @("BR", 0.75, 0.75))) {
    $x = [int]($bmp.Width * $q[1])
    $y = [int]($bmp.Height * $q[2])
    $p = $bmp.GetPixel($x, $y)
    Write-Output ("QUAD_{0}=#{1:X2}{2:X2}{3:X2}" -f $q[0], $p.R, $p.G, $p.B)
}

$bmp.Dispose()
[void][Ico]::DestroyIcon($large[0])
