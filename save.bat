@echo off
title ForwardVI - test save
rem One save to test from: kept once, put back before each test run, so every
rem run starts at the same place in the game. The test save lives beside the
rem save folder (victory.toml's data_dir), in a folder ending "-test-save".
rem
rem   save.bat keep      the saves as they are now become the test save
rem   save.bat restore   the test save replaces the saves (before a test run)
rem
rem The title's temp folder (crash dumps) is left out and left alone.
setlocal
cd /d "%~dp0"
set "GAME_MISSING="
set "DATA="
for /f "delims=" %%l in ('powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\game.ps1" -Env') do %%l
if defined GAME_MISSING (
  echo The game's folder and exe are not set: run build.bat, which asks for them.
  pause
  exit /b 1
)
set "KEPT=%DATA%-test-save"
if /i "%~1"=="keep" goto :keep
if /i "%~1"=="restore" goto :restore
echo   save.bat keep      the saves as they are now become the test save
echo   save.bat restore   the test save replaces the saves
echo.
echo   Saves:     %DATA%
echo   Test save: %KEPT%
if not exist "%KEPT%" echo   (no test save kept yet)
pause
exit /b 1

:keep
if not exist "%DATA%" (
  echo No saves in %DATA% yet: play once first.
  pause
  exit /b 1
)
echo == [%time%] Keeping %DATA% as the test save in %KEPT% ==
robocopy "%DATA%" "%KEPT%" /MIR /XD temp /NJH /NP
if errorlevel 8 goto :failed
echo == [%time%] Test save kept ==
exit /b 0

:restore
if not exist "%KEPT%" (
  echo No test save in %KEPT%: run save.bat keep first.
  pause
  exit /b 1
)
echo == [%time%] Putting the test save back into %DATA% ==
robocopy "%KEPT%" "%DATA%" /MIR /XD temp /NJH /NP
if errorlevel 8 goto :failed
echo == [%time%] Test save restored ==
exit /b 0

:failed
echo.
echo == [%time%] Copying failed - see the messages above ==
pause
exit /b 1
