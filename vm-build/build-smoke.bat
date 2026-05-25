@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
echo === compiling smoke.c ===
cl /nologo /W3 /MD smoke.c factor.dll.lib /Fe:smoke.exe
echo === build exit: %ERRORLEVEL% ===
