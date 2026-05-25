@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
cl /nologo /W3 /MD smoke6.c /Fe:smoke6.exe
echo === RUN smoke6 ===
.\smoke6.exe
echo === SMOKE6_EXIT=%ERRORLEVEL% ===
