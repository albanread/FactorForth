# 2026-05-27 — classes sprint 1: CLASS, SLOT, GENERIC, METHOD

Fourth journal entry today.  After this morning's compiler-shape
work, the rebrand to Factor4th, and the object-system design
doc — the actual object system landed.  Sprint 1 MVP: classes
with named slots, generic functions with single-class dispatch,
polymorphic slots, two setter idioms, user-facing docs.

## What shipped

Surface words available in the REPL right now:

```forth
CLASS: point          \ defines the class
    SLOT: x
    SLOT: y
;

\ Auto-generated:
\   <point>     ( x y -- p )     constructor
\   point>x     ( p -- v )       getter
\   x>>point    ( p v -- p )     chainable setter (returns object)
\   point.x!    ( v p -- )       ANS-style store (drops object)
\   (same for y)
\   point — class-name reserved for METHOD: dispatch

GENERIC: describe ( p -- )
METHOD: describe ( p:point -- )
    dup point>x . point>y . ;

3 4 <point> describe   \ prints "3 4 "
```

Six runtime tests all pass.  Full regression sweep clean: 68
runtime tests + 122 lib tests, no regressions.

## Pipeline integration

New stage in the desugar chain, exactly the shape we've been
building:

```
lex → parse → expand_templates → lower_qdup → lower_recurse
           → resolve → lower_exit → effect-infer → escape → emit
```

