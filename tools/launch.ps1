# Starts the game in this window (run.bat): its log goes to -Log, and each line
# is shown here as it is written, until the game ends. One window for the
# script, the log and its end; the game's own window opens as always.
#
#   launch.ps1 -Log <file> -Exe <program> -Arguments <its arguments>
#
# Without them it takes LOG, LAUNCH_EXE and LAUNCH_ARGS from the environment
# (run.bat sets those: quoted paths pass through cmd unharmed that way).
# -Exe may be another program that starts the game (Nsight Systems' nsys.exe).
# The game's standard error (its log) goes to -Log, standard output to the same
# name with .out before the extension; both are shown here. (A program that
# starts the game may pass the game's log on through either.) The files keep the
# program's colour codes; what is shown here has them taken out.
#
# Nsight Systems (nsight-systems-frametime.bat sets NSYS, NSYS_SESSION and
# NSYS_OUT): press R in this window to record 5 seconds into NSYS_OUT, as often
# as you like.
param(
  [string]$Log = $env:LOG,
  [string]$Exe = $env:LAUNCH_EXE,
  [string]$Arguments = $env:LAUNCH_ARGS
)
if (-not $Log -or -not $Exe) { 'launch.ps1: no log or program given.'; exit 1 }
$ErrorActionPreference = 'Stop'
$out = [IO.Path]::ChangeExtension($Log, '.out' + [IO.Path]::GetExtension($Log))
# When the game writes its log to the file itself (VICTORY_LOG_FILE, under a
# wrapper), standard error goes to a file of its own beside it.
$err = $Log
if ($env:VICTORY_LOG_FILE) { $err = [IO.Path]::ChangeExtension($Log, '.err' + [IO.Path]::GetExtension($Log)) }
$game = Start-Process -FilePath $Exe -ArgumentList $Arguments -NoNewWindow -PassThru -RedirectStandardError $err -RedirectStandardOutput $out
# (Taken at once, so that the exit code can be read at the end.)
$null = $game.Handle
$colours = [regex]"\x1b\[[0-9;]*m"

# One file being written, read as it grows: whole lines only (a line still
# being written waits for its end).
function New-Tail([string]$path) {
  @{ Path = $path; Reader = $null; Rest = '' }
}
function Show-Tail($tail, [switch]$Last) {
  if (-not $tail.Reader) {
    try {
      $stream = [IO.File]::Open($tail.Path, 'Open', 'Read', 'ReadWrite')
      $tail.Reader = New-Object IO.StreamReader($stream)
    } catch { return }
  }
  $text = $tail.Rest + $tail.Reader.ReadToEnd()
  $lines = $text -split "`r?`n"
  $tail.Rest = $lines[-1]
  # (0..-1 would count down: one element means no whole line yet.)
  if ($lines.Count -gt 1) {
    foreach ($line in $lines[0..($lines.Count - 2)]) { $colours.Replace($line, '') }
  }
  if ($Last -and $tail.Rest) { $colours.Replace($tail.Rest, ''); $tail.Rest = '' }
}
$tails = @((New-Tail $Log), (New-Tail $out))
if ($err -ne $Log) { $tails += New-Tail $err }

# Nsight Systems: R records 5 seconds.
$nsys = $env:NSYS; $session = $env:NSYS_SESSION
$recording = $false; $stopAt = $null; $takes = 0
$stamp = Get-Date -Format 'yyyy-MM-dd_HHmmss'
function Test-Key {
  try { if (-not [Console]::KeyAvailable) { return $null }; [Console]::ReadKey($true).KeyChar } catch { $null }
}
if ($nsys -and $session) {
  ''
  '== Nsight Systems: press R in this window to record 5 seconds (as often as you like) =='
  ''
}

while (-not $game.HasExited) {
  foreach ($t in $tails) { Show-Tail $t }
  if ($nsys -and $session) {
    $key = Test-Key
    if (-not $recording -and $key -and "$key" -ieq 'r') {
      $takes++
      $report = Join-Path $env:NSYS_OUT "$stamp`_$takes"
      "== [$(Get-Date -Format T)] Recording 5 seconds: $report.nsys-rep =="
      & $nsys start --session=$session --sample=process-tree --cpuctxsw=process-tree --gpu-metrics-devices=all --output="$report" --force-overwrite=true 2>&1 | ForEach-Object { "   nsys: $_" }
      $recording = $true; $stopAt = (Get-Date).AddSeconds(5)
    }
    if ($recording -and (Get-Date) -ge $stopAt) {
      "== [$(Get-Date -Format T)] Stopping; Nsight writes the report (a few seconds) =="
      & $nsys stop --session=$session 2>&1 | ForEach-Object { "   nsys: $_" }
      $recording = $false
      "== [$(Get-Date -Format T)] Done. Press R to record again =="
    }
  }
  Start-Sleep -Milliseconds 200
}
foreach ($t in $tails) { Show-Tail $t -Last; if ($t.Reader) { $t.Reader.Close() } }
$game.WaitForExit()
exit $game.ExitCode
