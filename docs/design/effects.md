# Stack effects — how inference works

Status: **describes shipped code** (`src/compiler/effect.rs`, Phase 2.7,
as of 2026-05-29). This is the contributor's-eye view of the
mechanism; for the user-facing "how to read/write `( a b -- c )`" guide
see `release/factorforth/docs/stack-effects.md`.

## Why effects exist at all

It would be easy to assume the effect pass is just a linter that warns
when your `( a -- b )` comment is wrong. It does that — but that is the
*secondary* job. The load-bearing reason is the **emitted Factor IR**.

Factor's `:` **requires** a stack-effect annotation, and Factor runs its
own *strict, typed* stack-effect inferencer at compile/JIT time. If we
emit a definition whose annotation is the row-variable form
`( ..a -- ..b )` ("any stack effect"), Factor accepts it but can no
longer inline it against fixed-effect callees, and the
`compiler.tree` SSA / float-unboxing / inline-cache optimisations don't
fire. Worse, a row-var word called from a fixed-effect context can make
Factor *refuse* the compile.

So our effect pass exists primarily to **synthesise a concrete
`( N -- M )` annotation for every colon definition**, derived from the
body so it's correct by construction, so that Factor's inferencer is
happy and the JIT can do its job. The mismatch warnings are a useful
by-product of having to compute the real effect anyway.

> **Two effect systems.** Ours (Rust, in `effect.rs`) is *count-based*
> and *permissive* — it produces warnings and falls back to `Unknown`.
> Factor's (in the VM) is *typed* and *strict* — it hard-errors. Ours
> runs first and feeds Factor an annotation Factor will accept. Keep
> the two straight: when this doc says "the effect," it means our
> count-based net effect, not Factor's row/type analysis.

## What an effect is

```rust
enum Effect {
    Known { inputs: u32, outputs: u32 },   // consumes `inputs`, leaves `outputs`
    Unknown,                                // couldn't / chose not to size it
}
```

Deliberately minimal:

- **Counts, not types or names.** ANS effect-comment items can carry
  names (`n^2`, `c-addr`) — we ignore them for inference and only count.
  The names survive only as *documentation* in the emitted annotation
  (see the decision table below).
- **Data stack only.** The floating-point stack and the return stack
  are **not** modelled. The return-stack words carry their *data-stack*
  effect only: `>r` is `(1 -- 0)`, `r>` is `(0 -- 1)`, `2>r` is
  `(2 -- 0)` — the r-stack movement is invisible here by design.
- **`Unknown` is all-or-nothing.** There is no row-variable arithmetic.
  The moment a body hits something unsizable, the whole body's effect
  collapses to `Unknown` (and the body-vs-declared check is skipped).

## The composition algebra

`Effect::then` threads one effect into the next — the whole of
straight-line inference is a left fold of `then` over the body:

```rust
fn infer_block(exprs) -> Effect {
    let mut acc = Known{0,0};
    for e in exprs { acc = acc.then(effect_of(e)); if acc == Unknown { return Unknown } }
    acc
}
```

`then` has two cases. Given `self = (ai -- ao)` followed by
`next = (bi -- bo)`:

- **`next` fits in what `self` left** (`ao >= bi`): the surplus stays.
  Result `( ai -- ao - bi + bo )`.
- **`next` needs more than `self` left** (`ao < bi`): it dips into the
  pre-state, so the whole thing requires `bi - ao` extra inputs.
  Result `( ai + (bi - ao) -- bo )`.

Either side being `Unknown` makes the result `Unknown`.

Worked example — `+ +` (each `+` is `(2 -- 1)`):

```
(0--0) then (2--1)  →  ao=0 < bi=2  →  ( 0+(2-0) -- 1 ) = (2--1)
(2--1) then (2--1)  →  ao=1 < bi=2  →  ( 2+(2-1) -- 1 ) = (3--1)
```

