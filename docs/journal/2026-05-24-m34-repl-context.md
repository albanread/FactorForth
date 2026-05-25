# M#34 follow-up — REPL context, `: square ; square` works (2026-05-24)

The first IDE launch hit the obvious-in-hindsight bug:

```
> : square dup * ;
> 5 square .
⚠ compile: unknown word `square`
```

Factor remembers — its dictionary keeps `scratchpad:square` alive
across our session's evals.  But **our compiler** doesn't.  Each
call to `compile()` builds a fresh `user_words` set populated only
from the source it was handed.  Eval 1 registers `square`, emits
the IR, throws the registration away.  Eval 2 starts blank, can't
find `square`, errors.

The user spotted it on the very first interactive session and
correctly diagnosed "the event handlers are tricky" — they ARE,
but in this case the bug was in our compiler's per-eval amnesia,
not the GUI's event plumbing.

## What shipped

A persistent `CompileContext` that the IDE worker holds for the
session's life and threads through each eval:

```rust
pub struct CompileContext {
    /// Lowercase ANS name → span of first definition.
    pub user_words:   HashMap<String, Span>,
    /// Lowercase ANS name → inferred stack effect.  Needed
    /// because Factor's strict effect inference rejects a
    /// concrete body under a row-var annotation, and without
    /// prior effect info the synth path falls back to row-vars
    /// for any reference to a prior-eval word.
    pub user_effects: HashMap<String, Effect>,
}
```

New entry points layered on the existing pipeline:

```rust
// resolve.rs
pub fn resolve_with_prior(prog, &prior_user_words) -> Result<Resolved, _>

// effect.rs
pub fn infer_with_prior(&resolved, &prior_effects) -> (Inferred, _)

// sema.rs
pub fn build_with_prior(prog, &prior_words, &prior_effects) -> Result<Sema, _>

// compiler/mod.rs
pub fn compile_in_context(src, &mut CompileContext) -> Result<String, String>
```

The original `compile()` is unchanged — it creates an empty
context internally.  Tests / CLI / one-shot evals work as before.
The IDE worker uses `compile_in_context` and holds the ctx
across the session's life.

## What I learned

The fix has TWO layers:

1. **Resolver memory**: `resolve_with_prior` lets eval 2's body
   resolve a reference to `square` defined in eval 1.

2. **Effect memory** (subtle, hit it as a second bug): when
   eval 2's body uses `a` from eval 1, the synth needs to know
   `a`'s effect.  Without it, the synth says Unknown, the emit
   falls back to row-vars `( ..a -- ..b )`, and Factor's strict
   inference then refuses to compile a concrete body (`a a +`,
   net effect `( -- z )`) under a row-var annotation.  Took a
   diag eval to spot — the IR looked plausible but Factor's
   compiler said "no."

Both are now plumbed through.

## Tests

`tests/session_repl_context.rs` — 4 lock-in tests:

- `user_def_from_first_eval_resolves_in_second` — the exact GUI
  bug.  `: square dup * ;` then `5 square .` → `25`.
- `constant_persists_across_evals` — `7 constant lucky` then
  `lucky lucky + .` → `14`.
- `three_evals_word_calls_prior_word` — chain: `: a 10 ;` then
  `: b a a + ;` then `b .` → `20`.  Catches the effect-memory bug.
- `fresh_context_does_not_see_old_defs` — sanity: contexts are
  independent.

## Known limitation: variables across evals

Variables are subject to per-compile escape analysis that hoists
them to "narrow" (Factor `SYMBOL:` + `get-global`) when all their
uses in one compile match the narrow pattern.  When a narrow
variable is defined in eval 1, eval 2's reference to the name
emits as a bare word call — which Factor interprets as "push
the symbol object" rather than "fetch the cell value."

Fix is filed as #52: in `compile_in_context` mode, force all
variables to the wide (nf-addr-backed) form so cross-eval shape
is consistent.  Within a single compile, narrow is still the
right call.

Interactive variables work within ONE eval today; cross-eval
support comes when #52 lands.

## Test summary

```
session_smoke              5/5   ok
session_io                 5/5   ok
session_floats             3/3   ok
session_quickwins          7/7   ok
session_ans_booleans      19/19  ok
session_ans_core          19/19  ok
session_managed_strings   30/30  ok
session_ans_errors         3/3   ok
session_tick_execute       5/5   ok
session_file_access        3/3   ok
session_test_runner        6/6   ok
session_let_lang          14/14  ok
session_crash_recovery     2/2   ok
session_repl_context       4/4   ok   (NEW)
ans-core-corpus           61/61  pass
─────────────────────────────────────
Session-based + corpus   186/186 ok
smoke_runtime (legacy)    40/40  ok   (run separately, #31)
Grand total              226/226 ok
```

## Files touched

- `src/compiler/resolve.rs` — `resolve_with_prior` + combined-set
  lookup (prior ∪ this compile)
- `src/compiler/effect.rs` — `infer_with_prior` seeds user_effects
- `src/compiler/sema.rs` — `build_with_prior` plus widened
  user_words registration (now covers Variable / Constant /
  Create / Collection / Template / TemplateInstance, not just
  Definition)
- `src/compiler/mod.rs` — `CompileContext` struct +
  `compile_in_context` / `compile_in_context_with_diagnostics`
- `src/bin/newfactor_ui.rs` — IDE worker holds the context,
  passes it to `handle_eval` / `handle_eval_repl`, resets it
  on `ForthRestart` to stay in lockstep with Factor's dictionary
- `tests/session_repl_context.rs` — 4 lock-in tests

## Rebuild

```
E:\NewFactor\target\release\newfactor-ui.exe
```

Re-launch and the original bug should be gone:

```
> : square dup * ;
> 5 square .
25
> : add1  1 + ;
> 41 add1 .
42
> : doubled  dup + ;
> 7 doubled doubled .
28
```
