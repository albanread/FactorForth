# M2.x #33 — `'` (tick) + EXECUTE for forth-level vectoring (2026-05-24)

The original #33 plan was DEFER/IS (ANS Programming-Tools).  When I
went to verify by reading ttester.fs — the canonical Forth 2012
test runner — it turned out ttester doesn't use DEFER/IS at all.
It implements its own vectored ERROR via a manual XT variable:

```forth
VARIABLE ERROR-XT
: ERROR ERROR-XT @ EXECUTE ;   \ vectored error reporting
\ later, to override:
' my-error  ERROR-XT !
```

That pattern needs three words:
- `VARIABLE`  ✓ (shipped M2.8)
- `EXECUTE`   ✓ (shipped M2.x #38)
- `'` (tick)  ← NEW

So this sprint ships `'` rather than DEFER/IS.  DEFER/IS is parked
for #49 as ANS Programming-Tools conformance, not a test-runner
blocker.

## What shipped

`Expr::Tick { name, span }` — a parsing-time form recognised in
parse.rs when the current token is `'`:

```rust
"'" => {
    self.bump();  // consume `'`
    let name_tok = self.peek().ok_or(...)?;
    let (name, end) = match &name_tok.kind {
        Tok::Word(w) => (w.clone(), name_tok.span.end),
        _ => return Err(...),
    };
    self.bump();  // consume the target name
    Ok(Expr::Tick { name, span: ... })
}
```

The resolver runs the same logic as `WordRef`: looks up the target
in user-defined or builtins, errors on unknown.  Effect is `(0, 1)` —
pushes one XT onto the stack.

Emit produces a Factor one-element quotation `[ <target> ]`:

```
Forth source:  ' say-hi execute
Factor IR:     [ say-hi ] forth.runtime:ans-execute
```

The reason `[ <target> ]` rather than `\ <target>` or `\ <target>
1quotation`: Factor's `call( -- )` (which `ans-execute` wraps)
reliably dispatches on a quotation but not on a bare word object
in the polymorphic-call path.  Wrapping in `[ ]` at emit time costs
nothing at runtime (Factor's optimiser elides the box where
possible) and gives the right semantics in every test.

## ans-execute update

```factor
: ans-execute ( xt -- )  call( -- ) ; inline
```

Previously `execute( -- )` — that's the word-call form.  Now
`call( -- )` so we route the quotation produced by tick.  Both
forms behave the same for normal user code; the change only
matters when ans-execute is invoked on a quotation.

## ttester pattern works end-to-end

```text
: dispatch  handler-xt @ execute ;
: impl-a    ." A!" ;
: impl-b    ." B!" ;
' impl-a handler-xt !  dispatch
' impl-b handler-xt !  dispatch
\ Output: "A!B!"
```

Asserted in `tests/session_tick_execute.rs::tick_xt_vectoring_through_helper`.

## Tests

5 new lock-in tests in `tests/session_tick_execute.rs`:

- `tick_then_execute_runs_user_word` — `' foo execute ≡ foo`
- `tick_xt_can_round_trip_through_stack` — two XTs pushed, executed
  in reverse order (XT is just a stack value)
- `tick_xt_stored_in_variable` — the ttester pattern
- `tick_xt_vectoring_through_helper` — verbose verification of the
  `: ERROR ... ERROR-XT @ EXECUTE ;` shape
- `tick_on_builtin_resolves` — `' cr execute` works (tick on a
  builtin word)

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
session_tick_execute       5/5  ok   (NEW)
─────────────────────────────────────
Session-based total       96/96 ok
smoke_runtime (legacy)    40/40 ok   (run separately, #31)
Grand total              136/136 ok
```

## Files touched

- `src/compiler/ast.rs` — `Expr::Tick { name, span }` variant
- `src/compiler/parse.rs` — `'` recognised in `expr_one`
- `src/compiler/resolve.rs` — Tick resolves like WordRef
- `src/compiler/effect.rs` — Tick has effect (0, 1)
- `src/compiler/emit.rs` — emit `[ <factor-name> ]`
- `src/compiler/dump.rs` — `Tick `name` @ <span>`
- `src/compiler/sema.rs` — Tick recorded in use-sites + call-graph
- `factor/forth/runtime/runtime.factor` — `ans-execute` switched
  to `call( -- )`
- `tests/session_tick_execute.rs` — 5 lock-in tests
- `images/nf-mandelbrot.image` — rebuilt

## What's next

**#32 file access** unblocks the test runner.  Plan: implement
the ANS File Access word set on top of Factor's `io.files`.
Critical word for #41: `INCLUDED` (read a file as Forth source
and eval it).
