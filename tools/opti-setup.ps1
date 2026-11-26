# Sets up OptiScaler beside a copy of victory.exe (for next-run-opti.bat): the
# release's DLLs and folders copied to -To, OptiScaler.dll as dxgi.dll, and its
# ini with the upscaler and frame generation on. Adapted from
# zm0kiemedia/Neptune_VI tools/play-zm.ps1 (Initialize-Opti), GPL-3.0-or-later.
param(
  [Parameter(Mandatory)] [string]$From,
  [Parameter(Mandatory)] [string]$To,
  [Parameter(Mandatory)] [string]$Exe,
  [string]$Upscaler = 'dlss'
)
$ErrorActionPreference = 'Stop'

function Set-IniValue([string]$Text, [string]$Section, [string]$Key, [string]$Value) {
  $lines = $Text -split "`r?`n"
  $in = $false; $done = $false
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^\[(.+)\]\s*$') { $in = ($Matches[1] -eq $Section); continue }
    if ($in -and -not $done -and $lines[$i] -match ('^' + [regex]::Escape($Key) + '\s*=')) { $lines[$i] = "$Key=$Value"; $done = $true }
  }
  if (-not $done) { Write-Warning "OptiScaler.ini: no $Key in [$Section]" }
  $lines -join "`r`n"
}

New-Item -ItemType Directory -Force $To | Out-Null
$stamp = Join-Path $To 'optiscaler.from'
$source = (Get-Item (Join-Path $From 'OptiScaler.dll')).LastWriteTimeUtc.Ticks.ToString() + ' ' + $From
if (-not (Test-Path $stamp) -or (Get-Content $stamp -Raw).Trim() -ne $source) {
  Get-ChildItem $From -File | Where-Object { $_.Extension -in '.dll', '.ini' } | Copy-Item -Destination $To -Force
  Get-ChildItem $From -Directory | Where-Object { $_.Name -ne 'Licenses' } | Copy-Item -Destination $To -Recurse -Force
  Copy-Item (Join-Path $From 'OptiScaler.dll') (Join-Path $To 'dxgi.dll') -Force
  Remove-Item (Join-Path $To 'OptiScaler.dll') -Force -ErrorAction SilentlyContinue
  Set-Content $stamp $source
}
$ini = Get-Content (Join-Path $From 'OptiScaler.ini') -Raw
$ini = Set-IniValue $ini 'Upscalers' 'Dx12Upscaler' $Upscaler
$ini = Set-IniValue $ini 'FrameGen' 'Enabled' 'true'
$ini = Set-IniValue $ini 'FrameGen' 'FGInput' 'upscaler'
$ini = Set-IniValue $ini 'FrameGen' 'FGOutput' 'fsrfg'
$ini = Set-IniValue $ini 'FSRFG' 'AllowAsync' 'true'       # the present does not wait for the generated frame's turn
$ini = Set-IniValue $ini 'OptiFG' 'DisableHUDFix' 'true'   # its tracking of every draw cost 5 fps (Neptune_VI)
Set-Content (Join-Path $To 'OptiScaler.ini') $ini -NoNewline
# The program, copied whenever it was built again.
$copy = Join-Path $To (Split-Path $Exe -Leaf)
if (-not (Test-Path $copy) -or (Get-Item $copy).LastWriteTimeUtc -ne (Get-Item $Exe).LastWriteTimeUtc -or (Get-Item $copy).Length -ne (Get-Item $Exe).Length) {
  Copy-Item $Exe $copy -Force
}
# The Agility SDK runtime, where there is one.
$agility = Join-Path (Split-Path $Exe -Parent) 'D3D12'
if (Test-Path $agility) { Copy-Item $agility $To -Recurse -Force }
