@echo off
cd /d E:\NewFactor\vm-build
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
echo === Cwd / PATH-check ===
cd
echo === nmake check ===
where nmake.exe
echo === Starting nmake build ===
nmake /f Nmakefile factor.dll.lib
echo === Build exit: %ERRORLEVEL% ===
