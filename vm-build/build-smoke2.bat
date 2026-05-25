@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
echo === compile smoke2.c ===
cl /nologo /W3 /MD smoke2.c /Fe:smoke2.exe
echo === build exit: %ERRORLEVEL% ===
echo === RUNNING smoke2.exe ===
.\smoke2.exe
echo === SMOKE_EXIT=%ERRORLEVEL% ===
