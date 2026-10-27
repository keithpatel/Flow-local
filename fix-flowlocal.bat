@echo off
setlocal
title FlowLocal - Apply fix and reinstall
cd /d "%~dp0"

echo [1/4] Closing all running FlowLocal instances...
taskkill /IM flowlocal.exe /F >nul 2>&1
timeout /t 2 /nobreak >nul

set "W64=%LOCALAPPDATA%\w64devkit\w64devkit"
set "PATH=%W64%\bin;%USERPROFILE%\.cargo\bin;C:\Program Files\nodejs;%PATH%"
set "LIBRARY_PATH=%W64%\lib\gcc\x86_64-w64-mingw32\16.2.0;%W64%\x86_64-w64-mingw32\lib;%LIBRARY_PATH%"
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\flowlocal_target"

echo [2/4] Building FlowLocal - this can take several minutes...
call npx tauri build
if errorlevel 1 (
  echo.
  echo BUILD FAILED. See the error above.
  pause
  exit /b 1
)

set "SETUP=%CARGO_TARGET_DIR%\release\bundle\nsis\FlowLocal_0.1.0_x64-setup.exe"
if not exist "%SETUP%" (
  echo.
  echo Installer not found: %SETUP%
  pause
  exit /b 1
)

echo [3/4] Installing the new build...
start "" /wait "%SETUP%" /S

echo [4/4] Starting FlowLocal...
start "" "%LOCALAPPDATA%\FlowLocal\flowlocal.exe"

echo.
echo Done. FlowLocal is updated and running as a single instance.
echo You can close this window.
pause
exit /b 0
