# 2026-05-27 — before/after method combinations

Tenth journal entry today.  Following yesterday's multi-method
dispatch landing, the natural next thing was `:before` /
`:after` auxiliary methods — the CLOS sequencing primitives
that let you bolt invariant checks, logging, audit trails, and
post-commit notifications onto an existing generic without
touching the primary method's body.

## What landed

Two new Forth-surface keywords: `METHOD-BEFORE:` and
`METHOD-AFTER:`.  They look exactly like `METHOD:` but the
declared effect's outputs are empty (the aux methods always
return nothing — their work is side-effecting):

```forth
CLASS: account ;

GENERIC: withdraw ( a amount -- )

METHOD-BEFORE: withdraw ( a:account amount -- )
    drop balance>account swap < abort" insufficient funds" ;

METHOD: withdraw ( a:account amount -- )
    swap balance>account swap - swap balance!! ;

METHOD-AFTER: withdraw ( a:account amount -- )
    2drop ." audit: withdrawal recorded" cr ;
```

The dispatcher runs them in CLOS order: before-methods first
(most-specific to least-specific), then the primary, then
after-methods (least-specific to most-specific).  The primary's
return value is what the caller sees; the aux methods' returns
are discarded.

## How it works under the hood

Each `GENERIC:` that has at least one aux method this compile
expands into FOUR Factor declarations plus a wrapper word:

```factor
multi-methods:GENERIC: withdraw:primary ( a amount -- )
multi-methods:GENERIC: withdraw:before  ( a amount -- )
multi-methods:GENERIC: withdraw:after   ( a amount -- )

! Object-default no-op methods so dispatch never fails when
! no aux method matches the actual class:
multi-methods:METHOD: withdraw:before { object object } 2drop ;
multi-methods:METHOD: withdraw:after  { object object } 2drop ;

! The wrapper — uses Factor's `::` locals form to hold the
! arguments across before/primary/after.  Locals avoid the
! stack juggling we'd otherwise need to thread N inputs and
! M outputs through three separate dispatches:
:: withdraw ( a0 a1 -- )
    a0 a1 withdraw:before
    a0 a1 withdraw:primary
    a0 a1 withdraw:after ;
```

Primary methods route to `withdraw:primary`, before-methods to
`withdraw:before`, after-methods to `withdraw:after`.  When no
aux methods exist this compile, none of this machinery is
generated — the no-aux path stays a plain `GENERIC:` /
`METHOD:` pair and pays no wrapper overhead.

The wrapper synthesises local names (`a0`, `a1`, ..., `r0`,
`r1`, ...) rather than reusing the user's effect-comment
names, since those may not be valid Factor identifiers.  For
M=1 output we use `:> r0`; for M>1 we use `:> ( r0 r1 ... )`.

## Why locals, not stack juggling

For a generic with N inputs and M outputs, the wrapper has to
make three calls and preserve the N inputs across before AND
between primary and after, then return the M outputs from
primary.  In point-free stack code that's a mess of `Ndup` and
`Ndip` combinators that grows quadratically with N+M.

With `locals`, it's linear:

```
a0 a1 before        \ before consumes a0 a1, returns nothing
a0 a1 primary :> r0 \ primary consumes a0 a1, captures r0
a0 a1 after         \ after consumes a0 a1, returns nothing
r0                  \ return the primary's result
```

`locals` is in basis (bootstrap), so this costs zero
distribution bytes; it's an `IMPORT` away.

## Same-eval limitation

Sprint 1 of aux methods restricts the generic and all its aux
methods to live in the **same** compile.  The reason: at
generic-emit time we need to know whether to emit the plain
shape (`GENERIC: foo`) or the wrapper shape
(`foo:primary` + `foo:before` + `foo:after` + `:: foo`).  We
decide that by pre-scanning this compile's items for any aux
methods.  An aux method appearing in a *later* eval — after
the generic was already emitted in the plain shape — can't
be retroactively wrapped.

The fix is doable: persist a per-generic aux-flag in
`CompileContext`, and on a later-eval aux method, redefine
the generic word in Factor.  But it's a follow-up — the
common case (defining the generic and its aux behaviour
together) doesn't need it.

**What "same eval" means in practice.**  The user put it
sharply: "same eval really means that the class and its
methods should be defined in a file, doesn't it."  Exactly.
When you `INCLUDED` a `.f` file, the entire file compiles as
one unit — one eval — so the generic and all its aux methods
land together automatically.  The same is true of the F7
checker, which compiles the whole editor buffer at once.  The
restriction only bites in the live REPL when you type
`GENERIC: foo` on one line, hit enter (one eval), then type
`METHOD-BEFORE: foo` on a later line (a second eval).  For
file-based and editor-based workflows — which is how anyone
actually organises a class and its behaviour — the
"limitation" is invisible.  It's a REPL-interactivity edge,
not a structural one.  Which is itself an argument for the
file being the natural unit of a class definition: slots,
generics, primaries, and aux methods all belong in one place,
and the compiler rewards keeping them there.

## What changed

- **`src/compiler/ast.rs`** — added `MethodKind` enum
  (`Primary`/`Before`/`After`) and the `kind` field on
  `MethodDef`.

- **`src/compiler/parse.rs`** — recognise `METHOD-BEFORE:`
  and `METHOD-AFTER:` as keywords; `method_definition` takes
  a `kind` argument and uses it to tag the produced
  `MethodDef`.

- **`src/compiler/emit.rs`** — `aux_generics()` pre-scans
  items.  `emit_generic` and `emit_method` consult the set:
  generics with aux get the wrapper shape, primary methods on
  those route to `:primary`, before/after to the shadow
  generics.  `vocabs_needed` adds `locals` when any aux
  method is present.

- **`tests/diag_method_combinations.rs`** — 7 new tests
  covering:
  - no-aux fast path still works
  - before runs before primary
  - after runs after primary
  - both together (sequencing `before primary after`)
  - primary return value passes through
  - multi-input generic (2 in / 1 out)
  - before on a non-matching class is a no-op (default
    method fallback)

## What this unblocks

The natural next steps from yesterday's list:

1. **`SUPER:` / `call-next-method`** — within a method body,
   invoke the next-most-specific applicable method on the
   same generic.  Multi-methods already implements
   `call-next-method`; we'd expose it as a Forth-surface
   word.

2. **`:around` method combinations** — these wrap the call.
   With aux methods now in place, `:around` is a similar
   shape with an additional "call-next" inside the wrapper.

3. **Slot `:initform`** — default values for slots.

The architectural pattern continues to hold: the Rust front
end stays grammar + desugar; the substrate is Factor's
existing machinery (multi-methods + locals); each new CLOS
feature is a small additive transform on the same desugar
table we've been compounding.

## Stats

  - 82 runtime tests → 89 (+7 method-combination tests)
  - 127 lib unit tests, all green
  - 11 lines added to `ast.rs`, 26 to `parse.rs`, ~140 to
    `emit.rs`
  - No image rebuild needed — `locals` is in basis, and
    `multi-methods` was baked in yesterday

## Reflection

The user's note from yesterday was load-bearing for today
too: "*do it right or why do it?*"  The aux-method machinery
could have been shoehorned in as a parse-time desugar that
rewrites the user's `METHOD-BEFORE:` body into an `IF
class? THEN ... ELSE` chain bolted onto the primary.  That
would have shipped sooner.

Instead the structural answer — separate shadow generics,
a single wrapper word, full polymorphic dispatch — keeps
the same semantics CLOS users expect.  It also unlocked
`call-next-method` and `:around` as straightforward
extensions of the same machine, rather than a parallel
re-implementation per feature.

— end of before-after-methods entry
