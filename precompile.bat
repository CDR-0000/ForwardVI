@echo off
title ForwardVI - precompile
rem Fills the shader cache ahead of time, so the game does not translate
rem shaders while you play (a few minutes, once per change of the translator),
rem then makes the driver's pipelines of every pipeline the game has created
rem in a run so far (run.bat writes them down), those it drew with first.
rem Run build.bat first. Every step is shown; progress prints as it goes.
setlocal
cd /d "%~dp0"
call "%~dp0tools\common.cmd" || (pause & exit /b 1)
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
echo == [%time%] 2/2 Making every pipeline the game has created (those runs drew with first) ==
echo    Long the first time; it goes on where it stopped if you close it.
echo on
"%EXE%" warm
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
