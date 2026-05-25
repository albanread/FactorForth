@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Professional\VC\Auxiliary\Build\vcvars64.bat" >/dev/null
dumpbin /exports E:\NewFactor\vm-build\factor.dll | findstr /R "nf_"
