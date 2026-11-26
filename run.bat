@echo off
title ForwardVI - run
rem Runs the game (build.bat and precompile.bat first). Does not build.
rem
rem   run.bat        1080p (the console reports a 1080p television)
rem   run.bat 4k     the console's 4K mode
rem   run.bat 1080p "VICTORY_CARD_FILLS=1"   one more switch, for a test run
rem   run.bat opti   OptiScaler: the title's Series S mode (1280x720 upscaled to
rem                  1080p) with its upscaler going to the PC FidelityFX DLL that
rem                  OptiScaler takes over, and OptiScaler's frame generation.
rem                  Needs an extracted OptiScaler release in OPTISCALER_DIR
rem                  (default: an OptiScaler folder beside this one);
rem                  OPTI_UPSCALER picks its upscaler (dlss by default).
rem
rem The game's folder and exe come from victory.toml (build.bat asks for them).
rem The log goes to the traces folder beside the cache folder; a second window
rem shows it as it is written (the game itself writes only to the file).
setlocal
cd /d "%~dp0"
call "%~dp0tools\common.cmd" || (pause & exit /b 1)
set "MODE=%~1"
if not defined MODE set MODE=1080p
for /f %%t in ('powershell -NoProfile -Command "Get-Date -Format yyyy-MM-dd_HHmmss"') do set "STAMP=%%t"

rem The switches (shared with the nsight-*.bat files).
call "%~dp0tools\settings.cmd"
set "ARGS="

if /i "%MODE%"=="4k" (
  set VICTORY_DISPLAY_MODE=3840x2160
) else (
  set VICTORY_DISPLAY_MODE=1920x1080
)
if /i "%MODE%"=="opti" call :opti || exit /b 1

rem A second argument "NAME=value" (in quotes) sets one more switch for a test
rem run, e.g.   run.bat 1080p "VICTORY_CARD_FILLS=1"
set "TAG=%MODE%"
if not "%~2"=="" (
  set "%~2"
  for /f "tokens=1,2 delims==" %%a in ("%~2") do set "TAG=%MODE%_%%a-%%b"
)
set "LOG=%LOGS%\%STAMP%_%TAG%.log"
echo == [%time%] Game: %GAME_EXE% ==
echo == [%time%] Mode %MODE%, display %VICTORY_DISPLAY_MODE%, card memory %VICTORY_CARD_MEMORY%, extra: %~2 ==
echo == Log: %LOG% ==
type nul > "%LOG%"
start "ForwardVI log" powershell -NoProfile -Command "Get-Content -LiteralPath '%LOG%' -Wait -Tail 50"
echo on
%VICTORY_LAUNCH% "%EXE%" run "%GAME_EXE%" --execute %ARGS% 2> "%LOG%"
@echo off
echo.
echo == [%time%] The game ended. Log: %LOG% ==
pause
exit /b 0

:opti
if not defined OPTISCALER_DIR set "OPTISCALER_DIR=%~dp0..\OptiScaler"
if not defined OPTI_UPSCALER set OPTI_UPSCALER=dlss
if not exist "%OPTISCALER_DIR%\OptiScaler.dll" (
  echo No OptiScaler.dll in %OPTISCALER_DIR%.
  echo Extract an OptiScaler release there, or set OPTISCALER_DIR to its folder.
  pause
  exit /b 1
)
set "OPTI=%~dp0target\release\opti"
echo == [%time%] Setting up OptiScaler from %OPTISCALER_DIR% in %OPTI% ==
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\opti-setup.ps1" -From "%OPTISCALER_DIR%" -To "%OPTI%" -Exe "%EXE%" -Upscaler %OPTI_UPSCALER%
if errorlevel 1 (
  echo OptiScaler setup failed - see the messages above.
  pause
  exit /b 1
)
set "VICTORY_FSR_DLL=%OPTI%\amd_fidelityfx_upscaler_dx12.dll"
if not exist "%VICTORY_FSR_DLL%" set "VICTORY_FSR_DLL=%OPTI%\amd_fidelityfx_dx12.dll"
set "EXE=%OPTI%\victory.exe"
set "ARGS=--console series-s"
exit /b 0
