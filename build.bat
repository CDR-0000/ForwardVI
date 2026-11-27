@echo off
title ForwardVI - build
rem Builds target\release\victory.exe, every step shown (cargo -v).
rem The first time (when victory.toml names no game yet) it asks for the game's
rem exe and writes its folder and name into victory.toml for the other .bat files.
rem
rem   build.bat        the release build, what run.bat runs
rem   build.bat diag   the diagnostics build, target\diag\victory.exe (own folder:
rem                    the release build stays as it is); run.bat's mode 5 runs it
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set "PROFILE=release"
set "CARGO_ARGS=--release -p victory -v"
if /i "%~1"=="diag" (
  set "PROFILE=diag"
  set "CARGO_ARGS=--profile diag -p victory --features diagnostics -v"
) else if not "%~1"=="" (
  echo Unknown option: %~1   ^(build.bat, or build.bat diag^)
  pause
  exit /b 1
)
echo == [%time%] Game ==
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\game.ps1" -Ask
if errorlevel 1 goto :failed
rem The diagnostics build asks for the D3D12 Agility SDK runtime beside it
rem (target\diag\D3D12\D3D12Core.dll; without it no device is made): fetched
rem once from nuget.org, Microsoft's package at a pinned version, its SHA-512
rem checked (cargo xtask agility). For this build alone: the release build
rem keeps Windows' own D3D12.
if /i "%PROFILE%"=="diag" if not exist "%cd%\target\diag\D3D12\D3D12Core.dll" (
  echo.
  echo == [%time%] D3D12 Agility SDK runtime for the diagnostics build ==
  echo on
  cargo xtask agility --profile diag
  @echo off
  if errorlevel 1 goto :failed
)
echo.
echo == [%time%] Building ForwardVI (%PROFILE%) in %cd% ==
echo on
cargo build %CARGO_ARGS%
@echo off
if errorlevel 1 goto :failed
echo.
echo == [%time%] Build finished: %cd%\target\%PROFILE%\victory.exe ==
pause
exit /b 0
:failed
echo.
echo == [%time%] BUILD FAILED - see the messages above ==
pause
exit /b 1
