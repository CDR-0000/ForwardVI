@echo off
rem What every .bat that starts the game needs first (call it, then check
rem errorlevel): the game's paths from victory.toml (tools\game.ps1: G, GAME_EXE,
rem LOGS, NSIGHT_CACHE), and EXE, the built victory.exe, which must not be
rem older than the code. Says what to do when something is missing.
set "EXE=%~dp0..\target\release\victory.exe"
for %%f in ("%EXE%") do set "EXE=%%~ff"
set "GAME_MISSING="
for /f "delims=" %%l in ('powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0game.ps1" -Env') do %%l
if defined GAME_MISSING (
  echo The game's folder and exe are not set: run build.bat, which asks for them.
  exit /b 1
)
if not exist "%EXE%" (
  echo No %EXE% - run build.bat first.
  exit /b 1
)
rem Built before the last commit that changed the program's code (not the
rem scripts or the documents): build.bat first. (Without git it is skipped.)
powershell -NoProfile -Command "try { $c = [datetime](git -C '%~dp0..' log -1 --format=%%cI -- apps crates xtask Cargo.toml Cargo.lock 2>$null) } catch { exit 0 }; if ((Get-Item -LiteralPath '%EXE%').LastWriteTime -lt $c) { exit 1 }"
if errorlevel 1 (
  echo victory.exe is older than the code - run build.bat first.
  exit /b 1
)
if not exist "%LOGS%" mkdir "%LOGS%"
exit /b 0
