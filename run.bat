@echo off
title ForwardVI - run
rem Runs the game (build.bat and precompile.bat first). Does not build.
rem Everything happens in this window: the game's log is shown here as it is
rem written (and saved to the logs folder here).
rem
rem   run.bat                 asks which launch mode (Enter: 1080p)
rem   run.bat 1080p           1080p (the console reports a 1080p television)
rem   run.bat 4k              the console's 4K mode
rem   run.bat opti            OptiScaler: the title's Series S mode (1280x720
rem                           upscaled to 1080p) with its upscaler going to the PC
rem                           FidelityFX DLL that OptiScaler takes over, and
rem                           OptiScaler's frame generation. Needs an extracted
rem                           OptiScaler release in OPTISCALER_DIR (default: an
rem                           OptiScaler folder beside this one); OPTI_UPSCALER
rem                           picks its upscaler (dlss by default).
rem   run.bat 1080p "NAME=value NAME=value"   more switches, for a test run
rem
rem A new mode: a line in :menu, a line in :mode, and what it sets in :set_<mode>.
setlocal
cd /d "%~dp0"
call "%~dp0tools\common.cmd" || (pause & exit /b 1)
set "MODE=%~1"
set "EXTRA=%~2"
set "FROM_MENU="
if not defined MODE (
  set "FROM_MENU=1"
  call :menu
)
call :mode || (pause & exit /b 1)
rem From the menu, with a test save kept (save.bat keep): start from it?
set "FROM_SAVE="
if defined FROM_MENU if exist "%DATA%-test-save" set /p "FROM_SAVE=  Start from the test save? (Enter: yes, n: no): "
if defined FROM_MENU if exist "%DATA%-test-save" if /i not "%FROM_SAVE%"=="n" (
  call "%~dp0save.bat" restore || (pause & exit /b 1)
)
for /f %%t in ('powershell -NoProfile -Command "Get-Date -Format yyyy-MM-dd_HHmmss"') do set "STAMP=%%t"

rem The switches (shared with the nsight-*.bat files), then the mode's.
call "%~dp0tools\settings.cmd"
set "ARGS="
set VICTORY_DISPLAY_MODE=1920x1080
call :set_%MODE% || (pause & exit /b 1)

rem More switches "NAME=value NAME=value ...", for a test run (space between them).
set "TAG=%MODE%"
if defined EXTRA (
  set "TAG=%MODE%_test"
  call :extra
)
set "LOG=%LOGS%\%STAMP%_%TAG%.log"

rem What starts: the game, or a program that starts it (WRAP_EXE and WRAP_ARGS,
rem set by nsight-systems-frametime.bat).
set "LAUNCH_EXE=%EXE%"
set LAUNCH_ARGS=run "%GAME_EXE%" --execute %ARGS%
if not defined WRAP_EXE goto :launch
set "LAUNCH_EXE=%WRAP_EXE%"
set LAUNCH_ARGS=%WRAP_ARGS% "%EXE%" run "%GAME_EXE%" --execute %ARGS%
rem A wrapper keeps the game's output: the game writes its log to the file itself.
set "VICTORY_LOG_FILE=%LOG%"
:launch
echo.
echo == [%time%] Game: %GAME_EXE% ==
echo == [%time%] Mode %MODE%, display %VICTORY_DISPLAY_MODE%, card memory %VICTORY_CARD_MEMORY%, extra: %EXTRA% ==
echo == [%time%] Starting: "%LAUNCH_EXE%" %LAUNCH_ARGS% ==
echo == Log: %LOG% ==
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\launch.ps1"
echo.
echo == [%time%] The game ended. Log: %LOG% ==
pause
exit /b 0

rem Sets each NAME=value of EXTRA (spaces between them; "=" splits arguments in
rem cmd, so they are read off one by one with for /f, whose separator is space).
:extra
set "REST=%EXTRA%"
:extra_next
if not defined REST exit /b 0
set "ONE="
for /f "tokens=1*" %%a in ("%REST%") do (
  set "ONE=%%a"
  set "REST=%%b"
)
if not defined ONE exit /b 0
set "%ONE%"
echo == test switch: %ONE% ==
goto :extra_next

rem ---- The launch modes ----

:menu
echo.
echo   Launch mode:
echo     1  1080p   native 1080p (default)
echo     2  4k      the console's 4K mode
echo     3  opti    Series S 1280x720 upscaled to 1080p by OptiScaler, with frame generation
echo     4  test    1080p with switches you type (NAME=value, spaces between them)
echo.
set "PICK="
set /p "PICK=  Choose 1-4 (Enter: 1): "
if not defined PICK set PICK=1
if "%PICK%"=="1" set "MODE=1080p"
if "%PICK%"=="2" set "MODE=4k"
if "%PICK%"=="3" set "MODE=opti"
if "%PICK%"=="4" goto :menu_test
if not defined MODE goto :menu
exit /b 0
:menu_test
set "MODE=1080p"
set /p "EXTRA=  Switches (NAME=value, spaces between, e.g. VICTORY_RELAY=2 VICTORY_CARD_MEMORY=0): "
exit /b 0

rem Whether MODE is one of the modes.
:mode
for %%m in (1080p 4k opti) do if /i "%MODE%"=="%%m" exit /b 0
echo Unknown launch mode: %MODE% (1080p, 4k, opti)
exit /b 1

:set_1080p
set VICTORY_DISPLAY_MODE=1920x1080
exit /b 0

:set_4k
set VICTORY_DISPLAY_MODE=3840x2160
exit /b 0

:set_opti
if not defined OPTISCALER_DIR set "OPTISCALER_DIR=%~dp0..\OptiScaler"
if not defined OPTI_UPSCALER set OPTI_UPSCALER=dlss
if not exist "%OPTISCALER_DIR%\OptiScaler.dll" (
  echo No OptiScaler.dll in %OPTISCALER_DIR%.
  echo Extract an OptiScaler release there, or set OPTISCALER_DIR to its folder.
  exit /b 1
)
set "OPTI=%~dp0target\release\opti"
echo == [%time%] Setting up OptiScaler from %OPTISCALER_DIR% in %OPTI% ==
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\opti-setup.ps1" -From "%OPTISCALER_DIR%" -To "%OPTI%" -Exe "%EXE%" -Upscaler %OPTI_UPSCALER%
if errorlevel 1 (
  echo OptiScaler setup failed - see the messages above.
  exit /b 1
)
set "VICTORY_FSR_DLL=%OPTI%\amd_fidelityfx_upscaler_dx12.dll"
if not exist "%VICTORY_FSR_DLL%" set "VICTORY_FSR_DLL=%OPTI%\amd_fidelityfx_dx12.dll"
set "EXE=%OPTI%\victory.exe"
set "ARGS=--console series-s"
exit /b 0
