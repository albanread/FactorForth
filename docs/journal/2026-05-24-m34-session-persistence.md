# M#34 follow-up — full session persistence (2026-05-24)

The user word fix from yesterday was the first layer.  Today's
work covers everything else: variables, constants, arrays,
template instances — all surviving across REPL evals.

## What the test surface looks like

`tests/session_persistence.rs` — 13 lock-in tests, all passing:

```
two_definitions_compose
redefining_a_word_updates_dictionary
definitions_chain_three_deep
multiple_constants_and_a_word_using_them
variable_defined_in_eval1_readable_in_eval2
two_variables_each_holds_its_own_value
variable_incremented_across_evals       (+!  across 3 evals)
variable_used_by_word_defined_later
mixed_state_compounds                   (constants + word + var + +!)
array_persists_across_evals
array_initialized_and_summed_in_loop
create_does_template_persists_and_instances_are_independent
basic_smoke_repl_evaluates
```

The shared `Repl` struct in the test file is what every test
drives — one Session, one CompileContext, multiple `r.eval(src)`
calls, then assert on captured output.  Exactly how the IDE
worker uses it.

## What needed fixing

### Variables — force wide in interactive mode

The narrow-variable optimization (Factor `SYMBOL: name` plus
`name @` peep'd to `name get-global`) requires whole-program
visibility — every use site has to be in the same compile so the
peep can rewrite them.

Across evals, eval N+1's reference to the variable name doesn't
go through the peep (it's a different compile).  Factor emits a
bare word call, which pushes the SYMBOL object onto the stack
(that's what `SYMBOL: name` makes `name` do).  Then `@` errors
with no-method.

The wide form is cross-eval-safe by construction: a `:` def that
returns the storage's nf-addr.  Eval N+1's reference calls the
def, gets the address, `@` dereferences normally.

**Fix**: in `compile_in_context`, mutate every variable's escape
state to `Wide` before emit:

```rust
for state in sema.escape.values_mut() {
    *state = sema::EscapeState::Wide {
        reason: sema::EscapeReason::InteractiveSession,
        at: dummy_span,
    };
}
```

Added `EscapeReason::InteractiveSession` for the dump-side
diagnostic.  The narrow optimization still applies to the batch
`compile()` driver (test runner, CLI), where whole-program
visibility holds.

### Templates — persist across compiles

Templates (CREATE/DOES> defining-words) need three pieces of
state:

1. The template itself (`: myarray create cells allot does> ... ;`)
2. Template instances (`4 myarray xs` → `Item::TemplateInstance`)
3. References to instances (`100 0 xs !`)

The template lifting happens at parse time, and the
`<n> templatename <newname>` triple recognition happens in
`expand_templates_pre_resolve`.  The triple-recogniser only saw
templates DEFINED IN THIS PROGRAM.  Cross-eval, eval 2's `4 myarray xs`
didn't expand because `myarray` was defined in eval 1.

**Fix**: extend `CompileContext` with
`templates: BTreeMap<String, TemplateDef>`.  Plumb through
`build_with_prior_and_templates → expand_templates_pre_resolve_with_prior`.
After each compile, merge `sema.templates` into `ctx.templates`.

### EscapeReason gained a variant

`EscapeReason` is the diagnostic that explains *why* a variable
was hoisted to Wide.  The existing reasons (Duplicated,
PassedToUnknownWord, AddressArithmetic, UnknownSink,
PrintedAsValue) describe escape from a single compile's
analysis.  The new reason is structural rather than analytic:
the variable might be referenced in subsequent evals we can't
see.  Added as `EscapeReason::InteractiveSession`.

## A bug found while writing the test (#53 filed)

While writing the CREATE/DOES> test, I tried the natural shape:

```rust
r.eval("0 xs @  1 xs @  0 ys @  1 ys @");   // push 4 values
r.eval(". . . .");                          // print them
```

The second eval crashed with an access violation.  Factor's
`(eval)` is called with a declared effect of `( -- )` —
enforcing that each eval is net-zero in stack balance.  Push
four values, the stack-effect check throws "wrong number of
results," and that escapes recover (or trips the alien-callback
boundary).  Process dies.

This is a fundamental REPL UX bug, not specific to templates.
In Factor's standalone listener you can type `5` and have it
sit on the stack; in our embedded path you can't.  Filed as
**#53**.  Fix is short — install a custom eval-callback that
uses `eval-with-stack` (basis/eval/eval.factor:33) instead of
`eval>string`.  Same VM-side machinery as the #47 alien-callback
issue, so they'll likely land together.

Workaround for the persistence test: keep each eval net-zero
by interleaving prints with reads — `0 xs @ . 1 xs @ .` etc.

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
session_tick_execute       5/5    ok
session_file_access        3/3    ok
session_test_runner        6/6    ok
session_let_lang          14/14   ok
session_crash_recovery     2/2    ok
session_repl_context       4/4    ok
session_persistence       13/13   ok    (NEW)
ans-core-corpus           61/61   pass
─────────────────────────────────────
Session-based + corpus   199/199 ok
smoke_runtime (legacy)    40/40   ok    (run separately, #31)
Grand total              239/239 ok
```

## Files touched

- `src/compiler/sema.rs` — `EscapeReason::InteractiveSession`,
  `build_with_prior_and_templates`,
  `expand_templates_pre_resolve_with_prior`
- `src/compiler/mod.rs` — `CompileContext.templates`,
  escape-state override in `compile_in_context_with_diagnostics`,
  templates merge
- `src/compiler/dump.rs` — new EscapeReason arm in the
  diagnostic dump
- `tests/session_persistence.rs` — 13 lock-in tests
- IDE binary rebuilt — variables and templates now work
  interactively

## What now works in the IDE

```
> variable counter
> 0 counter !
> : inc 1 counter +! ;
> inc inc inc
> counter @ .
3
> 4 array nums
> 10 0 nums ! 20 1 nums ! 30 2 nums !
> 0 nums @ 1 nums @ 2 nums @ + + .
60
> : multiplier create , does> @ * ;          (NOTE: , not yet shipped)
... but:
> : square-array create cells allot does> swap cells + ;
> 4 square-array xs
> 100 0 xs ! 200 1 xs !
> 0 xs @ . 1 xs @ .
100 200
```

The headline UX gap is closed.  REPL behaves like a Forth REPL
should: definitions, variables, constants, buffers, and
templates all accumulate across evals.

## Architectural observation

The pattern of "thread a context across compiles" is now used
for four things:

| Field | Carries | Why it persists |
|---|---|---|
| `user_words` | name → first-def span | so eval N+1 can reference eval N's words |
| `user_effects` | name → stack effect | so synth doesn't fall back to row-vars |
| `templates` | name → template body | so triple-expansion sees prior templates |
| (escape override) | (implicit) | so all variables go wide in interactive mode |

Each is a clean orthogonal piece.  Adding more shapes (e.g.
LOCALS frames if we ship #46-stragglers, or vocab imports if we
support per-isolate vocabs) follows the same pattern: a new
field in CompileContext, threaded through one more
`*_with_prior` function.

## What's still pending for full REPL fidelity

- **#53**: data stack values surviving between evals.  Today's
  workaround is "consume what you push in each eval."  The fix
  is a custom eval-callback that uses `eval-with-stack`.
- **#52** (this task) is now done; closes the most-immediate
  user complaint.

## Next from the roadmap

Per the priority list in `current_status.md`:

1. **Line-buffer flush at eval completion** (~30 min) — fixes
   the `.` trailing-space-no-newline display issue and probably
   the late-session crash the user observed.
2. **#53**: data stack between evals (~1 hr) — closes the next
   layer of REPL fidelity.
3. **#10 Mandelbrot v1** — now feasible end-to-end.