The new bits:

  - **AST**: `Item::Class`, `Item::Generic`, `Item::Method`,
    `Item::RawFactor` (escape hatch for emitting Factor source
    we don't want to AST-model).  Plus `ClassDef`, `SlotDef`,
    `GenericDef`, `MethodDef`, `MethodSpecializer`.
  - **Parser** (~150 lines added): top-level `CLASS:` / `GENERIC:`
    / `METHOD:` arms.  Specialisers parsed from `name:class`
    syntax in the effect annotation — dense and readable.
  - **`compiler::lower_classes`**: a slot-flattening helper
    (`compute_class_slots`) plus a name-enumeration helper
    (`class_synthesised_names`) that returns the list of
    auto-generated word names resolve should register.
  - **Resolve**: pass 1 registers class name + constructor +
    accessors + generic names; pass 2 walks method bodies like
    `:` def bodies.
  - **Effect**: classes register `(0,0)`; generics carry their
    declared effect; methods inherit (no separate entry).
  - **Emit**: `emit_class` → `TUPLE: name [< parent] { slot } …
    ;` plus inline accessor `:` defs.  `emit_generic` → `GENERIC:
    name ( … )`.  `emit_method` → `M: class generic body ;` with
    lower_exit fallback for EXIT-in-body.  `vocabs_needed` adds
    `classes.tuple / generic / generic.standard / accessors`
    when any class item is present.

Notable: every other match in the codebase needed an arm for
the new Item variants — dump, lower_qdup, sema's call-graph and
escape passes.  Mostly one-line additions because they have no
Forth-side body to walk (Method is the exception — it gets
walked like a `:` body).

## The two-setter design

The first cut had only the chainable `slot>>class ( p v -- p )`
form.  Then the user pointed out: "slots are polymorphic, how
do we set them" — and after the explanation, "we actually
should ship both, because these are not quite the same
operations."

Both setters touch the same slot but they're *different idioms*:

| Setter             | Effect           | Use when                                          |
|--------------------|------------------|---------------------------------------------------|
| `slot>>class`      | `( p v -- p )`   | Building / transforming an object, want to chain  |
| `class.slot!`      | `( v p -- )`     | Mutate one field and move on (ANS muscle memory)  |

Both compile to the same Factor `>>x` accessor primitive — same
machine code, same JIT inlining.  The difference is purely stack
discipline, which matters for code readability and shape:

```forth
\ Chainable — builds a point with three writes:
0 0 <point>  3 x>>point  4 y>>point

\ ANS — drops the object so we don't carry it forward:
99 mypoint point.x!
```

This was a small detail with disproportionate impact on the
language's *feel*.  Without both, ANS users would be writing
`mypoint 99 x>>point drop` (chainable + drop) which is
ungainly, while Factor users would chafe at a value-first store
shape.

Names: `class.slot!` reads "store-into class's slot" left to
right.  Symmetric with the `class>slot` getter (period-separated
might be even more symmetric long-term but `>` is established).

## Polymorphism

Falls out for free.  Factor tuple slots are tag-erased; the
slot doesn't know what type it holds.  So:

```forth
CLASS: holder  SLOT: x  ;
0 <holder>                                  \ x = int 0
dup 42 swap holder.x!                       \ x = int 42
dup 3.14e swap holder.x!                    \ x = float
dup s$" hello" swap holder.x!               \ x = managed-string
3 4 <point> over swap holder.x!             \ x = a point instance
```

One slot, five different runtime types, no recompile.  Same
story as polymorphic VALUE and TYPEOF from earlier today —
because it's the same Factor primitive underneath.

The composability is the win.  Polymorphic VALUE + polymorphic
slots + TYPEOF means a heterogeneous data structure (Lisp-style
property list, a stack of any-type values, a tree of mixed
records) is just regular Forth code.

## Namespacing

```forth
CLASS: point    SLOT: x  SLOT: y  ;
CLASS: vector3  SLOT: x  SLOT: y  SLOT: z  ;
```

Both have `x` and `y` slots, but the auto-generated accessors
are class-qualified:

  - `point>x` vs `vector3>x`
  - `x>>point` vs `x>>vector3`
  - `point.x!` vs `vector3.x!`

So `99 v3 point.x!` doesn't resolve — `point.x!` exists only for
the `point` class.  Catches a real category of bugs at compile
time.

## The doc

User asked specifically to start documenting under the
release/factor4th tree.  Wrote
`release/factorforth/docs/classes.md` (still in the
factorforth/ directory pending the sprint-2 path rename).
Covers the surface syntax, the two-setter design, polymorphism
through slots, GENERIC: + METHOD:, the worked bank-account
example, namespacing, and what's deferred to sprint 2.  Linked
from `docs/index.md` between stack-effects and let-algebra.

The doc explicitly explains *why* both setters ship — they're
not redundant, they're different operations in code-flow even
though they're the same operation in memory.  Worth saying out
loud because every reader will ask the question.

## What sprint 2 unlocks (task #64)

  - Cross-eval class persistence — `CompileContext.classes`
    threaded through `build_with_prior_state` and the F7
    snapshot, so a class defined in one eval is visible from the
    next eval AND the editor checker
  - Parent-class accessors auto-generated on children, with
    proper slot-flattening for the constructor effect
  - Multi-method dispatch via `GENERIC#: 2` for `( a b -- … )`
    shapes — the multimethods half of CLOS
  - Method combinations: `METHOD-BEFORE:` / `METHOD-AFTER:` /
    `:around` — Factor has these as `BEFORE:` / `AFTER:` /
    `AROUND:` already
  - `SUPER:` (or `CALL-NEXT-METHOD`) for chaining to the parent
    method's behaviour
  - Slot initial values: `SLOT: x INIT 0.0e`
  - Per-class TYPEOF codes + `CLASS-OF` (dynamic allocation
    of integer codes as classes are defined; useful when CASE
    on TYPEOF wants finer granularity than "tuple")
  - Effect inference on method bodies vs the generic's declared
    effect

Each one is ~50-100 lines slotting into the existing pipeline
shape.  Same desugar discipline.  No new infrastructure.

## Architectural reflection (still going)

Today's third instance of the same observation: each ANS
extension is a ~150-300 line desugar pass plus its tests, *not*
a runtime word fighting Factor's optimiser or a knot in
`emit.rs`.

What's interesting about the object system specifically is how
*little* of it lives in Rust.  The Rust side does:

  1. Parse the surface syntax (`CLASS:` / `SLOT:` / `GENERIC:` /
     `METHOD:`)
  2. Register the auto-generated names so resolve sees them
  3. Emit the right Factor source (`TUPLE:` / `GENERIC:` / `M:`)
     plus the inline accessor `:` defs

Everything else — dispatch, inline caching, slot layout, GC,
JIT optimisation, the entire actual *object system* — is
Factor's existing tuple/generic machinery.  We're providing a
new front-end syntax for capabilities that were already there.

The line between *language feature* and *library* gets thinner
again.  From an ANS Forth user's perspective they're getting a
new language construct.  From an implementor's perspective
we're writing ~600 lines of surface syntax onto a CLOS
descendant that was already JIT'd and battle-tested.  The Rust
code we're shipping is increasingly *grammar plus desugar*
rather than *runtime*.

The meta-circular Forth observation from earlier holds: we're
slowly building one, with `:`/`;` defs as the first-class
construct, then `CREATE`/`DOES>` as the metaprogramming hook,
then template-based generators like `array` / `farray` /
`cbuffer`, then VALUE/TO for polymorphic globals, now `CLASS:`
/ `GENERIC:` / `METHOD:` as the modern OO surface.  Each layer
is a small surface-syntax addition exposing a chunk of Factor's
existing capability.

## Stats

  - 60 → 64 tasks in the system, 53 → 56 completed
  - 3 new tasks created: rebrand sprint 2 (#62), object system
    sprint 1 (#63, completed), object system sprint 2 (#64)
  - Lib unit tests: 122 → 122 (no new lib tests this sprint,
    just runtime tests)
  - Runtime tests: 65 → 68 (3 new class tests + 3 setter/poly
    tests)
  - Release binary: 1.95 MB → 2.01 MB
  - One new user-facing doc page: classes.md (~250 lines)

## What's next, probably

Sprint 2 of the object system (task #64) is the obvious
follow-up, especially the cross-eval persistence which would
let users define classes interactively without losing them on
the next eval.  Multi-method dispatch is the other big-impact
item — it unlocks `GENERIC: collide ( a b -- )` style code that
dispatches on both arguments, which is where CLOS really starts
to feel different from single-dispatch OO.

The rebrand sprint 2 (#62) is also still pending — binaries
still say factorforth-ui, title bars still read FactorForth.
Half-day of find-replace plus rebuild.

— end of classes-sprint-1 entry
