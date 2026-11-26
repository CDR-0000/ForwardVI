# Starts the game in this window (run.bat): its log (standard error) goes to -Log,
# and each line is shown here as it is written, until the game ends. One window
# for the script, the log and its end; the game's own window opens as always.
#
#   launch.ps1 -Log <file> -Exe <program> -Arguments <its arguments>
#
# Without them it takes LOG, LAUNCH_EXE and LAUNCH_ARGS from the environment
# (run.bat sets those: quoted paths pass through cmd unharmed that way).
# -Exe may be another program that starts the game (Nsight Systems' nsys.exe).
# The log keeps the program's colour codes; what is shown here has them taken out.
param(
  [string]$Log = $env:LOG,
  [string]$Exe = $env:LAUNCH_EXE,
  [string]$Arguments = $env:LAUNCH_ARGS
)
if (-not $Log -or -not $Exe) { 'launch.ps1: no log or program given.'; exit 1 }
$ErrorActionPreference = 'Stop'
$game = Start-Process -FilePath $Exe -ArgumentList $Arguments -NoNewWindow -PassThru -RedirectStandardError $Log
# (Taken at once, so that the exit code can be read at the end.)
$null = $game.Handle
# Read while the game writes (its handle stays open for writing).
$reader = $null
while (-not $reader) {
  try {
    $stream = [IO.File]::Open($Log, 'Open', 'Read', 'ReadWrite')
    $reader = New-Object IO.StreamReader($stream)
  } catch {
    if ($game.HasExited) { break }
    Start-Sleep -Milliseconds 100
  }
}
$colours = [regex]"\x1b\[[0-9;]*m"
# Whole lines only: a line the game is still writing waits for its end.
$script:rest = ''
function Show-New([switch]$Last) {
  $text = $script:rest + $reader.ReadToEnd()
  $lines = $text -split "`r?`n"
  $script:rest = $lines[-1]
  # (0..-1 would count down: one element means no whole line yet.)
  if ($lines.Count -gt 1) {
    foreach ($line in $lines[0..($lines.Count - 2)]) { $colours.Replace($line, '') }
  }
  if ($Last -and $script:rest) { $colours.Replace($script:rest, ''); $script:rest = '' }
}
if ($reader) {
  while (-not $game.HasExited) {
    Show-New
    Start-Sleep -Milliseconds 200
  }
  Show-New -Last
  $reader.Close()
}
$game.WaitForExit()
exit $game.ExitCode
