@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat"
echo === post-vcvars ===
where cl.exe
where link.exe
where nmake.exe
where ml64.exe
where rc.exe
where mt.exe
cl.exe 2>&1 | head -1
