# Embedding Factor

This document is for people who want to know *how* FactorForth
uses Factor under the hood.  If you just want to write Forth,
skip to [getting-started.md](getting-started.md).

## Why embed?

We could have written a Forth interpreter from scratch.  WF64
does exactly that — assembly-level JIT, hand-written GC, the
works.  For FactorForth we took a different bet: build the
front-end and reuse a mature VM.

Factor gives us:

- **A garbage collector** that's been hardened over 20 years.
- **A native compiler** that emits good machine code.
- **An FFI** that handles Win64 ABI quirks, callbacks, structs.
- **A standard library** with ~2000 vocabularies covering files,
  threads, regex, JSON, HTTP, you name it.
- **Float support** with proper XMM register handling.

For a small team this is a massive head-start.  The trade-off:
we don't get to choose Factor's runtime semantics, and we can't
fix bugs in Factor without forking.  So far that hasn't been
a blocker.

## The embedding API

Stock Factor runs from `factor.exe`.  To use the VM as a
library we patched `vm/factor.cpp` to expose three C entry
points:

```c
// Initialise the VM, load an image, return a handle.
factor_vm* nf_init_factor(const vm_char* image_path,
                          const vm_parameters* params);

// Evaluate a Factor source string.  Returns malloc'd C string
// with the captured output (must be freed via nf_eval_free).
char* nf_eval_string(factor_vm* vm, char* source);
void  nf_eval_free  (factor_vm* vm, char* result);
```

These wrap Factor's internal `c_to_factor_toplevel` and friends.
The patch is small (~50 lines) and lives in our repo under
`vm-build/`.

## What goes in the image

`scripts/build-image.sh` produces `factorforth.image` from a
stock Factor bootstrap image plus three steps:

1. Add `E:/NewFactor/factor` as a vocab root.
2. Load `forth.runtime` and `forth.wf64-gfx` vocabularies.
3. Install our custom `OBJ_EVAL_CALLBACK`.
4. Save the image.

The result is ~134 MB.  Most of that is Factor's standard
library; our additions are a rounding error.

## The OBJ_EVAL_CALLBACK trick

Factor's VM has a "special object" slot called
`OBJ_EVAL_CALLBACK` that names a Factor word.  When a host calls
`nf_eval_string`, the VM invokes that word with the source
string as argument.  Stock Factor sets it to `eval>string`,
which calls `(eval)` with effect `( -- )`.

`( -- )` is bad for a REPL: it means every eval must leave the
data stack exactly as it found it.  Type `5` and you get an
error — `5` left a value behind.  Type `5  drop` and you're
fine.  This is correct for the standalone listener (which
prints the stack after each eval anyway) but terrible for
interactive Forth.

Our custom callback uses `parse-string` + `with-datastack`,
which accepts arbitrary net stack changes.  Values left on the
stack persist into the next eval, exactly as a Forth listener
should behave.

The callback also handles:

- Cross-frame **data-stack persistence** via a `nf-saved-datastack`
  SYMBOL: that survives the alien-callback boundary.
- **Error formatting** — ANS error codes (`-4` stack underflow,
  `-13` undefined word, etc.) instead of Factor's debugger
  output.
- **Recovery** — if a user-level error fires, the recover quot
  prints the formatted message and the session keeps going.

See `factor/forth/runtime/runtime.factor` for the implementation
and `docs/journal/2026-05-25-m53-stack-survives-evals.md` for
the design history.

## I/O routing

Forth's `EMIT` / `KEY` / `TYPE` / `." ... "` need to flow
through the IDE's console pane, not Factor's stdout.  We do
this by:

1. Exporting three `nf_rt_*` symbols from the IDE binary (via
   `build.rs` `/EXPORT:` linker args).
2. Registering them as Factor `FUNCTION:`s at startup.
3. Having `install-host-streams` bind Factor's
   `input-stream` / `output-stream` to wrappers that call into
   the host.

When user Forth code calls `EMIT 65`, the chain is:

```
EMIT 65
  -> Factor's emit primitive
    -> output-stream's write1 method
      -> host-output-stream tuple's write1 (defined in our code)
        -> FUNCTION: nf_rt_write_char
          -> Rust function nf_rt_write_char
            -> session's IoState
              -> Gui mode: closure that appends to fconsole pane
```

It's a lot of indirection but each step is a single function
call.  Throughput is fine even for long output.

## Persistence across evals

Three layers persist:

1. **Factor's image** — the underlying word dictionary, vocab
   roots, image-loaded code.  Persists as long as the Session
   worker thread lives.

2. **The data stack** — handled by our custom eval-callback
   via `nf-saved-datastack`, as described above.

3. **User definitions across evals** — handled in the Rust
   compiler.  `CompileContext` tracks `user_words` (name →
   first-def span), `user_effects` (name → stack effect),
   `templates` (name → CREATE/DOES> body).  Each call to
   `compile_in_context` threads these in, so eval N+1 can
   reference everything from eval N.

When you say `: square dup * ;` and then `5 square .`, the
second line's `square` resolves via the CompileContext.  No
session re-boot, no re-parse.

## Restart semantics

Ctrl+Shift+F5 in the IDE drops the current `Session` (the
worker thread exits, factor.dll's per-thread state is freed)
and spawns a fresh one.  The image is re-loaded; the Factor
dictionary resets to its image-time state.

But the `CompileContext` also resets.  This keeps Forth's view
in sync with the VM: if you defined `square` in the old
session, you'll have to redefine it in the new one.

## What we can't do

A few things require deeper Factor patches than we've tackled:

- **In-flight interrupt.**  Long-running Forth loops can't be
  interrupted from outside Factor.  Stock Factor's listener
  uses thread signals; we'd need an SEH-based equivalent on
  Windows.
- **Hardware-trap recovery (full).**  Some traps (page faults
  inside compiled code) leave the VM in a state that's hard to
  recover from cleanly.  We spawn a fresh thread but lose any
  uncommitted state.
- **Stop-and-debug.**  Factor has a great visual debugger; we
  don't expose it.  Could be added by routing the debug-shell
  through host streams.

These are tracked in the source repo's issue list.  None of
them block normal interactive use.

## Source pointers

If you want to read the code:

- `vm-build/` — the factor.cpp patch
- `factor/forth/runtime/runtime.factor` — the Forth-side
  runtime support (callbacks, error formatting, host stream
  wrappers, ans-execute, etc.)
- `src/session.rs` — the Rust side of the embedding
- `src/compiler/` — the ANS Forth → Factor IR compiler
- `scripts/build-image.sh` — the image build recipe

All BSD-licensed.  Patches welcome.
