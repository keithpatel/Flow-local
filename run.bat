@echo off
setlocal
title FlowLocal - Offline Voice Dictation

echo ====================================================================
echo                   FlowLocal: Offline Voice Dictation
echo ====================================================================
echo.

set W64_BIN=C:\Users\keith\AppData\Local\w64devkit\w64devkit\bin
set W64_LIB=C:\Users\keith\AppData\Local\w64devkit\w64devkit\lib\gcc\x86_64-w64-mingw32\16.2.0;C:\Users\keith\AppData\Local\w64devkit\w64devkit\x86_64-w64-mingw32\lib
set PATH=%W64_BIN%;C:\Users\keith\.cargo\bin;%PATH%
set LIBRARY_PATH=%W64_LIB%;%LIBRARY_PATH%
set CARGO_TARGET_DIR=C:\Users\keith\AppData\Local\flowlocal_target

if not exist "models\ggml-tiny.en.bin" (
    echo [Setup] Downloading Whisper tiny.en model...
    python scripts\setup_whisper_engine.py
)

echo [FlowLocal] Starting voice dictation engine...
echo --------------------------------------------------------------------
echo  [HOTKEY] Hold [Right Ctrl] to talk, release to transcribe & paste.
echo  [HANDS-FREE] Double-tap [Right Ctrl] or press [Right Ctrl + Space].
echo  [CANCEL] Press [Escape] anytime to cancel.
echo --------------------------------------------------------------------
echo.

cargo run --manifest-path src-tauri\Cargo.toml

pause
