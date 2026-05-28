# 2026-05-27 — classes: inheritance + flattened slots

Fifth journal entry today.  Trust-but-verify pass on the sprint
1 object system by writing a *user-level* program — the
textbook shape hierarchy — and seeing what broke.  Two real
bugs surfaced, both got fixed in the same session, sprint 2's
"inheritance with proper slot flattening" item lands early.

## What the user wrote

```forth
CLASS: shape ;
CLASS: circle EXTENDS shape  SLOT: r  ;
CLASS: square EXTENDS shape  SLOT: side  ;

GENERIC: area ( s -- a )
METHOD: area ( c:circle -- a )
    circle>r dup f* 3.14159e f* ;
METHOD: area ( s:square -- a )
    square>side dup f* ;

5.0e <circle> area .   \ expect ~78.54
3.0e <square> area .   \ expect 9.0
```

Plus a `colored-point EXTENDS point` test that wanted to read
parent slots through the child.

## What broke

**Bug 1: constructor signature ignored inherited slots.**
`<colored-point>` reported `( rgb -- p )` — only one input —
when it should have been `( x y rgb -- p )`.  Factor's effect
checker rejected `3 4 255 <colored-point>` because we were
pushing 3 items into a 1-arg constructor.

Root cause: `emit_class` iterated `c.slots` (this class's own
slots) instead of the flattened list (parent's + own).

**Bug 2: parent-class accessors not auto-generated on child.**
`colored-point>x` didn't exist — the user had to fall back to
`point>x`.  Works because Factor TUPLE: subclassing preserves
slot access, but it's ugly and asymmetric.

Root cause: `class_synthesised_names` and `emit_class` both
only enumerated own slots, not the flattened list.

## The fix (~80 lines, three files)

1. Added `Sema.class_slots: HashMap<String, Vec<String>>` —
   the flattened slot list per class.
2. Computed it in `sema::build_with_prior_state` using the
   existing `lower_classes::compute_class_slots` helper that
   nobody was actually calling.
3. New `resolve::resolve_with_prior_and_values_and_classes`
   signature accepts the slot map.  Pass-1 class registration
   now looks up the flattened list and registers accessors for
   every slot.
4. `emit_class` takes `&Sema`, looks up the flattened slot
   list, uses it for both the constructor stack effect and
   the accessor generation.

The TUPLE: declaration itself still only lists *own* slots
plus the `< parent` clause — that's correct because Factor's
tuple system handles inheritance from there.

## What now works

```forth
CLASS: point  SLOT: x  SLOT: y  ;
CLASS: colored-point EXTENDS point  SLOT: rgb  ;

3 4 255 <colored-point>      \ ( x y rgb -- p ), correct arity
dup colored-point>x .        \ 3 — inherited slot via child's namespace
dup colored-point>y .        \ 4
dup colored-point>rgb .      \ 255 — own slot

\ Both namespaces work; the inherited accessors aren't shadowed:
dup point>x .                \ 3 — parent's namespace also valid
```

And the shape hierarchy:

```
captured: "78.53975 9.0 "
```

The generic dispatched on `circle` for the first call (running
`circle>r dup f* π f*`) and on `square` for the second
(`square>side dup f*`).  Multi-class single-dispatch via Factor's
inline-cached generic machinery.

## What this taught us

The substrate (Factor's tuple/generic machinery) is mature and
correct.  Both bugs were in **our surface layer** — emit
forgetting that parent slots count, resolve forgetting that
parent slot names should be registered.  The fixes were
small, isolated, and didn't touch the Factor side at all.

That matches the architectural through-line: **the bulk of
correctness lives in Factor's already-tested substrate; our
Rust code is grammar + desugar plus a few side-tables**.  Bugs
in our layer are usually surface-syntax bugs, which are quick
to find and quick to fix.

The user's framing earlier today — "I would never have written
a lot of forth if the forth I was using did not have a decent
class system" — got tested by the simplest possible class
program, and the simplest possible class program flushed two
real bugs that needed fixing.  That's the right cadence: write
something, find what hurts, fix it, journal.

## What's still pending in sprint 2

  - **Cross-eval class persistence** — biggest ergonomic miss.
    A class defined in one eval is invisible to the next.
    `CompileContext.classes` plumbing, same shape as `values`.
  - **Multi-method dispatch** — `GENERIC#: 2` for `( a b -- )`
    arity-2 dispatch.  Compare to Factor's `GENERIC#:`.
  - **Method combinations** — `METHOD-BEFORE:` / `METHOD-AFTER:`.
  - **SUPER:** — chain to parent method's behaviour.
  - **Slot initial values** — `SLOT: x INIT 0.0e`.
  - **Per-class TYPEOF codes** + `CLASS-OF`.

Today fixed: inheritance with flat slot list AND child-side
accessor generation.  That's two of sprint 2's bullets done as
a side effect of trying to write a real program.

## Stats

  - 122/122 lib unit tests
  - 72 runtime tests now (was 68 before this iteration), all green
  - Release binary 2.02 MB (was 2.01)
  - Three files touched: `sema.rs`, `resolve.rs`, `emit.rs`
  - Net diff: ~80 lines of Rust changed, including the new
    `Sema.class_slots` field and the new
    `resolve_with_prior_and_values_and_classes` signature

— end of shape-hierarchy entry
