# Embedding API — findings from reading the source

Companion to `docs/dls10-synthesis.md`.  Written while the slim bootstrap
runs in the background, after reading:

- `basis/alien/remote-control/remote-control.factor` (26 lines)
- `basis/alien/remote-control/remote-control-tests.factor` (43 lines; STALE)
- `basis/eval/eval.factor` (45 lines)
- `vm/factor.cpp` (189 lines)
- A grep-sweep of every `VM_C_API` declaration in `vm/`

---

## Bottom-line conclusion

The prebuilt `factor.dll` we have **cannot** be used for in-process embedding
as it ships.  But the patch to make it embeddable is **~25 lines of C++
added to `vm/factor.cpp`** — six trivial wrappers exposing already-written
member functions of `factor_vm`.  This is the smallest possible Stage 3.

The manifesto's `nf_vm_*` API maps almost 1:1 onto factor_vm's existing
embedding methods.  We're not designing an API; we're *exporting* one
Slava already wrote.

---

## What's actually exported by the current factor.dll

Complete list of `VM_C_API`-marked functions in `vm/` (185 symbols total,
mostly primitives):

| Category | Symbols | Comment |
|---|---|---|
| Lifecycle | `start_standalone_factor`, `start_standalone_factor_in_new_thread`, `wmain` | The only entry points.  All run the startup quotation and either exit or return after termination. |
| Context (JIT-called) | `new_context`, `delete_context`, `reset_context`, `begin_callback`, `end_callback`, `trampoline`, `trampoline2` | Stack management.  Not directly useful to embed-from-outside. |
| Math (JIT-called) | `from_signed_*`, `from_unsigned_*`, `to_signed_*`, `to_unsigned_*`, `to_fixnum`, `to_cell`, `overflow_fixnum_*` | Boxing/unboxing helpers.  Called by JIT'd code. |
| Compiler (JIT-called) | `lazy_jit_compile`, `inline_cache_miss`, `undefined_symbol`, `flush_icache`, `factor_memcpy` | Internal JIT infrastructure. |
| Error handling | `err_no`, `set_err_no`, `exception_handler` (Win SEH) | Plumbing. |
| Primitives | `primitive_*` × 165 | Each takes `factor_vm*` and operates on its data stack.  Generated via the X-macro in `vm/primitives.hpp`.  **These are accessible to outside C code if we have a `factor_vm*`.** |

## What's NOT exported (but exists internally)

These are all C++ **member functions** of `factor_vm` in `vm/factor.cpp`:

