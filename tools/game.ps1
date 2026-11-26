# Where the game is, for the .bat files: [paths] game_dir and game_exe in
# victory.toml (made from victory.example.toml if there is none).
#
#   game.ps1 -Ask   (build.bat) if victory.toml names no game folder and exe that
#                   exist, asks for the exe's full path, splits it into the
#                   folder and the file name and writes both into victory.toml.
#   game.ps1 -Env   (the other .bat files) prints `set` lines for them:
#                   G (the game folder), GAME_EXE (the exe's full path), LOGS
#                   (where run logs go) and NSIGHT_CACHE (the Nsight runs' cache);
#                   GAME_MISSING=1 if the game is not set yet.
#
# VICTORY_GAME_DIR / VICTORY_GAME_EXE, VICTORY_CACHE_DIR override victory.toml, as
# they do for the program itself. Logs go to the logs folder here
# (VICTORY_LOGS_DIR overrides), the Nsight runs' cache to "cache-nsight" beside the cache folder.
param([switch]$Ask, [switch]$Env)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$toml = Join-Path $root 'victory.toml'

function Read-Paths {
  $values = @{}
  if (Test-Path -LiteralPath $toml) {
    $section = ''
    foreach ($line in Get-Content -LiteralPath $toml) {
      if ($line -match '^\s*\[(.+?)\]\s*$') { $section = $Matches[1]; continue }
      if ($section -ne 'paths') { continue }
      if ($line -match "^\s*(\w+)\s*=\s*'([^']*)'") { $values[$Matches[1]] = $Matches[2] }
      elseif ($line -match '^\s*(\w+)\s*=\s*"((?:[^"\\]|\\.)*)"') { $values[$Matches[1]] = ($Matches[2] -replace '\\\\', '\') }
    }
  }
  foreach ($pair in @(@('game_dir', 'VICTORY_GAME_DIR'), @('game_exe', 'VICTORY_GAME_EXE'), @('cache_dir', 'VICTORY_CACHE_DIR'))) {
    $from = [Environment]::GetEnvironmentVariable($pair[1])
    if ($from) { $values[$pair[0]] = $from }
  }
  $values
}

# The game's exe, if victory.toml (or the environment) names one that exists.
function Get-GameExe($values) {
  $dir = $values['game_dir']; $exe = $values['game_exe']
  if (-not $dir -or -not $exe) { return $null }
  $path = Join-Path $dir $exe
  if (Test-Path -LiteralPath $path -PathType Leaf) { return $path }
  $null
}

# Sets `key = 'value'` in [paths], in place of the line there (commented out or
# not) or as a new line under [paths].
function Set-PathValue([string[]]$lines, [string]$key, [string]$value) {
  $out = New-Object System.Collections.Generic.List[string]
  $section = ''; $done = $false; $pathsAt = -1
  foreach ($line in $lines) {
    if ($line -match '^\s*\[(.+?)\]\s*$') { $section = $Matches[1] }
    if ($section -eq 'paths' -and -not $done -and $line -match ('^\s*#?\s*' + $key + '\s*=')) {
      $out.Add("$key = '$value'"); $done = $true; continue
    }
    $out.Add($line)
    if ($line -match '^\s*\[paths\]\s*$') { $pathsAt = $out.Count }
  }
  if (-not $done) {
    if ($pathsAt -lt 0) { $out.Add('[paths]'); $out.Add("$key = '$value'") }
    else { $out.Insert($pathsAt, "$key = '$value'") }
  }
  , $out.ToArray()
}

if ($Ask) {
  $values = Read-Paths
  $found = Get-GameExe $values
  if ($found) { "Game: $found"; exit 0 }
  while ($true) {
    $answer = Read-Host 'Please enter complete path of folder including the exe (for example: D:/folder/app.exe)'
    $answer = $answer.Trim().Trim('"').Trim("'") -replace '/', '\'
    if (-not $answer) { continue }
    if (-not (Test-Path -LiteralPath $answer -PathType Leaf)) { "Not found: $answer"; continue }
    if ([IO.Path]::GetExtension($answer) -ne '.exe') { "Not an .exe: $answer"; continue }
    if ($answer.Contains("'")) { "A path with ' in it cannot go into victory.toml as it is; move the game to another folder."; continue }
    break
  }
  $full = (Resolve-Path -LiteralPath $answer).Path
  $dir = Split-Path -Parent $full
  $exe = Split-Path -Leaf $full
  if (-not (Test-Path -LiteralPath $toml)) {
    $example = Join-Path $root 'victory.example.toml'
    $lines = if (Test-Path -LiteralPath $example) { Get-Content -LiteralPath $example } else { @('[paths]') }
  } else {
    $lines = Get-Content -LiteralPath $toml
  }
  $lines = Set-PathValue $lines 'game_dir' $dir
  $lines = Set-PathValue $lines 'game_exe' $exe
  # UTF-8 without a byte-order mark (Windows PowerShell's utf8 writes one).
  [IO.File]::WriteAllLines($toml, [string[]]$lines, (New-Object Text.UTF8Encoding $false))
  "Game folder: $dir"
  "Game exe:    $exe"
  "Saved in $toml"
  exit 0
}

if ($Env) {
  $values = Read-Paths
  $found = Get-GameExe $values
  if (-not $found) { 'set "GAME_MISSING=1"'; exit 0 }
  $cache = $values['cache_dir']
  if (-not $cache) { $cache = Join-Path $env:LOCALAPPDATA 'victory\cache' }
  $base = Split-Path -Parent $cache
  # Logs in this folder's logs (git ignores it), unless VICTORY_LOGS_DIR says.
  $logs = if ($env:VICTORY_LOGS_DIR) { $env:VICTORY_LOGS_DIR } else { Join-Path $root 'logs' }
  "set ""G=$($values['game_dir'].TrimEnd('\'))"""
  "set ""GAME_EXE=$found"""
  "set ""LOGS=$logs"""
  "set ""NSIGHT_CACHE=$(Join-Path $base 'cache-nsight')"""
  exit 0
}
'Use -Ask or -Env.'
exit 1
