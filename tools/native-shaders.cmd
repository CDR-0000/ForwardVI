@echo off
rem Compiles the hand-written shaders (native\<hash>.<name>.hlsl) to DXIL in
rem target\native\<hash>.dxil, where VICTORY_NATIVE_SHADERS=1 finds them
rem (native\README.md). The first 16 characters of a file's name are the hash
rem a run's log names the console program by. Uses the newest Windows SDK's
rem dxc.exe (DXC set beforehand wins); its dxil.dll signs the output, as
rem D3D12 requires. Every command is shown. build.bat calls this.
setlocal EnableDelayedExpansion
set "ROOT=%~dp0.."
for %%f in ("%ROOT%") do set "ROOT=%%~ff"
if not exist "%ROOT%\native\*.hlsl" (
  echo No hand-written shaders in %ROOT%\native.
  exit /b 0
)
if not defined DXC for /d %%d in ("%ProgramFiles(x86)%\Windows Kits\10\bin\10.*") do if exist "%%d\x64\dxc.exe" set "DXC=%%d\x64\dxc.exe"
if not defined DXC (
  echo No dxc.exe: install the Windows SDK, or set DXC to a dxc.exe.
  exit /b 1
)
set "OUT=%ROOT%\target\native"
if not exist "%OUT%" mkdir "%OUT%"
echo == [%time%] Hand-written shaders, with %DXC% ==
set "FAILED="
for %%s in ("%ROOT%\native\*.hlsl") do (
  set "NAME=%%~ns"
  set "HASH=!NAME:~0,16!"
  echo on
  "%DXC%" -T cs_6_6 -E main -HV 2021 -O3 -Fo "%OUT%\!HASH!.dxil" "%%~fs"
  @echo off
  if errorlevel 1 (
    echo    FAILED: %%~nxs
    set "FAILED=1"
  ) else (
    echo    %%~nxs -^> %OUT%\!HASH!.dxil
  )
)
if defined FAILED exit /b 1
exit /b 0
