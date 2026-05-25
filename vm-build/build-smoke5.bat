@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
cl /nologo /W3 /MD smoke5.c /Fe:smoke5.exe
echo === RUN ===
.\smoke5.exe
echo === SMOKE5_EXIT=%ERRORLEVEL% ===
