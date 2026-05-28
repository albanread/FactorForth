# 2026-05-27 — object-system design (no code yet)

Companion entry to today's "compiler, not translator" journal.
Started designing a CLOS-flavoured object system for FactorForth.
Sketched at the end of a long session, ahead of any
implementation, while the day's architectural decisions are still
fresh.  Filed at `docs/design/object-system.md`.

## Why now

The polymorphic VALUE + TYPEOF work this morning answered "a slot
can hold anything Factor can tag" for the primitive types.  The
immediate follow-up question is what *anything* means when one of
the things is a tuple — Factor's record type with named slots,
class hierarchy, multi-method dispatch.

Factor's tuple system is already a CLOS descendant.  Same
dispatch semantics, same method-combination model
(:before/:after/:around), same shape MOP if we ever expose one.
The work to give ANS Forth users access to it is approximately
just a desugar pass plus syntactic surface — Factor does all the
actual implementation work for us.

This makes "ANS Forth with classes" a comparable scope to
VALUE/TO + TYPEOF combined.  About 600 lines, slotting into the
existing pipeline shape.

## Surface-language decisions

Picked Forth-idiomatic shapes throughout:

  - `CLASS: name SLOT: x SLOT: y ;` — top-level defining word,
    parses like CONSTANT / VALUE
  - `EXTENDS parent` — single inheritance keyword
  - `<classname>` — auto-generated constructor (the Smalltalk /
    Lisp `make-instance` shape, spelled Forth-style)
  - `classname>slotname` — getter (parallel to `point>x` in
    the WF64 graphics demos)
  - `slotname>>classname` — setter, returns the object for
    chaining (Factor's `>>x` convention with ANS namespacing)
  - `GENERIC: name ( a b -- c )` — generic function with required
    effect annotation (drives dispatch arity, matches what we
    enforce for RECURSE)
  - `METHOD: name ( a:point b:point -- d ) body ;` — method
    definition with specialisers embedded in the effect
    annotation
  - `METHOD-BEFORE:` / `METHOD-AFTER:` — method combinations
  - `SUPER:name` — call-next-method (CLOS's name was
    `call-next-method`; this is the densest Forthy spelling)

The one new parsing trick: specialisers embedded in the effect
annotation as `param:class`.  Denser than a separate
`SPECIALIZE:` declaration form and reads naturally — `( a:point
b:point -- d )` says exactly what you want at a glance.

## Translation strategy

Each surface form maps to a Factor primitive:

```
CLASS: point SLOT: x SLOT: y ;
  ↓
TUPLE: point { x } { y } ;
: <point> ( x y -- p ) point boa ; inline
: point>x ( p -- x ) x>> ; inline
: x>>point ( p x -- p ) >>x ; inline
\ (plus point>y / y>>point)

GENERIC: distance ( a b -- d )
  ↓
GENERIC#: distance 2 ( a b -- d )

METHOD: distance ( a:point b:point -- d ) body ;
  ↓
M:: point point distance ( a b -- d ) body ;
```

The `GENERIC#:` is Factor's n-ary dispatch form — the `#: 2` means
"dispatches on top 2 stack items," which is the multiple-dispatch
hook.  Arity falls out of the declared effect's input count.

## New infrastructure: Item::RawFactor

Most of the desugar can emit synthetic `Item::Definition` entries
(constructor, accessors, setters are all just thin `:` defs).  But
the TUPLE: / GENERIC: / M: declarations themselves are Factor
syntax we don't want to AST-model — there's no Forth equivalent
to lower TUPLE: from.

The clean answer is a new `Item::RawFactor { source: String, span:
Span }` that emit passes through verbatim.  Useful here, useful
anywhere a future feature wants Factor surface syntax outside our
modelling.  Tracked in the doc as the only new escape hatch.

## Integration with what shipped today

The composition story is what makes this worth doing:

  - **VALUE + class instance**: `3.0e 4.0e <point> VALUE origin`
    holds a tuple in a polymorphic slot.  Falls out for free —
    Factor's `set-global` is tag-agnostic.
  - **TO + class instance**: `another-point TO origin` rebinds.
    Same.
  - **TYPEOF + class instance**: open question whether to
    return a uniform `tuple-type = 6` (recommended, simpler) or
    allocate per-class codes dynamically (deferred until a real
    program asks).  The CASE-on-TYPEOF idiom still works for
    "instance vs primitive"; fine-grained class behaviour belongs
    in generic-function dispatch where it's naturally typed.
  - **F7 editor checker**: classes and generics go into
    CompileContext alongside `values`, `templates`, `user_words`.
    Editor sees `<point>` and `point>x` and `distance` as known
    names across evals.

## Open questions filed in the doc

Six items to ping the user on before implementation:

  1. Specialiser syntax — `a:point` (dense) vs `SPECIALIZE: a
     point` (more parseable).  Leaning dense.
  2. TYPEOF — option A uniform `tuple-type` vs option B per-class
     codes.  Recommend A; revisit if real code asks for B.
  3. `call-next-method` spelling — `SUPER:distance` /
     `CALL-NEXT-METHOD` / `next-method`.  CLOS users will
     recognise the original.
  4. Slot initial values — `SLOT: x INIT 0.0e`.  Factor TUPLE:
     supports defaults; probably yes.
  5. Constructor naming — `<point>` collides with comparison `<`.
     Lexer special-case is probably right; alternatives
     (`(point)`, `point.new`) lose Forth idiom.
  6. New reserved parsing words — `CLASS:`, `SLOT:`, `GENERIC:`,
     `METHOD:`, `METHOD-BEFORE:`, `METHOD-AFTER:`, `EXTENDS`,
     `SUPER:`.  Worth a manual section.

## Architectural reflection (continued from earlier)

The through-line from this morning's journal extends naturally
here.  **Each ANS extension is now a ~100-300 line desugar pass
plus its tests, not a runtime word fighting Factor's optimiser or
a knot in emit.rs.**

The deeper thing this object-system design surfaces: by exposing
Factor's CLOS through Forth syntax, we're giving ANS users a
*more powerful CREATE/DOES>*.  ANS already has CREATE/DOES> as
the metaprogramming primitive — define a defining word, define a
shape, instantiate.  Multi-method dispatch with method
combinations is the modern descendant of the same idea, with
hierarchy and dynamic specialisation added.  The boundary between
"language feature" and "library" gets thinner with each desugar
pass.

We are, in a real sense, **slowly building a meta-circular
Forth**.  The classical ANS Forth defining-word zoo (CREATE,
DOES>, BUILD, VARIABLE, CONSTANT, VALUE, USER) was always
metaprogramming — words that define words.  CLOS-flavoured
classes are just the same idea with a richer dispatch model and
a parser surface that knows about inheritance.  Factor having
done the hard work means we get to expose the result without
implementing it.

## Status

  - Design draft: complete (`docs/design/object-system.md`)
  - Open questions: six listed, awaiting answers
  - Implementation: not started
  - Effort estimate: ~600 lines (AST + parser + lower_classes +
    Item::RawFactor + resolve + effect + emit + sema plumbing +
    F7 checker integration + runtime test demos)

Token budget exhausted; picks up next session with whichever open
questions get answered first.

— end of design entry
