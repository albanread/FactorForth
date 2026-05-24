# 2026-05-24 — Effect diagnostics: synth is authoritative, declared is documentation, mismatches are warnings

**Shipped**

- Effect inference handles control flow (IF/THEN, IF/ELSE/THEN,
  BEGIN/UNTIL, BEGIN/WHILE/REPEAT, DO/LOOP — see formulas in
  `effect.rs`).  CASE remains Unknown for now.
- Stack effects are *always* emitted on every `:` definition.
  Decision table picks the right source: declared if it matches
  synth (preserves user's documentation names), synth if they
  disagree (truth wins), declared if synth is Unknown, row-vars
  if neither is available.
- Effect-mismatch errors are downgraded to warnings.  Compile
  produces IR with the synth annotation; warnings flow to stderr
  from the CLI.  Suite stays green: 87 lib + 26 integration.

**Open**

- CASE effect formula (the per-arm `dup MATCH = if drop body else …`
  is tangled enough to deserve its own treatment).
- IDE warning surface — for now warnings just go to stderr.
- "Ambiguous effect" warning when no declaration AND synth Unknown
  is currently silent; could be loud when desired.

---

## The conversation

We started with M2.7's first cut: declared/inferred mismatch =
hard compile error.  The user pushed on two reframings.

First: **we synthesise effects from bodies, so we can always
write a real annotation.**  That replaces my placeholder
`( ..a -- ..b )` fallback with computed counts.

Then the deeper observation: **the synth is more believable than
what the user types.**  Bodies are concrete; annotations are
claims about bodies that can drift.  When they disagree, the body
wins.  The user's declaration becomes documentation, not law.

And finally: **in our IDE we warn, invalid or ambiguous stack
effect.**  Forth's culture is permissive — users sometimes write
genuinely ambiguous programs and expect the tool to be a partner,
not a gatekeeper.  Effects-as-warnings honours that.

## What changed

### Three sources of truth, three priorities

`emit_definition` now picks among:

| declared    | synth (body)    | what we emit                  |
|-------------|-----------------|-------------------------------|
| present     | Known, matches  | declared (keep names)         |
| present     | Known, differs  | **synth** (truth wins)        |
| present     | Unknown         | declared (best we have)       |
| absent      | Known           | synth                         |
| absent      | Unknown         | `( ..a -- ..b )` (row-vars)   |

The user's names like `n^2`, `c-addr`, `flag` carry meaning the
synth can't recover — so we preserve them when they're correct.
When they're wrong (declared 0 outputs but body produces 2), we
quietly emit the synth's `( -- r0 r1 )` and surface a warning.

Critically this required *two* maps in Sema:

```rust
pub user_effects: HashMap<String, Effect>,   // caller's view (declared if any)
pub body_effects: HashMap<String, Effect>,   // ground truth from body walk
```

Pass 1 of effect inference populates `user_effects` with the
DECLARED effect (so mutual recursion can type itself against
declarations).  Pass 2 walks bodies and populates `body_effects`
with the ground truth.  Emit reads `body_effects` for its
decision; `user_effects` stays for caller typing.

Initially I had only `user_effects` and tried to use it for both
purposes.  That's why the synth-wins logic silently did nothing:
when a user declared `( -- )` for `: bad ... 1 2 ;`, Pass 1 wrote
`Effect::Known { 0, 0 }` into user_effects.  Pass 2 inferred
`Known { 0, 2 }` for the body but only updated `user_effects` when
no declaration was present.  Emit asked for "synth", got `(0,0)`
from user_effects, saw it match the declaration, and emitted the
declared annotation.  The two roles conflict; they need two maps.

### Mismatch is a warning

```rust
pub fn compile_with_diagnostics(source: &str)
    -> Result<(String, Vec<EffectError>), String>
```

`compile()` keeps its old signature (returns just the IR) by
discarding warnings.  The new function returns both.

The CLI calls `build_sema` and prints `sema.effect_errors` to
stderr after the compile.  Tests can check for specific
diagnostics via the new function.

### Control-flow effect formulas

In `effect.rs::effect_of_expr` the control-flow variants now have
real rules instead of returning Unknown:

```text
IF/THEN          body must be (n -- n)        →  (1 + n -- n)
IF/ELSE/THEN     branches must match (i -- o) →  (1 + i -- o)
BEGIN/UNTIL      body (i -- i+1)              →  (i -- i)
BEGIN/WHILE/RPT  pred (i -- i+1), body (i--i) →  (max(i) -- max(i))
BEGIN/AGAIN      never returns                →  Unknown
DO/?DO/LOOP      body (i -- i)                →  (2 + i -- i)
CASE             vacuous                      →  (1 -- 0)
                 otherwise                    →  Unknown (formula deferred)
```

Each returns Unknown if its sub-bodies don't fit the shape it
needs.  Conservative.

CASE got a quick-fail Unknown because the per-arm formula —
which has to account for the dispatch dup, the match value, the
equality consume, the drop on match, the structural recursion
through arms — is genuinely involved and I want to get it right
in a separate sub-milestone, not rush it.

## What the user pushed me to see

The "synth is more believable" observation is small but it
reorders the design philosophy:

- **Old framing:** the user is the source of truth; the compiler
  verifies their claim.  Mismatch = the user is wrong, reject.
- **New framing:** the body is the source of truth; the compiler
  derives the contract from it.  The user's annotation is a
  comment that *describes* the contract.  Mismatch = the comment
  is stale, warn.

It's the same move as `git blame` (the code is what runs, the
comment is what we hoped) applied to ANS effect annotations.
Once stated, it's obvious; before stating, I'd been treating
declarations as law.

The "warn don't fail" follows naturally.  If the body is truth
and the comment is documentation, then a mismatched comment is
just stale prose.  Warn the user; don't block them.

## Pipeline state

```
ANS Forth source
  ↓  lex     ✓
  ↓  parse   ✓
  ↓  sema    ✓ resolve, effect, escape, call graph, use sites
  ↓  emit    ✓ uses body_effects to pick annotation; warns on mismatch
  ↓  eval    ✓
  → output + warnings (stderr)
```

`compile()` always returns Ok when the lexer/parser/resolver
succeed.  Diagnostics ride along on the side.  The CLI surfaces
them; the future IDE will too.

## What I still want to do

- **CASE effect formula.**  Per-arm structure deserves its own
  derivation; current "Unknown for any CASE" is overly
  conservative.
- **"Ambiguous" warning.**  No-declaration + Unknown-synth
  silently emits row-vars today.  Could be a warning visible by
  default, gated by a verbose flag, or always-on once CASE is
  done (then ambiguous would be rare).
- **Warning categories.**  Right now all effect issues are one
  variant.  When more kinds appear (unused word, unreachable
  branch, dead constant) we'll want a tagged severity system.
