@echo off
title ForwardVI - build
rem Builds target\release\victory.exe, every step shown (cargo -v).
rem The first time (when victory.toml names no game yet) it asks for the game's
rem exe and writes its folder and name into victory.toml for the other .bat files.
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
echo == [%time%] Game ==
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\game.ps1" -Ask
if errorlevel 1 goto :failed
echo.
echo == [%time%] Building ForwardVI (release) in %cd% ==
echo on
cargo build --release -p victory -v
@echo off
if errorlevel 1 goto :failed
echo.
echo == [%time%] Build finished: %cd%\target\release\victory.exe ==
pause
exit /b 0
:failed
echo.
echo == [%time%] BUILD FAILED - see the messages above ==
pause
exit /b 1