| Symbol | Lines in factor.cpp | Purpose |
|---|---|---|
| `factor_vm::init_factor(vm_parameters*)` | 38–125 | Allocates stacks, loads image, sets up FFI, populates `special_objects` (incl. OBJ_STDIN/STDOUT/STDERR, OBJ_IMAGE, etc.).  This is the entry point we want. |
| `factor_vm::pass_args_to_factor(int, vm_char**)` | 128–136 | Populates OBJ_ARGS from argv. |
| `factor_vm::start_standalone_factor(int, vm_char**)` | 162–172 | Calls `init_factor` then `c_to_factor_toplevel(OBJ_STARTUP_QUOT)`. |
| `factor_vm::factor_eval_string(char*)` | 142–146 | Looks up OBJ_EVAL_CALLBACK and calls it.  Returns `char*` (malloc'd, caller frees with `factor_eval_free`). |
| `factor_vm::factor_eval_free(char*)` | 148 | `free(p)`.  One line. |
| `factor_vm::factor_yield()` | 150–154 | Looks up OBJ_YIELD_CALLBACK and calls it. |
| `factor_vm::factor_sleep(long)` | 156–160 | Looks up OBJ_SLEEP_CALLBACK and calls it. |
| `factor_vm::stop_factor()` | 138–140 | Runs OBJ_SHUTDOWN_QUOT. |
| `new_factor_vm()` (free function, not member) | 174–181 | Allocates and registers a `factor_vm`. |

**Note on the stale test.**  `basis/alien/remote-control/remote-control-tests.factor`
shows an embedding example with `default_parameters`, `F_PARAMETERS`,
`STRING_LITERAL`, `start_embedded_factor` — none of these exist in the
current source.  The tests are commented out.  This is from an older
Factor that had a different embedding API; it has rotted.  Ignore.

---

## How `alien.remote-control` actually works (26 lines, full reproduction)

```factor
USING: alien alien.c-types alien.data eval io.encodings.utf8
kernel kernel.private threads words ;
IN: alien.remote-control

: eval-callback ( -- callback )
    void* { c-string } cdecl
    [ eval>string utf8 malloc-string ] alien-callback ;

: yield-callback ( -- callback )
    void { } cdecl [ yield ] alien-callback ;

: sleep-callback ( -- callback )
    void { long } cdecl [ sleep ] alien-callback ;

: ?callback ( word -- alien )
    dup word-optimized? [ execute ] [ drop f ] if ; inline

: init-remote-control ( -- )
    \ eval-callback ?callback OBJ-EVAL-CALLBACK set-special-object
    \ yield-callback ?callback OBJ-YIELD-CALLBACK set-special-object
    \ sleep-callback ?callback OBJ-SLEEP-CALLBACK set-special-object ;

MAIN: init-remote-control
```

What this does:

1. **`*-callback` words** use `alien-callback` to compile a Factor
   quotation into a *C-callable function pointer* with a declared C
   signature.  The compiler emits a thunk that converts C args → Factor
   stack and back.  The result of executing one of these words at run
   time is the function pointer itself.
2. **`init-remote-control`** stores those function pointers as special
   objects so the C side (via `factor_vm::factor_eval_string` etc.)
   can locate them.
3. **`?callback`** is defensive — if a callback word hasn't been
   compiled by the optimising compiler yet (e.g. running in a minimal
   image without compiler.units fully wired up), use `f` (false) and
   the C side will skip the callback.

**The `eval-callback` body is `[ eval>string utf8 malloc-string ]`.**
The C side passes a `char*`, this body:

1. Receives the C string on the Factor stack
2. Calls `eval>string` (text-eval, goes through parser, returns string)
3. Encodes the result as UTF-8 and `malloc`s it
4. Returns the malloc'd pointer through the C ABI

So `factor_vm::factor_eval_string(p)` is morally:

```c
char* factor_eval_string(char* s) {
    char* result = (eval-callback)(s);
    return result;  // caller frees with factor_eval_free
}
```

The callback ABI is `void* fn(const char*)`.

---

## What `eval>string` actually does (from `basis/eval/eval.factor`)

```factor
: parse-string ( str -- quot )
    [ split-lines parse-lines ] with-compilation-unit ;

: (eval) ( str effect -- )
    [ parse-string ] dip call-effect ; inline

: eval>string ( str -- output )
    [
        [ parser-quiet? on
          '[ _ ( -- ) (eval) ] [ print-error ] recover
        ] with-string-writer
    ] with-file-vocabs ;
```

**Crucially: this path goes through Factor's surface parser.**  It is
text-in, text-out.  Factor source code in, evaluation output as a string.

This is exactly the Forth-→-Factor-text transpilation path the manifesto's
mission-restatement explicitly ruled out.  Sending Forth output through
`factor_eval_string` would put us right back into that rejected mode.

**However**, `eval>string` *does* go through the full optimising compiler:
`parse-string` wraps `parse-lines` in `with-compilation-unit`, which is
the canonical compilation entry point.  Output of `parse-string` is a
quotation whose entry-point is either optimised machine code (if eagerly
JIT'd) or `lazy_jit_compile` (if deferred).

So `eval>string`:
- ✅ Produces optimised code
- ❌ Requires Factor surface syntax as input
- ❌ Re-parses + re-compiles on every call (no caching unless we cache the
     resulting quotation handle Rust-side, which we can't get from the
     `eval>string` path)

---

## The architecture this forces

Two distinct embedding paths, both needed:

### Path A — `eval>string` for bootstrap, one-time

Use the existing `alien.remote-control` mechanism for initial setup
operations that ARE legitimately expressed in Factor source:

- `"USE: alien.remote-control" eval>string` to load the embedding vocab
- `"USE: forth.all" eval>string` to load our Forth runtime vocab
- Any one-shot configuration commands

This path is fine for one-time setup; perf doesn't matter.  Costs:
parser + compiler per call, but called O(1) times during init.

### Path B — direct quotation submission for the hot path

Rust emits Factor quotations directly (arrays of word references +
literals + primitives), pushes them to the VM, calls them.

The primitives we need are all already exported from `factor.dll`:

| Primitive | Purpose in our use |
|---|---|
| `<array>` | Allocate the quotation's backing array |
| `set-slot` | Fill the array with word refs, literals, primitive refs |
| `array>quotation` | Tag the array as a quotation |
| `jit-compile` | Eagerly JIT the quotation (optional; would happen on first call anyway via `lazy-jit-compile`) |
| `(call)` | Execute the quotation |

Each takes `factor_vm*` and operates on its data stack.  But to drive them
from Rust, we need to be able to *push and pop the data stack from C*,
which means we need an exported `factor_vm*` pointer and exported
push/pop helpers.

**Currently missing exports** for Path B to work from Rust:

| Missing | Where it would live |
|---|---|
| `nf_new_vm()` → `factor_vm*` | wrapper around `new_factor_vm()` |
| `nf_init_factor(factor_vm*, vm_parameters*)` | wrapper around `vm->init_factor(p)` |
| `nf_call_quotation(factor_vm*, cell quot)` | wrapper around `vm->c_to_factor(quot)` — verify the exact entry point |
| `nf_eval_string(factor_vm*, char*)` → `char*` | wrapper around `vm->factor_eval_string(s)` (Path A) |
| `nf_eval_free(factor_vm*, char*)` | wrapper around `vm->factor_eval_free(p)` |
| `nf_yield(factor_vm*)` | wrapper around `vm->factor_yield()` |
| `nf_datastack_push(factor_vm*, cell)` | inline: `vm->ctx->push(value)` |
| `nf_datastack_pop(factor_vm*)` → `cell` | inline: `return vm->ctx->pop()` |

**Approximate patch size: 25 lines.**  All trivial pass-throughs.

---

## The minimal Stage 3 patch (proposed)

Append to `vm/factor.cpp`:

```cpp
// ── NewFactor embedding wrappers ────────────────────────────────────
// Trivial VM_C_API pass-throughs exposing factor_vm's existing
// embedding methods.  These do not add semantics; they make the
// VM linker emit symbols that DLL clients can call.

VM_C_API factor_vm* nf_new_vm() {
    return new_factor_vm();
}

VM_C_API void nf_init_factor(factor_vm* vm, vm_parameters* p) {
    vm->init_factor(p);
}

VM_C_API void nf_pass_args(factor_vm* vm, int argc, vm_char** argv) {
    vm->pass_args_to_factor(argc, argv);
}

VM_C_API char* nf_eval_string(factor_vm* vm, char* s) {
    return vm->factor_eval_string(s);
}

VM_C_API void nf_eval_free(factor_vm* vm, char* p) {
    vm->factor_eval_free(p);
}

VM_C_API void nf_yield(factor_vm* vm) {
    vm->factor_yield();
}

VM_C_API void nf_sleep(factor_vm* vm, long us) {
    vm->factor_sleep(us);
}

VM_C_API void nf_stop(factor_vm* vm) {
    vm->stop_factor();
}

// Direct quotation invocation — bypasses parser
VM_C_API void nf_call_quotation(factor_vm* vm, cell quot) {
    vm->c_to_factor(quot);
}

// Datastack access from C side
VM_C_API void nf_push(factor_vm* vm, cell value) {
    vm->ctx->push(value);
}

VM_C_API cell nf_pop(factor_vm* vm) {
    return vm->ctx->pop();
}

VM_C_API cell nf_peek(factor_vm* vm) {
    return vm->ctx->peek();
}

VM_C_API cell nf_datastack_depth(factor_vm* vm) {
    return (vm->ctx->datastack - vm->ctx->datastack_seg->start) / sizeof(cell);
}
```

Add corresponding declarations to `vm/factor.hpp`.  Rebuild.

That's the whole "fork".  No structural changes.  No new semantics.  Just
explicit dllexport of methods Slava already wrote.

The build path is still TBD — we have VS18 Pro installed but `cl.exe` not
on PATH, will need `vcvars64.bat` orchestration.  Or Zig as a C++ compiler
since `zig.exe` is on PATH.  Stage 1's "VS18 vs Zig" decision still matters.

---

## What the slim image's startup quotation must do

Once we have the embedding-patched VM and a slim image, the image's
startup quotation needs to leave Factor in a state where:

1. `alien.remote-control` is loaded and `init-remote-control` has been
   called (sets the three special-object callbacks).
2. Our `forth.all` runtime vocab is loaded (provides the words our
   Rust-emitted quotations reference).
3. Factor parks in a yield loop OR returns control to C via
   `start_standalone_factor`'s normal return path.

A reasonable startup quotation:

```factor
[
    "alien.remote-control" run    ! sets up eval/yield/sleep callbacks
    "forth.all" require           ! loads our runtime
    ! Park — yield repeatedly so factor_yield() works and so the
    ! main Factor thread doesn't exit.  Wake-up is triggered by
    ! C calling factor_eval_string() or factor_yield().
    [ yield t ] loop
]
```

We set this as `OBJ_STARTUP_QUOT` either via a boot-script that runs
after the slim bootstrap, or by saving the image from inside Factor
after configuring it.

---

## Open questions parked for Stage 2 (VM source read)

1. **What is the exact entry point for "call a quotation from C"?**
   The paper says `c_to_factor_toplevel(OBJ_STARTUP_QUOT)`.  Is
   `c_to_factor` (without `_toplevel`) suitable for non-startup calls?
   → Read `vm/factor.cpp` callers of `c_to_factor_*` and `vm/contexts.cpp`.

2. **What's the lifetime of a `factor_vm*`?**  Does it need to be
   single-threaded?  (The TLS-keyed `thread_vms` map in `vm/mvm.cpp`
   suggests one VM per thread.)

3. **How does `ctx->push`/`pop` interact with the data stack segment?**
   Need to confirm the right way to read stack depth from outside.

4. **What does `(call)` look like at the C level?**  It's listed as a
   primitive (`primitive_(call)` in the index).  How is it different
   from `c_to_factor`?

These are all bounded reading tasks for next session.

---

## Status against the manifesto

| Stage | Status |
|---|---|
| 1. Download docs | ✅ Done.  Synthesis written. |
| 2. Understand VM representation | 🟡 Partial.  Embedding API now clear; quotation layout / image format still to read. |
| 3. Build slim VM with embedding API | 🟢 Scope dropped to a 25-line patch.  Build toolchain question (MSVC vs Zig) remains. |
| 3 (concurrent). Build slim image | 🟡 In progress.  Bootstrap running now. |
| 4. Rust Forth compiler | ⏳ Blocked on 3. |
| 5. ANS test suite | ⏳ Blocked on 4. |