So `: chain  + + ;` infers `( 3 -- 1 )`. (If the user declared `( -- )`
on it, that's a `Mismatch` warning.)

## The builtin effect table

`builtin_effects()` is a `HashMap<&str, Effect>` parallel to
`resolve::builtin_table()` — same lowercased ANS keys, but the value is
the net count rather than the Factor target. Every builtin resolve
knows about has an entry here (`dup` = `(1 -- 2)`, `swap` = `(2 -- 2)`,
`+` = `(2 -- 1)`, `.` = `(1 -- 0)`, …).

`?dup` is **deliberately absent** from both tables: its effect is
stack-polymorphic (`( x -- 0 | x x )`), which neither our counts nor
Factor's strict inference can size. A reference to it resolves as
`Unknown` and (today) doesn't resolve as a builtin at all.

A `WordRef` gets its effect by: if resolve bound it to a `UserDefined`
target, look it up in `user_effects`; otherwise look it up in
`builtin_effects`. Missing in both → `Unknown`.

## The inference driver — two passes

`infer_with_prior(resolved, prior_effects)` returns:

```rust
struct Inferred {
    user_effects: HashMap<String, Effect>,  // the CALLER's view
    body_effects: HashMap<String, Effect>,  // the GROUND TRUTH (body walk)
}
```

The two maps are the crux. **`user_effects`** is what a caller types
itself against: the *declared* effect when one is present (so mutual
recursion and forward references work before bodies are walked),
inferred otherwise. **`body_effects`** is what the body actually does,
independent of any (possibly stale) declaration. Emit trusts
`body_effects`; callers trust `user_effects`.

**Pass 1 — seed declarations.** Start from `prior_effects` (cross-eval,
below), then for every item record a starting effect:

- `Definition` with a declared `( … )` → those counts; without → `Unknown`.
- `Variable` / `Constant` / `Value` / `Create` → `(0 -- 1)` (push one).
- `Collection` / `TemplateInstance` → `(1 -- 1)` (`idx -- addr`).
- `Generic` → its declared effect (required on generics).
- `Class` name → `(0 -- 0)`; the constructor/accessor/predicate effects
  are seeded separately in sema (it needs the flattened slot list).

Seeding declarations *first* is what lets `: a … b … ;` and
`: b … a … ;` both type-check: each sees the other's declared effect.

**Pass 2 — walk bodies.** For each `Definition` in source order, run
`infer_block` over the body to get `body_eff`, record it in
`body_effects`, run `check_definition` (declared-vs-body → maybe a
`Mismatch`), collect CASE warnings, and — *only if the def had no
declared effect* — promote `body_eff` into `user_effects` so later
callers can use it. Forward references therefore use the *declared*
seed from pass 1, not a body effect that hasn't been computed yet.

## Control-flow formulas

`effect_of_expr` handles each structured form with a closed formula
over its sub-body effects. The recurring constraint is **join-point
balance**: where two paths merge, they must leave the same shape.

| form | rule | result |
|------|------|--------|
| `IF … THEN` (no else) | then-body must be balanced (`i == o`) | `(1+i -- i)` — the `1` is the flag |
| `IF … ELSE … THEN` | both branches must agree (`join_branch_effects`) | `(1+i -- o)` |
| `BEGIN … UNTIL` | body produces a flag (`o == i+1`) | `(i -- i)`; if `o > i` more generally → `(i -- o-1)` |
| `BEGIN … WHILE … REPEAT` | pred `(i -- i+1)`, body `(i -- i)` | `(max -- max)` |
| `BEGIN … AGAIN` | infinite loop, no normal exit | **always `Unknown`** |
| `DO … LOOP` / `?DO` | body balanced (`i == o`) | `(2+i -- i)` — consumes limit+start |
| `CASE … ENDCASE` | recursive over arms; each arm consumes the dispatch flag | `Known` if arms + default agree, else `Unknown` |

Any sub-body that is `Unknown`, or any violated balance constraint,
yields `Unknown` for the whole form (and so for the enclosing def).
That's *conservative but safe*: an `Unknown` def emits with a trusted
declared annotation, or the row-var fallback.

Leaf effects: int/float literal `(0 -- 1)`; strings by kind — `."`
`(0 -- 0)`, `S"` `(0 -- 2)`, `C"`/`S$"` `(0 -- 1)`; `'` tick `(0 -- 1)`;
`TO` `(1 -- 0)`; `LET` `(inputs -- outputs)` from its form; `SEE` `(0 -- 0)`.

### CASE wants a total dispatch

`CASE` with no `DEFAULT`/`OTHER` arm gets a `CaseNeedsDefault` warning
when its arms leave a different shape than the no-match path (which is
`( disc -- )`, i.e. it just drops the discriminant). A value-producing
`CASE` therefore needs an explicit default to be `Known`; a
statement-style `CASE` (arms net `(1 -- 0)`) is fine without one.

## How the annotation gets emitted

`emit_definition` picks the annotation from `(declared_counts, synth)`
where `synth = body_effects[name]`. The principle: **synth is
authoritative** (correct by construction) **but the declaration carries
names worth keeping**.

| declared | synth | emitted annotation |
|----------|-------|--------------------|
| present | `Known`, counts match | **declared** — keep the user's names (`n^2`, `c-addr`) |
| present | `Known`, counts differ | **synth** — counts win; the `Mismatch` warning already fired |
| present | `Unknown` | **declared** — synth can't speak, trust the user |
| absent | `Known` | **synth** — synthesise `( a b -- r0 r1 )` |
| absent | `Unknown` | **`( ..a -- ..b )`** — row-var fallback: give up, accept any |

Synthetic names are positional and meaningless to Factor: inputs
`a b c …`, outputs `r0 r1 …` (`synth_effect_annotation`). Declared
names are sanitised for Factor's parser — `...` becomes `_dots_`, and
any `.` in a name becomes `_`.

The row-var fallback `( ..a -- ..b )` is the escape hatch for `Unknown`
defs (infinite loops, unsizable control flow, calls to `Unknown`
words). It keeps the compile alive but, as noted up top, costs
inlining and composition — so the inferencer earns its keep every time
it can produce a concrete `( N -- M )` instead.

## Diagnostics are warnings, not errors

`EffectError` is a misnomer kept for historical reasons — every variant
is a **warning**. `Mismatch` (declared ≠ body) and `CaseNeedsDefault`
are surfaced by the CLI/IDE but **do not fail the compile**: it still
emits valid IR using the synthesised effect, which is correct by
construction. `compile_with_diagnostics` returns `(ir, warnings)`;
`compile`/`compile_in_context` discard the warnings. Lex/parse/resolve
errors, by contrast, are hard failures (no IR is produced).

Rationale: Forth culture is permissive about ambiguous shapes, and the
synth is right regardless of what the comment claims — so a drifted
annotation is a documentation bug, not a build breaker.

## Cross-eval seeding

In the REPL/IDE, each eval is a separate compile. `infer_with_prior` is
seeded with `CompileContext.user_effects` — the effects of words
defined in earlier evals. Without this, a reference in eval N+1 to a
word from eval N would look `Unknown`, the enclosing def would fall
back to row-vars, and Factor's strict inference could then reject it.
The context is updated after each compile with whatever inference
figured out, so the dictionary's effects persist across the session
exactly like its names do.

## What is intentionally not modelled

- **The FP and return stacks.** Data-stack counts only. `>r`/`r>` look
  like `(1 -- 0)`/`(0 -- 1)`; a word that stashes on the r-stack and
  retrieves it later nets out correctly on the data stack but the
  r-stack itself is invisible.
- **Types.** Counts only — `dup` on a float and `dup` on an int are the
  same `(1 -- 2)`. Type safety, where it matters, is Factor's job.
- **Stack-polymorphic words** (`?dup`, and anything `( x -- 0 | x x )`):
  unrepresentable as a single count → absent from the table → `Unknown`.
- **Row-variable arithmetic.** We never track "the rest of the stack"
  symbolically; `Unknown` is the only fallback, and it's coarse.
- **Nested/again loops.** `BEGIN … AGAIN` is always `Unknown`; deeply
  nested control flow often collapses to `Unknown` and relies on the
  declared annotation or the row-var fallback.

These are deliberate first-cut boundaries (Phase 2.7). The system is
good enough to give Factor concrete annotations for the overwhelming
majority of real defs, and to warn when a comment has drifted — which
is all it's there to do.
