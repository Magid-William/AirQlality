# EXP08 data generator.
#
# Downloads GeoNames `cities15000` (+ country names) and writes the committed
# assets the app bakes in:
#   core/assets/cities.json     - [{id,name,ascii,country,lat,lon,tz,pop}, ...]
#   core/assets/countries.json  - { "EG": "Egypt", ... }
#
# GeoNames data is CC BY 4.0 (https://www.geonames.org/). Re-running is safe and
# deterministic for a given dump; the generated JSON is what gets committed.
#
# Usage:  pwsh -File experiments/EXP08/make_cities.ps1

$ErrorActionPreference = "Stop"
$inv = [System.Globalization.CultureInfo]::InvariantCulture

$exp   = Split-Path -Parent $MyInvocation.MyCommand.Path
$root  = (Resolve-Path (Join-Path $exp "..\..")).Path
$assets = Join-Path $root "core\assets"
$tmp   = Join-Path $exp "tmp"
New-Item -ItemType Directory -Force -Path $assets, $tmp | Out-Null

function Get-Dump($name) {
    $zip = Join-Path $tmp "$name.zip"
    if (-not (Test-Path $zip)) {
        Invoke-WebRequest -Uri "http://download.geonames.org/export/dump/$name.zip" -OutFile $zip -UseBasicParsing
    }
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    return (Join-Path $tmp "$name.txt")
}

function Esc([string]$s) {
    if ($null -eq $s) { return "" }
    $s = $s.Replace('\', '\\').Replace('"', '\"')
    $s = $s.Replace("`t", '\t').Replace("`n", '\n').Replace("`r", '\r')
    return $s
}

# --- countries: ISO alpha2 -> name ---
$countryInfo = Join-Path $tmp "countryInfo.txt"
if (-not (Test-Path $countryInfo)) {
    Invoke-WebRequest -Uri "http://download.geonames.org/export/dump/countryInfo.txt" -OutFile $countryInfo -UseBasicParsing
}
$cJson = New-Object System.Text.StringBuilder
[void]$cJson.Append("{")
$first = $true
foreach ($line in Get-Content $countryInfo) {
    if ($line.StartsWith("#") -or $line.Trim().Length -eq 0) { continue }
    $f = $line.Split("`t")
    if ($f.Count -lt 5) { continue }
    $iso = $f[0]; $name = $f[4]
    if (-not $first) { [void]$cJson.Append(",") }
    $first = $false
    [void]$cJson.Append('"' + (Esc $iso) + '":"' + (Esc $name) + '"')
}
[void]$cJson.Append("}")
$countriesOut = Join-Path $assets "countries.json"
[System.IO.File]::WriteAllText($countriesOut, $cJson.ToString())

# --- cities ---
$citiesTxt = Get-Dump "cities15000"
$sb = New-Object System.Text.StringBuilder
[void]$sb.Append("[")
$first = $true
$n = 0
foreach ($line in Get-Content $citiesTxt) {
    if ($line.Trim().Length -eq 0) { continue }
    $f = $line.Split("`t")
    if ($f.Count -lt 18) { continue }
    $id   = $f[0]
    $name = $f[1]
    $ascii= $f[2]
    $lat  = [double]::Parse($f[4], $inv)
    $lon  = [double]::Parse($f[5], $inv)
    $cc   = $f[8]
    $pop  = 0; [void][int64]::TryParse($f[14], [ref]$pop)
    $tz   = $f[17]
    if ($tz.Trim().Length -eq 0) { continue }
    if (-not $first) { [void]$sb.Append(",") }
    $first = $false
    $latS = $lat.ToString("0.#####", $inv)
    $lonS = $lon.ToString("0.#####", $inv)
    [void]$sb.Append("{")
    [void]$sb.Append('"id":' + $id + ',')
    [void]$sb.Append('"name":"' + (Esc $name) + '",')
    [void]$sb.Append('"ascii":"' + (Esc $ascii) + '",')
    [void]$sb.Append('"country":"' + (Esc $cc) + '",')
    [void]$sb.Append('"lat":' + $latS + ',')
    [void]$sb.Append('"lon":' + $lonS + ',')
    [void]$sb.Append('"tz":"' + (Esc $tz) + '",')
    [void]$sb.Append('"pop":' + $pop)
    [void]$sb.Append("}")
    $n++
}
[void]$sb.Append("]")
$citiesOut = Join-Path $assets "cities.json"
[System.IO.File]::WriteAllText($citiesOut, $sb.ToString())

$ci = (Get-Item $citiesOut).Length
$co = (Get-Item $countriesOut).Length
Write-Output "wrote $citiesOut ($n cities, $([math]::Round($ci/1MB,2)) MB)"
Write-Output "wrote $countriesOut ($([math]::Round($co/1KB,1)) KB)"
