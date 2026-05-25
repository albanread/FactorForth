# M2.x #32 — INCLUDED file-access primitive (2026-05-24)

The critical word the canonical Forth 2012 test runner needs:
load an ANS source file at runtime.  `runtests.fth` does:

```forth
S" prelimtest.fth" INCLUDED
S" tester.fr"      INCLUDED
S" ttester.fs"     INCLUDED
S" core.fr"        INCLUDED
\ ... etc
```

## The architectural twist

INCLUDED reads source from a file at RUNTIME, but the file
contents are ANS Forth — NewFactor's compiler must translate them
to Factor IR before Factor's `(eval)` can run them.  And the
compilation has to happen *inside* the running session, not at
host-eval-call time.

The flow:

```
Forth source:    S" tester.fr" INCLUDED
                 │
                 ▼
Factor IR:       "tester.fr" forth.runtime:nf-included
                 │
                 ▼  inside Factor's eval, on the worker thread:
nf-included:     >$ rt_compile_ans ( -- ) (eval)
                 │
                 ▼  FFI back to Rust:
rt_compile_ans:  read file path → fs::read_to_string
                 → newfactor::compiler::compile(contents)
                 → return Factor IR as malloc'd C string
                 │
                 ▼  Factor receives IR string, runs (eval):
                 ANS source's compiled Factor IR executes in-place.
```

Key design choice: **the file's compilation runs synchronously
on the worker thread**.  That's fine because the worker IS what's
executing the outer eval; the compile call is a regular function
call within the same VM thread.

## The FFI extern

```rust
#[no_mangle]
pub extern "C" fn rt_compile_ans(path_cstr: *const c_char) -> *mut c_char
```

Takes a NUL-terminated UTF-8 path.  Returns a malloc'd
Factor-readable IR string.  On error (file not found, malformed
source, etc.) returns a Factor snippet that prints the error —
the outer (eval) runs the snippet, the user sees a diagnostic
through the captured stream.

Factor side declares the FFI:

```factor
FUNCTION: c-string rt_compile_ans ( c-string path )
```

Factor's `c-string` marshaling: allocates a NUL-terminated copy
of the Factor string for the call, copies the return value back
into a Factor string.  Memory ownership handled transparently.

## What's NOT in this milestone

The full ANS File Access Word Set has many more words:
`OPEN-FILE`, `CREATE-FILE`, `CLOSE-FILE`, `READ-FILE`,
`WRITE-FILE`, `READ-LINE`, `WRITE-LINE`, `FILE-POSITION`,
`REPOSITION-FILE`, `FILE-SIZE`, `RESIZE-FILE`, `DELETE-FILE`,
`R/O`, `R/W`, `W/O`, `BIN`, `INCLUDE-FILE`.

None are needed for the test runner.  Deferred to a follow-up
ticket — Factor's `io.files` vocab already has the substrate;
each one is essentially a thin wrapper.

## Architectural limitation worth knowing

NewFactor's resolver runs at *compile* time, so words defined by
an INCLUDED file are visible only to OTHER code INSIDE that same
included file (which is also processed in one compile unit).
External callers compile to Factor IR before the include runs,
so they can't reference included-defined words by name.

The fixture demonstrates this:

```forth
\ Inside the fixture itself:
: included-word 42 ;
." word produces: " included-word . cr
```

The fixture's own `included-word .` call works because it's
inside the same compile unit.  But a separate eval that does
`S" file" INCLUDED  included-word .` won't compile because
the resolver doesn't see the future include.

This is an acceptable limitation for the test runner — each
test file is self-contained, and the test runner's tester.fr
fixture lives in its own session-wide load step that brings in
all the words tester.fr needs.

## Tests

3 lock-in tests in `tests/session_file_access.rs`:

- `included_prints_fixture_output` — `S" path" INCLUDED` runs the
  fixture's `." hello from included"`
- `included_with_managed_string_path_works` — same via the
  `S$" path" $>addr INCLUDED` round-trip
- `included_missing_file_does_not_kill_session` — bad path
  produces a diagnostic, session survives

## Test summary

```
session_smoke              5/5  ok
session_io                 5/5  ok
session_floats             3/3  ok
session_quickwins          7/7  ok
session_ans_booleans      19/19 ok
session_ans_core          19/19 ok
session_managed_strings   30/30 ok
session_ans_errors         3/3  ok
session_tick_execute       5/5  ok
session_file_access        3/3  ok   (NEW)
─────────────────────────────────────
Session-based total       99/99 ok
smoke_runtime (legacy)    40/40 ok   (run separately, #31)
Grand total              139/139 ok
```

## Files touched

- `src/session.rs` — `rt_compile_ans` extern + keep-alive static
- `build.rs` — added rt_compile_ans to /EXPORT list
- `factor/forth/runtime/runtime.factor` — `FUNCTION:
  rt_compile_ans` + `nf-included` rewrite + `INCLUDED` resolver
- `src/compiler/resolve.rs` — `included` entry
- `src/compiler/effect.rs` — `included` effect (2, 0)
- `tests/session_file_access.rs` — 3 lock-in tests
- `tests/fixtures/included-hello.fs` — fixture
- `images/nf-mandelbrot.image` — rebuilt

## What's next

**#41 stand up the Forth 2012 test runner.**  All blockers cleared:

- ✅ `'` + EXECUTE (#33) — for tester.fr's vectored ERROR
- ✅ INCLUDED (#32) — for `S" path" INCLUDED`
- ✅ ANS booleans (#40) — for `T{ ... -> -1 }T`
- ✅ Error visibility (#35) — diagnostics visible in captured output
- ✅ Session-survives-error — proven

Next sprint: copy `E:/wf32/forth2012-test-suite-master/` into the
project, write a Rust driver that invokes `runtests.fth` (or the
subset we can support), and instrument tester.fr's ERROR vector
to produce a pass/fail/nyimp tally.
