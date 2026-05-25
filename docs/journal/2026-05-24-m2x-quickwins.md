# M2.x — quick-wins (resolver-only adds) + MOD fix (2026-05-24)

After the static gap-analysis (`docs/ans-gap-analysis.md`), the
biggest single finding was that ~20 ANS words were already defined
in `forth.runtime` but missing from `resolve.rs::builtin_table()` —
the resolver was the gate, not the runtime. This batch surfaces
them, and along the way fixes a real `MOD` bug.

## Shipped

### Resolver entries (table #38)

Added to `builtin_table()`:
- `DEPTH` — was in runtime, just needed exposure
- `>R` `R>` `R@` `RDROP` `2>R` `2R>` — return-stack family
- `U.` — unsigned print
- `S>D` `D>S` — identity in 64-bit cell model
- `D>F` `F>D` — int ↔ float bridge
- `F+` `F-` `F*` `F/` `F<` `F>` `F=` — float arithmetic (Factor polymorphic)
- `EXECUTE` — invoke an xt

### `MOD` semantics fix (#42)

ANS Forth `MOD` is **floored** (sign-follows-divisor). Factor's
`math:mod` is **truncated** (sign-follows-dividend). They disagree
when operands have mixed signs:

```
-7 mod 3   ANS = 2,   Factor = -1
 7 mod -3  ANS = -2,  Factor = 1
```

The runtime had `floored-mod` defined but the resolver pointed at
the wrong primitive. Worse, when I looked at the existing
`floored-mod` implementation, **it was buggy** — produced `-8` for
`-7 mod 3`. Rewrote with explicit hand-traced algorithm:

```factor
: floored-mod ( a b -- r )
    tuck mod
    dup 0 = [
        nip                              ! r=0: keep r
    ] [
        2dup 0 < swap 0 < xor            ! signs differ?
        [ + ] [ nip ] if                 ! yes: r+b ; no: just r
    ] if ;
```

Verified all four sign quadrants plus the zero case land correctly.

## Surfaced (and parked)

### `?DUP` — Factor inference rejects polymorphic effect

`?DUP` is inherently `( x -- 0 | x x )` — the OUTPUT stack effect
depends on the runtime value. Factor's static inference refuses
to compile `dup [ dup ] when` because the `if` branches differ in
stack height.

Tried `inline` annotation — same result. Tried row variables
`( ..a x -- ..b )` — same. The body simply doesn't type-check.

**Decision: drop `?DUP` from supported surface for now.** Modern
ANS code prefers `dup IF ... THEN` over `?DUP IF ... THEN`; the
loss is small. Filed as task #45 for a possible emit-time
inline-rewrite or `MACRO:` solution later.

### Lesson: a word "defined in runtime" doesn't mean "callable"

The gap analysis inventory listed words as 🟡 (defined but not
resolved). The inventory was over-optimistic — being defined as
a `: name ... ;` in `forth.runtime` doesn't guarantee Factor will
compile a call site that uses it. Effect-inference can reject
polymorphic patterns even when the word's body is syntactically
valid.

The verifier: each newly-exposed word now has a `T{ ... }T`-style
test in `tests/session_quickwins.rs` that compiles a `:`-def using
it and asserts the captured output. Words that don't pass don't
ship.

### Second lesson: emit needs concrete stack effects for `:` defs

When the synth path emits a `:` def with `( ..a -- ..b )` row
variables (because effect.rs couldn't infer), Factor's compiler
refuses to inline-compile against fixed-effect callees. The fix
was to add the new words to `effect.rs::builtin_effects()` so the
synth produces concrete `( N -- M )` instead.

This is a general pattern: any future word exposed via the
resolver also needs an entry in `builtin_effects()` or its callers
won't pass effect inference.

## What's next

Per the layered plan in the ANS gap doc:

- **#40 boolean convention** — must precede #39 (Core completeness)
  so newly-added comparators use the right convention from day one
- **#39 ANS Core completeness** — 1+, 1-, 2*, 2/, /MOD, */, */MOD,
  2DUP, 2DROP, 2SWAP, 2OVER, 0<>, U<, U>, LSHIFT, RSHIFT, COUNT,
  CMOVE>, -ROT, PICK, MOVE, ERASE, KEY?, EXIT, .S
- **#43 managed strings** — visible win, no design risk
- **#35 / #33 / #32 / #41** — test-suite substrate + runner
- **#44 LET** — the heavyweight

## Files touched

- `src/compiler/resolve.rs` — 14 new entries, 1 fix (MOD)
- `src/compiler/effect.rs` — 13 new effect entries
- `factor/forth/runtime/runtime.factor` — `floored-mod` rewrite,
  dropped `inline` on `>r/r>/r@/rdrop`
- `tests/session_quickwins.rs` — 7 lock-in tests, all passing
- `images/nf-mandelbrot.image` — rebuilt
