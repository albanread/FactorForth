/*
 * smoke.c — Stage 3 milestone: prove our patched factor.dll can be embedded
 * from a plain C host.  No Rust yet; this is the floor of the entire project.
 *
 * What it does:
 *   1. Create a fresh Factor VM
 *   2. Initialize it against our slim image (E:\NewFactor\nf-slim-v1.image)
 *   3. Evaluate "2 3 + ." as a Factor source string
 *   4. Print the captured output
 *
 * If this prints "5", we have:
 *   - the patched factor.dll's embedding API working
 *   - the slim image loading correctly
 *   - alien.remote-control's eval callback wired up
 *   - bidirectional C ↔ Factor communication
 *
 * Build:    cl /nologo /W3 smoke.c factor.dll.lib
 * Run:      .\smoke.exe
 *
 * The Factor DLL must be on PATH (or in the same dir as smoke.exe).
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

/* Opaque pointer types — we never dereference these from C. */
typedef struct factor_vm    factor_vm;
typedef struct vm_parameters vm_parameters;

/* vm_char on Windows is wchar_t (16-bit).  All path strings use this. */
typedef wchar_t vm_char;

/* Factor uses uintptr_t for tagged cells. */
#include <stdint.h>
typedef uintptr_t cell;

/* NewFactor embedding API exports (patched into factor.dll).
 * See vm/factor.hpp for the full set.
 */
__declspec(dllimport) factor_vm*    nf_new_vm(void);
__declspec(dllimport) vm_parameters* nf_default_parameters(void);
__declspec(dllimport) void           nf_free_parameters(vm_parameters* p);
__declspec(dllimport) void           nf_params_set_image_path(vm_parameters* p, const vm_char* path);
__declspec(dllimport) void           nf_params_set_signals(vm_parameters* p, int enable);
__declspec(dllimport) void           nf_init_factor(factor_vm* vm, vm_parameters* p);
__declspec(dllimport) char*          nf_eval_string(factor_vm* vm, char* s);
__declspec(dllimport) void           nf_eval_free(factor_vm* vm, char* p);

int main(int argc, char** argv) {
    const vm_char* image_path = L"E:\\NewFactor\\nf-slim-v1.image";

    printf("[smoke] Step 1: nf_new_vm()\n");
    factor_vm* vm = nf_new_vm();
    if (!vm) {
        fprintf(stderr, "[smoke] FAILED: nf_new_vm returned NULL\n");
        return 1;
    }
    printf("[smoke]         got vm=%p\n", (void*)vm);

    printf("[smoke] Step 2: nf_default_parameters()\n");
    vm_parameters* p = nf_default_parameters();
    if (!p) {
        fprintf(stderr, "[smoke] FAILED: nf_default_parameters returned NULL\n");
        return 1;
    }
    nf_params_set_image_path(p, image_path);
    nf_params_set_signals(p, 0);  /* don't install signal handlers */
    printf("[smoke]         image_path = %ls\n", image_path);

    printf("[smoke] Step 3: nf_init_factor() — loads the image\n");
    nf_init_factor(vm, p);
    printf("[smoke]         init done\n");

    /* Free our params copy; init_factor strdup'd what it needed. */
    nf_free_parameters(p);

    printf("[smoke] Step 4: nf_eval_string(\"2 3 + .\")\n");
    char* result = nf_eval_string(vm, (char*)"2 3 + . flush");
    if (!result) {
        fprintf(stderr, "[smoke] FAILED: nf_eval_string returned NULL\n");
        return 1;
    }
    printf("[smoke]         result = \"%s\"\n", result);

    /* Expected: "5\n" or "5 " or similar.  Just check for "5". */
    if (strstr(result, "5") != NULL) {
        printf("[smoke] *** PASS: result contains \"5\" ***\n");
    } else {
        fprintf(stderr, "[smoke] *** FAIL: result did NOT contain \"5\" ***\n");
        nf_eval_free(vm, result);
        return 1;
    }

    nf_eval_free(vm, result);

    /* A second eval to confirm the VM remains usable: */
    printf("[smoke] Step 5: a second eval — \"\\\"hello from factor\\\" print flush\"\n");
    char* r2 = nf_eval_string(vm, (char*)"\"hello from factor\" print flush");
    if (r2) {
        printf("[smoke]         result = \"%s\"\n", r2);
        nf_eval_free(vm, r2);
    }

    printf("[smoke] done.\n");
    return 0;
}
