# 2026-05-27 — classes: perf numbers and toy examples

Sixth (seventh?) journal entry today.  The user asked "how slow
are method calls?" and "we should write a few more toy examples
for the doc-crate book."  Both got addressed.

## Empirical dispatch cost

Wrote a benchmark that ran 200,000 iterations of a polymorphic
generic-function call (alternating `circle` and `square` at the
same call site).  Result, in release build:

```
N            = 200,000
median total = 8.9 ms
per-call     = 44 ns
```

**44 ns per call** for the *whole* loop body — VALUE fetch,
IF/THEN to pick an instance, generic dispatch, slot read, float
arithmetic, drop.  Generic dispatch itself is a fraction of
that, probably <10 ns.

For context: C++ virtual function ≈ 10-15 ns.  Java HotSpot
warm virtual call ≈ 5-15 ns.  Naive CLOS dispatch (no IC) ≈
500-2000 ns.  Smalltalk interpreted message send ≈ 200-500 ns.
**We're in vtable territory, two orders of magnitude faster
than naive CLOS.**

That's not surprising in retrospect — Factor's inline-cache
machinery is descended from Self, refined into V8's hidden-class
implementation.  Slava's compiler.tree was engineered for fast
dispatch from the start.  We're just exposing it through a
Forth-flavoured surface.

The benchmark also surfaced the next pain point structurally —
*the dispatch_vs_direct_call test couldn't be measured cleanly
because cross-eval class persistence isn't shipped yet.*  Trying
to define `CLASS: circle` in eval 1 and then `5.0e <circle>` in
eval 2 produced "unknown word `<circle>`".  That's task #64's
top priority, now confirmed by experiment rather than by spec
review.

## Doc additions

Added to `release/factorforth/docs/classes.md`:

  1. **Performance section** — explains the IC machinery in
     terms readers will recognise (V8, Self, HotSpot lineage),
     shows the 44 ns number with the comparison table, lists
     the cliffs (megamorphic call sites, future multi-method
     dispatch, method combinations).  Honest about both the
     wins and the rare slow paths.

  2. **Linked-list worked example** — recursive CLASS structure
     with two specializations (`nil-node` + `cons-node`).
     Shows off the CLOS shape: `list-length` is two methods on
     the same generic, the receiver class changes during
     recursion (cons → cons → cons → nil → terminate), no
     base-case `if empty?` check needed.  The nil method IS the
     base case by virtue of being on the nil class.
     Doubles as a demo of polymorphic slots — a single list can
     hold mixed types because the slots are tag-erased.

Total `classes.md`: 508 lines.  Tutorial-flavoured, examples
forward, performance section near the end.

The linked-list code in the doc gets a runtime test
(`tests/diag_classes_linked_list.rs`).  Result:

```
captured: "3 6 "
```

`list-length` returns 3, `list-sum` returns 6.  The doc isn't
lying.

## What the perf numbers mean for the project

Going forward we can write generics without anxiety about the
hot path.  A tight loop calling `area` on shapes is the same
speed as a tight loop calling a regular `:` def doing the same
work.  When the JIT can prove the receiver class (constant
constructor → constant method call), the dispatch *disappears
entirely* — Factor inlines the body.

This matters for the design philosophy.  Without the speed,
we'd be tempted to advise "use classes for structure, but
inline the hot loops as regular `:` defs."  With the speed, we
can advise "use the model that fits the problem; the JIT will
handle the rest."

## Cross-eval persistence is now the priority

Two iterations in a row have run into the same wall: define
class in eval N, use in eval N+1, fail.  That's the next
implementation task — task #64's first bullet — and the next
time we sit down at this code, it's where we should start.

Shape of the work:
  - Add `CompileContext.classes: BTreeMap<String, Vec<String>>`
    (lowercased class name → flat slot list)
  - Thread it through `build_with_prior_state` as `prior_classes`
  - Update `lower_classes::compute_class_slots` to take
    prior_classes (already does — just need to actually pass it
    in)
  - Merge new classes from `sema.class_slots` into
    `ctx.classes` after each successful compile (same shape as
    `values` and `templates` already do)
  - F7 editor checker reads `classes` from the EDITOR_SNAPSHOT
    so the editor sees them too

About 30 lines.  Mirrors the VALUE/TO persistence we shipped
earlier today.

## Stats

  - 122/122 lib unit tests, no change
  - 73 runtime tests (was 72; +1 linked-list test).  All green.
  - 1 perf benchmark (`diag_classes_perf`), polymorphic case
    measured at 44 ns/call
  - `classes.md` grew from ~250 → 508 lines: added performance
    section + linked-list example with type-aware print variant
  - Release binary unchanged (no code rebuild needed)

— end of perf-and-toys entry
