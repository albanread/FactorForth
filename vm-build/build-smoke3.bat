@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
cl /nologo /W3 /MD smoke3.c /Fe:smoke3.exe
echo === build exit: %ERRORLEVEL% ===
echo.
echo === RUNNING smoke3 (noop startup quot) ===
.\smoke3.exe
echo === SMOKE3_EXIT=%ERRORLEVEL% ===
