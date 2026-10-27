@echo off
setlocal
title FlowLocal - Commit and push the fix
cd /d "%~dp0"
set "PATH=C:\Program Files\Git\cmd;%PATH%"

echo === Current changes ===
git status --short
echo.

echo === Staging the single-instance fix ===
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs
if errorlevel 1 (
  echo Could not stage files. See message above.
  pause
  exit /b 1
)

echo === Committing ===
git commit -m "Enforce single app instance to prevent duplicate dictation pastes"
if errorlevel 1 (
  echo.
  echo Commit failed - see the message above.
  pause
  exit /b 1
)

echo === Pushing to origin/master ===
git push origin master
if errorlevel 1 (
  echo.
  echo Push failed - see the message above.
  pause
  exit /b 1
)

echo.
echo Done. The fix is committed and pushed to GitHub.
pause
exit /b 0
