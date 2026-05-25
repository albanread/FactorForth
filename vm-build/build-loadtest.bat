@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
echo === compile loadtest.c ===
cl /nologo loadtest.c
echo === try run loadtest ===
.\loadtest.exe
echo === LOADTEST_EXIT=%ERRORLEVEL% ===
