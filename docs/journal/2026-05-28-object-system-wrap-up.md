# 2026-05-28 — object system wrap-up

After the multi-day object-system arc — classes, inheritance,
cross-eval persistence, multi-method dispatch, LET-methods, and
before/after method combinations — we've drawn the line.  The
system is feature-complete for practical Forth use, and the
remaining CLOS features are deliberate non-goals rather than a
backlog.

## What Factor4th's object system is

A CLOS-flavoured object system that rides Factor's own
substrate end to end:

- **`CLASS:` / `SLOT:` / `EXTENDS`** — record classes with named
  slots, single inheritance, auto-generated constructor, getters,
  and two setter forms (chainable `slot>>class` and ANS-style
  `class.slot!`).  Children inherit the parent's accessors.
  Lowers to Factor `TUPLE:` + `boa` + slot accessors.

- **`GENERIC:` / `METHOD:`** — generic functions with
  most-specific-wins dispatch.  Single- and multi-method dispatch
  are unified through `multi-methods`' `{ class-list }` syntax —
  a method specialising on N input positions dispatches on all N,
  with inheritance respected.

- **`METHOD-BEFORE:` / `METHOD-AFTER:`** — CLOS auxiliary method
  combinations.  Before-methods run most-specific-first, then the
  primary, then after-methods least-specific-first.  Implemented
  with shadow `:before` / `:primary` / `:after` generics plus a
  `::` locals wrapper; the no-aux path stays a plain generic with
  zero overhead.

- **LET-methods** — destructure tuple inputs directly in the LET
  binding list, so the infix-algebra DSL composes with classes.

- **Cross-eval persistence** — classes defined in one eval are
  usable in the next, through the REPL and the F7 checker alike.

## The principle that held throughout

*The Rust front end is grammar + desugar; the runtime substrate
is Factor's existing machinery.*  We never reimplemented
dispatch, slot storage, or method resolution — each Forth-surface
keyword desugars to Factor's tuple / generic / multi-methods
forms.  That's why the whole arc landed in one session of
compounding additive transforms rather than a ground-up object
runtime.

## What we consciously left out — and why

- **Multiple inheritance.**  Factor tuples are single-inheritance,
  and the user's verdict was decisive: "I hate multiple
  inheritance as a concept... composition is simpler anyway, I
  often think multiple inheritance was a mistake."  Agreed.  Not
  a gap — a non-goal.

- **`:around` / `call-next-method`.**  This was the one feature
  that would have required wandering off the Factor reservation:
  `multi-methods` has no `call-next-method`, so `:around` would
  mean synthesising a parallel dispatch mechanism.  The user's
  steer — "let's not wander far from the Factor reservation" —
  drew the line right where it belongs.  `:before` / `:after`
  already cover the practical uses (guards, logging, audit,
  notification); `:around`'s marginal cases (memoisation,
  transactions) don't justify a second dispatch engine.

- **Metaobject protocol / metaclasses.**  Nobody needs the MOP in
  a Forth.

## If we ever extend further

Three small additions would still ride Factor's native tuple
machinery — no synthesised dispatch, so they stay on the
reservation:

1. Slot initial values — `{ slot initial: v }` is native to
   Factor `TUPLE:`.
2. Typed slots — `{ slot integer }` is native.
3. `CLASS-OF` / class-membership predicates — Factor's
   `class-of` is one word.

Plus cross-eval aux methods (a `CompileContext` persistence
task), if REPL-line-at-a-time aux definition ever becomes a real
need.  All optional; none blocking.

## "Same eval" means "same file"

A clarification from the user that's worth recording: the
"same-eval" requirement on aux methods isn't a real limitation
for how anyone organises code.  When you `INCLUDED` a `.f` file
the whole thing compiles as one unit, and the F7 checker
compiles the whole editor buffer at once — so a class, its
generics, primaries, and aux methods naturally land together.
The restriction only surfaces if you type a `GENERIC:` on one
live-REPL line and a `METHOD-BEFORE:` on a later line.  The file
is the natural unit of a class definition, and the compiler
rewards keeping it that way.

## Stats for the arc

  - 89 runtime/diag tests across the object system + supporting
    features
  - 127 lib unit tests, all green
  - One image rebuild (to bake in `multi-methods`), ~1 MB
  - Committed as one cohesive object-system commit, b639c85,
    52 files, +6057 / -105

## Reflection

The user, a 50-year Forth hobbyist, framed the whole project at
the start: "I would never have written a lot of forth if the
forth I was using did not have a decent class system to help
organize my work, vocabs of words are like frogs in a bucket."
This is the system that puts a lid on the bucket — and it did so
without building a single line of bespoke object runtime, by
treating Factor's CLOS-descended machinery as the substrate and
ANS Forth as the surface.

The two load-bearing user instincts across the arc:
  - "do it right or why do it?" — which got `multi-methods` baked
    into the image instead of hacked around.
  - "let's not wander far from the Factor reservation" — which
    drew the feature line at exactly the point where we'd have
    started reimplementing Factor instead of using it.

Both pointed the same direction: lean on the substrate, keep the
front end thin.  That's the through-line, and it's why an object
system this capable fit in the time it did.

— end of object-system wrap-up entry
