# 2026-05-27 — multi-method dispatch (and an image rebuild)

Ninth journal entry today.  The big fish: multi-method dispatch
landed.  Methods that specialise on N classes at once, dispatch
choosing the most-specific applicable method per call.  This is
the central CLOS feature, and the thing that distinguishes
"CLOS-flavoured object system" from "Java-with-better-syntax."

## The user's sharp question that unlocked it

When I first tried switching emit to `multi-methods` syntax, the
runtime captured nothing — Factor's `USE: multi-methods` failed
silently because the vocab lives in `extra/`, not `basis/`, and
our deployed image had no file-system access to the Factor
source tree.

The user's response cut through the analysis: *"we have a
massive hug 128 MB of factor is some walled off?"*

The 128 MB had all of `basis/` (Factor's standard library) but
nothing from `extra/`.  The fix was structural: rebuild the
image with `multi-methods` baked in.  ~1 MB more on disk, no
extra/ folder needed at runtime, every CLOS-shaped feature
from here onward sits in that vocab.

We chose option (a) — do it right — and rebuilt.

## What changed

**`scripts/build-image.sh`** — added `USE: multi-methods` so the
vocab is loaded during bootstrap.  Image: 134.7 MB → 133.8 MB
(actually slightly smaller, probably because the rebuild cleaned
up some uncompacted bytecode).  Compressed `.zst`: 17.4 → 17.5
MB.  Negligible distribution delta.

**`src/compiler/emit.rs`** — emit_method and emit_generic
switched to multi-methods syntax:

```factor
multi-methods:GENERIC: name ( inputs -- outputs )
multi-methods:METHOD: name { class1 class2 ... } body ;
```

The fully-qualified `multi-methods:` prefix avoids the
name-collision warning with standard `generic`'s `GENERIC:`.
For single-dispatch methods we pass a one-element class list
`{ class1 }`; for multi-dispatch any number of classes.  No
new keyword needed in the Forth surface — the same
`GENERIC:` and `METHOD:` cover both cases.

Order matters in the multi-methods syntax — generic name FIRST,
then the class list.  My first try had it reversed (`METHOD: {
class } generic body ;`) and the test silently failed to
dispatch.  The test fixtures in `factor-src/extra/multi-methods/
tests/syntax.factor` clarified the actual shape.

## What works now

```forth
CLASS: rock     ;
CLASS: paper    ;
CLASS: scissors ;

GENERIC: beats? ( a b -- ? )

METHOD: beats? ( a:paper    b:rock     -- ? )  2drop -1 ;
METHOD: beats? ( a:scissors b:rock     -- ? )  2drop  0 ;
METHOD: beats? ( a:paper    b:paper    -- ? )  2drop  0 ;
\ ... etc, 9 methods total covering every pair ...

<paper>    <rock>     beats? .   \ -1 (true — paper beats rock)
<scissors> <rock>     beats? .   \  0 (false)
<rock>     <paper>    beats? .   \  0
<paper>    <paper>    beats? .   \  0
```

The dispatch is real CLOS dispatch: each call inspects the
classes of both top-of-stack values and finds the most-specific
applicable method.  Inheritance works too — a method declared on
`(animal, animal)` matches `(cat, dog)` because both inherit
from `animal`, while a method on `(cat, cat)` wins for two cats
because it's more specific.

Verified by three tests:
  - **rock_paper_scissors** — 9 methods on 3×3 class combos
  - **geometric_intersect** — `line/line`, `line/circle`,
    `circle/line`, `circle/circle` each get their own method
  - **multi_dispatch_specificity** — `(animal, animal)` general
    method + `(cat, cat)` specific method, calls dispatch to
    the most specific applicable

## Side effects of the image rebuild

  - Existing single-dispatch class tests now go through
    `multi-methods` too — all 15 still pass (no behavioural
    difference for one-class methods)
  - One method-on-multi-arg generic that previously required
    nested manual dispatch can now be written naturally
  - Future CLOS features (`:before` / `:after` / `:around` /
    `call-next-method`) live in the same vocab and are
    unblocked

## What's now possible (and easy to land next)

Now that `multi-methods` is in the image, the rest of the CLOS
auxiliary-method machinery is mostly a syntactic-surface
problem in our parser + emit.  In rough order of usefulness:

1. **`:before` / `:after` method combinations** —
   `METHOD-BEFORE:` / `METHOD-AFTER:` Forth-surface keywords,
   emit Factor's existing `BEFORE:` / `AFTER:` syntax.

2. **`SUPER:` / `call-next-method`** — within a method body,
   invoke the next-most-specific applicable method on the
   same generic with the same arguments.  Factor has
   `call-next-method`; we'd expose it as a Forth word.

3. **`:around` method combinations** — wrap the call.  Less
   common but unblocks aspect-oriented patterns (memoization,
   transactions, instrumentation).

4. **Slot `:initform`** — default values for slots.  Quality
   of life.

Each one is a small additive change on the same desugar
pattern we've been compounding.

## The image rebuild was cheap

  - Two minutes wall clock for `bash scripts/build-image.sh`
  - 42 seconds to re-compress with zstd-19
  - All 82 runtime tests + 122 lib tests still green after
  - Distribution `.zst` re-published, sized at 17.5 MB (was 17.4)

The "rebuild the image" step had felt large in prospect; it
turned out trivial in practice.  The right architectural answer
won by being simple to execute, not just by being right.

## Stats

  - 79 runtime tests → 82 (+3 multi-dispatch tests)
  - 122 lib unit tests, all green
  - Release binary 2.04 MB
  - Image 134.7 MB → 133.8 MB (in-process)
  - Compressed image 17.4 MB → 17.5 MB
  - 3 new test cases — rock_paper_scissors, geometric_intersect,
    multi_dispatch_specificity — covering the textbook CLOS
    examples

## Reflection — "do it right or why do it"

The user's note at the moment of decision was load-bearing: *"I
am agreeing upfront we do it right or why do it ?"*

Option (c) — defer multi-dispatch — would have been faster to
ship something visible today.  Option (a) — rebuild the image —
was the structural answer that unblocks every subsequent CLOS
feature.

We went with (a), the rebuild took less time than the analysis
of which option to pick, and multi-method dispatch lands cleanly
through Factor's own well-tested machinery.  No handrolled
dispatch table, no parallel implementation to maintain, no
"sprint 3" to revisit and re-do.

The architectural through-line continues to hold: the Rust front
end stays grammar + desugar; the substrate is Factor's existing
machinery, which after one image rebuild now includes the entire
CLOS auxiliary-method line.  The next four features
(`:before`, `:after`, `:around`, `call-next-method`) all live in
that same vocab and are mostly parser + emit work on our side.

— end of multi-method-dispatch entry
