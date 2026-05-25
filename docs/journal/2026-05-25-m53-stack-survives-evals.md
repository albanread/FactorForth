# M#53 — data stack survives between REPL evals (2026-05-25)

The catastrophic-underflow fix.  Yesterday's `0 xs @  1 xs @ . . . .`
shape — push four values, print them in a separate eval — crashed
the session because Factor's stock `eval>string` callback enforces
`( -- )` net effect per eval.  Any program that left a value on
the stack temporarily (the natural REPL idiom: type `5`, see it
on the stack, then `dup .`) died with an underflow-shaped error.

This was the most critical remaining bug for IDE usability: a
user could run a program, print a result, and the next eval would
crash because of stack residue from the first.

## The fix

Three pieces.

### 1. Custom eval-callback that saves/restores the data stack

`factor/forth/runtime/runtime.factor`:

```factor
SYMBOL: nf-saved-datastack
{ } nf-saved-datastack set-global

: nf-eval-with-saved-stack ( str -- )
    parse-string
    [ nf-saved-datastack get-global swap with-datastack
      nf-saved-datastack set-global ] call( quot -- ) ;

: nf-eval-callback ( -- callback )
    void* { c-string } cdecl
    [ nf-do-eval-with-vocabs f ] alien-callback ;

: install-nf-eval-callback ( -- )
    \ nf-eval-callback ?callback OBJ-EVAL-CALLBACK set-special-object ;
```

Mechanism: Factor's `with-callback-frame` zeroes the data stack
at the C boundary (alien-callbacks declare a strict effect, so
the VM saves/restores the actual stack around the callback body).
We hold the inter-eval stack in a global SYMBOL, restore it
before running user code via `with-datastack`, capture the
resulting stack after, save it back.  Cross-callback stack
persistence by hand.

The outer wrap `[ ... ] call( quot -- )` gives the alien-callback
type-checker a clean static effect on the body.  `with-datastack`'s
own effect is `( seq quot -- new-seq )` — Factor-static, no
runtime row-vars needed.

Installed via `install-nf-eval-callback` during session setup
(`src/session.rs`).

### 2. Hardened `nf-format-error` so recovery can't escape

When a parse error or runtime error fires inside the callback,
`nf-do-eval`'s `recover` invokes `nf-format-error`.  The original
code called `err present print` on the catch-all, which throws
`no-method` for some error tuple shapes (parse errors are
wrapped in `lexer-error` → `condition` → actual error).  A
throw from the recovery quot escapes the inner recover and trips
the alien-callback boundary's `check-datastack` → `kernel:die`.
Process dead.

Three hardenings:

- `nf-safe-present` wraps `present` in its own `recover`, falling
  back to a fixed string on no-method.
- `nf-unwrap-error` peels `lexer-error` and `condition` layers
  to find the real underlying error class.
- `nf-format-error` itself wraps the whole dispatch in `recover`
  — guarantees no exception ever escapes the formatter.

Recovery quot effect: try-quot has `( err -- )`, so recovery
sees `( err error -- )` and must `2drop` both before printing
its fallback string.  Mismatched effects on the previous attempt
triggered `unbalanced-branches-error` at compile time.

### 3. `no-word-error` formatted as ANS-13 (undefined word)

Pre-fix, the catch-all branch called `err present print` which
either printed garbage or threw no-method.  Added a direct case
that pulls `name>>` and emits `ANS error -13: Undefined word: foo`.

## Test surface

`tests/session_stack_survives.rs` — 10 lock-in tests, all passing:

```
one_value_left_on_stack_does_not_crash       (5 ; . cr  → "5")
many_values_survive_across_evals             (1 2 3 4 5 ; + + + +  → 15)
user_word_leaves_value_for_next_eval         (: forty-two 42 ; ; forty-two ; .)
three_evals_each_pushes_one_then_consume_all (10 ; 20 ; 30 ; + + . → 60)
arithmetic_leaving_value_for_dot_works       (3 4 * ; . → 12)
dup_then_print_in_separate_evals             (7 ; dup ; . . → "7 7")
balanced_eval_still_works                    (21 21 + . → 42)
definitions_still_persist_via_compile_context
variables_still_persist
no_method_error_caught_session_alive         ($len on int → recover, alive)
```

The shape every test drives: one Session, one CompileContext,
multiple `r.eval(src)` calls accumulating residue on the data
stack, then `r.captured()` for assertions.  The IDE's exact
worker shape.

## Diagnosis path

The fix took three iterations.  Each crashed differently:

**Attempt 1** — `parse-string call( ..a -- ..b )` without saved-stack.
This is what Factor's `eval-with-stack` does; passes for compiled
IR but the test suite's raw `session.eval("42 drop")` crashed
because `drop` (kernel) wasn't in the parser's vocab search path.
`with-file-vocabs` only adds `syntax`, not `kernel` / `math` / etc.
Stock `eval>string` has the same issue but its `print-error`
recovery doesn't throw, so the parse failure becomes a string in
the output — test doesn't notice.

