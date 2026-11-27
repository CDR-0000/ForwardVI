@echo off
title ForwardVI - precompile
rem Fills the shader cache ahead of time, so the game does not translate
rem shaders while you play (a few minutes, once per change of the translator),
rem then makes the driver's pipelines ahead of time.
rem
rem   precompile.bat         the pipelines earlier runs drew with (a few thousand)
rem   precompile.bat -full   every pipeline the game has created in a run so far
rem                          (tens of thousands: long, about 10-15 GB of the
rem                          driver's cache; those runs drew with first)
rem
rem run.bat writes down which pipelines the game creates and draws with; each
rem run adds to the list. Either way it goes on where it stopped if you close it.
rem Run build.bat first. Every step is shown; progress prints as it goes.
setlocal
cd /d "%~dp0"
set "WARM=--used"
set "WHICH=the pipelines earlier runs drew with (precompile.bat -full: every one the game created)"
if /i "%~1"=="-full" (
  set "WARM="
  set "WHICH=every pipeline the game has created, those runs drew with first"
) else if not "%~1"=="" (
  echo Unknown option: %~1   ^(precompile.bat, or precompile.bat -full^)
  pause
  exit /b 1
)
call "%~dp0tools\common.cmd" || (pause & exit /b 1)
rem The switches run.bat runs with, so the shaders and pipelines made here are
rem the ones the game asks for.
call "%~dp0tools\settings.cmd"
set VICTORY_LOG=info
rem What holds the game's shaders: its shader archive, the exe, the ray-tracing
rem structure builder and the upscaler, those of them the game folder has.
set "FILES="
for %%f in ("%G%\shaders.rpf" "%GAME_EXE%" "%G%\XboxBvhBuilder.dll" "%G%\amd_fidelityfx_upscaler_xbox.dll") do if exist "%%~f" call set FILES=%%FILES%% "%%~f"
echo == [%time%] 1/2 Translating every shader into the cache ==
echo on
"%EXE%" shaders %FILES%
@echo off
if errorlevel 1 goto :failed
echo.
echo == [%time%] 2/2 Making %WHICH% ==
echo    It goes on where it stopped if you close it.
echo on
"%EXE%" warm %WARM%
@echo off
if errorlevel 1 goto :failed
echo.
echo == [%time%] Precompile finished ==
pause
exit /b 0
:failed
echo.
echo == [%time%] PRECOMPILE FAILED - see the messages above ==
pause
exit /b 1
