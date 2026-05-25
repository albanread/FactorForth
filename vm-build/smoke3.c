/*
 * smoke2.c — same goal as smoke.c, but via runtime LoadLibrary instead of
 * static-linking against factor.dll.lib.  Avoids whatever static-loader issue
 * was blocking smoke.exe from reaching main().
 */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef void* factor_vm;
typedef void* vm_parameters;
typedef wchar_t vm_char;
typedef uintptr_t cell;

typedef factor_vm*    (*pfn_new_vm)(void);
typedef vm_parameters* (*pfn_default_params)(void);
typedef void           (*pfn_free_params)(vm_parameters*);
typedef void           (*pfn_set_image_path)(vm_parameters*, const vm_char*);
typedef void           (*pfn_set_signals)(vm_parameters*, int);
typedef void           (*pfn_init)(factor_vm*, vm_parameters*);
typedef void           (*pfn_run_startup)(factor_vm*);
typedef char*          (*pfn_eval)(factor_vm*, char*);
typedef void           (*pfn_eval_free)(factor_vm*, char*);

#define LOAD(var, name) \
    var = (pfn_##var)GetProcAddress(hF, "nf_" #name); \
    if (!var) { fprintf(stderr, "[smoke2] missing export nf_" #name "\n"); return 10; } \
    printf("[smoke2]   nf_" #name " @ %p\n", (void*)var);

int main(int argc, char** argv) {
    printf("[smoke2] Loading factor.dll...\n");
    fflush(stdout);
    HMODULE hF = LoadLibraryW(L"factor.dll");
    if (!hF) {
        fprintf(stderr, "[smoke2] LoadLibrary failed: %lu\n", GetLastError());
        return 1;
    }
    printf("[smoke2]   factor.dll @ %p\n", (void*)hF);

    pfn_new_vm         new_vm;
    pfn_default_params default_params;
    pfn_free_params    free_params;
    pfn_set_image_path set_image_path;
    pfn_set_signals    set_signals;
    pfn_init           init;
    pfn_run_startup    run_startup;
    pfn_eval           eval;
    pfn_eval_free      eval_free;

    printf("[smoke2] Resolving symbols...\n"); fflush(stdout);
    LOAD(new_vm,         new_vm);
    LOAD(default_params, default_parameters);
    LOAD(free_params,    free_parameters);
    LOAD(set_image_path, params_set_image_path);
    LOAD(set_signals,    params_set_signals);
    LOAD(init,           init_factor);
    LOAD(run_startup,    run_startup);
    LOAD(eval,           eval_string);
    LOAD(eval_free,      eval_free);

    printf("[smoke2] new_vm()\n"); fflush(stdout);
    factor_vm* vm = new_vm();
    printf("[smoke2]   vm @ %p\n", (void*)vm); fflush(stdout);

    printf("[smoke2] default_parameters() + set image path\n"); fflush(stdout);
    vm_parameters* p = default_params();
    const vm_char* img = L"E:\\NewFactor\\nf-noop.image";
    set_image_path(p, img);
    set_signals(p, 0);
    printf("[smoke2]   image=%ls\n", img); fflush(stdout);

    printf("[smoke2] init_factor() — loading image...\n"); fflush(stdout);
    init(vm, p);
    printf("[smoke2]   init done\n"); fflush(stdout);
    free_params(p);

    printf("[smoke2] run_startup() — runs init-remote-control to wire callbacks\n"); fflush(stdout);
    run_startup(vm);
    printf("[smoke2]   startup done\n"); fflush(stdout);

    printf("[smoke2] eval_string(\"2 3 + . flush\")\n"); fflush(stdout);
    char* r = eval(vm, (char*)"2 3 + . flush");
    if (!r) { fprintf(stderr, "[smoke2] eval returned NULL\n"); return 2; }
    printf("[smoke2]   result=\"%s\"  (length %zu)\n", r, strlen(r)); fflush(stdout);

    int pass = (strstr(r, "5") != NULL);
    eval_free(vm, r);

    if (pass) {
        printf("[smoke2] *** PASS *** — embedded Factor VM eval'd \"2 3 + .\" and returned 5\n");
        return 0;
    } else {
        fprintf(stderr, "[smoke2] *** FAIL *** — result did not contain \"5\"\n");
        return 3;
    }
}
