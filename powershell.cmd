@echo off
set W64_BIN=C:\Users\keith\AppData\Local\w64devkit\w64devkit\bin
set W64_LIB=C:\Users\keith\AppData\Local\w64devkit\w64devkit\lib\gcc\x86_64-w64-mingw32\16.2.0;C:\Users\keith\AppData\Local\w64devkit\w64devkit\x86_64-w64-mingw32\lib
set PATH=%W64_BIN%;C:\Users\keith\.cargo\bin;%PATH%
set LIBRARY_PATH=%W64_LIB%;%LIBRARY_PATH%
set CARGO_TARGET_DIR=C:\Users\keith\AppData\Local\flowlocal_target
C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -ExecutionPolicy Bypass %*
