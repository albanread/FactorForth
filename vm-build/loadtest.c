#include <windows.h>
#include <stdio.h>
int main() {
    HMODULE h = LoadLibraryW(L"factor.dll");
    if (!h) { printf("LoadLibrary failed: %lu\n", GetLastError()); return 1; }
    printf("factor.dll loaded at %p\n", h);
    FARPROC p = GetProcAddress(h, "nf_new_vm");
    if (!p) { printf("nf_new_vm not found: %lu\n", GetLastError()); return 2; }
    printf("nf_new_vm at %p\n", p);
    return 0;
}