**Attempt 2** — `with-datastack` directly.  Same kernel-error
problem as #1 plus `nf-format-error`'s `present` threw no-method
on the deep lexer-wrapped error tuple, escaping recover at the
alien-callback boundary → `combinators:wrong-values` →
`kernel:die`.  Process dead.

**Attempt 3 (current)** — `with-datastack` + hardened formatter.
`nf-format-error` wrapped in outer recover; `present` wrapped in
inner recover; `lexer-error` / `condition` unwrapped to find the
underlying class; `no-word-error` recognized directly.  All
error paths now stay inside the callback's stack frame.

Key lesson: an alien-callback's `with-callback-frame` *will* kill
the process if your callback body's net stack effect doesn't
match the declared `( c-string -- void* )` shape on *any* control
path, including the recover branch.  Every `recover` recovery
quot must be effect-balanced with the try quot.  And every word
called from the recovery quot must itself not throw — or be
wrapped in another recover.

## Files touched

- `factor/forth/runtime/runtime.factor`
  - `SYMBOL: nf-saved-datastack` (new)
  - `nf-eval-with-saved-stack` (new — parse-string + with-datastack)
  - `nf-safe-present`, `nf-unwrap-error` (new — formatter hardening)
  - `nf-format-tuple-error` — recognize `no-word-error`, use safe-present
  - `nf-format-error` — outer recover wrap
  - `nf-eval-callback`, `install-nf-eval-callback` (new)
- `src/session.rs` — append `install-nf-eval-callback` to setup
- `tests/session_stack_survives.rs` — 10 lock-in tests (new)
- `tests/diag_eval_callback.rs` — diagnostic harness (new)

## Test summary

```
session_smoke              5/5    ok
session_io                 5/5    ok
session_floats             3/3    ok
session_quickwins          7/7    ok
session_ans_booleans      19/19   ok
session_ans_core          19/19   ok
session_managed_strings   30/30   ok
session_ans_errors         3/3    ok
session_tick_execute       3/5    (2 pre-existing failures, see below)
session_file_access        3/3    ok
session_test_runner        6/6    ok
session_let_lang          14/14   ok
session_crash_recovery     2/2    ok
session_repl_context       4/4    ok
session_persistence       13/13   ok
session_stack_survives    10/10   ok    (NEW, #53)
─────────────────────────────────────
Total session            146/148  pass
```

## Pre-existing failures (not regressions)

`session_tick_execute::tick_xt_stored_in_variable` and
`tick_xt_vectoring_through_helper`.  Both store a quotation in
a Factor narrow-form variable (`SYMBOL: x  [ ... ] x set-global`)
then retrieve and call.  Factor's `call( -- )` then trips
`type-check-error` (kernel error 3).

Confirmed pre-existing by reverting `install-nf-eval-callback`
and re-running under stock callback — same failure, different
error message.

These tests' use case (an XT stored in a variable) needs the
wide variable form to round-trip cleanly through `set-global` /
`get-global`.  The compiler's interactive path (`compile_in_context`)
force-promotes all variables to wide, so the IDE never hits
this — only the standalone `compile()` driver used in this one
test file does.  Worth filing as a separate bug.

## What now works in the IDE

The catastrophic case is closed:

```
> 5                                  ← was crash; now survives
5
> . cr
5
> 3 4 *                              ← was crash on second eval
12
> .
12
> : count-up 1 + ;                   ← cross-eval definitions persist
> 41 count-up
> .
42
```

The REPL is now ergonomic: type a value, see it on the stack,
operate on it in the next eval.  Standard Forth listener UX.

## What's still pending for full REPL fidelity

- Quotation-in-narrow-variable round-trip (2 tick_execute tests).
  Standalone-compiler-only issue; doesn't block IDE.
- #47: alien-callback recover boundary — still partly relevant
  for kernel errors that bypass `nf-format-error`'s recover
  (hardware traps, oom).  Fixed in spirit for the common path.
- #48: hardware-trap recovery (DBZ, page fault) — separate
  machinery (SEH), unchanged.

## Architectural observation

The save-restore-via-global trick (`SYMBOL: nf-saved-datastack`)
is structurally simple but covers a fundamental impedance
mismatch: Factor's VM treats every callback as a clean
sub-computation with fresh stacks, while a REPL needs to
preserve state across what *look* like independent calls.  The
global symbol breaks out of the callback frame's sandbox by
storing the cross-call state in a place the VM won't wipe.

For now `nf-saved-datastack` is a single global — fine for the
singleton-Session world we're in.  If we ever support multiple
parallel sessions (separate VMs, separate threads), this would
need to be a per-session slot.  Not on the roadmap.

## Next from the roadmap

Per `current_status.md`:

1. Quote-in-variable round-trip fix (small).
2. **#10 Mandelbrot v1** — now end-to-end feasible.
3. Line-buffer flush at eval completion (~30 min cleanup).
