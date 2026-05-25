@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
cl /nologo /W3 /MD smoke4.c /Fe:smoke4.exe
echo === RUN ===
.\smoke4.exe
echo === SMOKE4_EXIT=%ERRORLEVEL% ===
