# Factor4th Object System (F4OS) — design draft

Status: **design only**, no code yet.  Sketched ahead of
implementation to settle the shape before we have token budget
for real coding.

## Why

The natural next step after polymorphic VALUE and TYPEOF.  Once
the surface language admits "a slot can hold anything Factor can
tag," the question "what is *anything*?" arises immediately.
We've answered it for the primitive types (int / float / string /
xt / addr).  But Factor *also* has tuples — record types with
named slots and a class hierarchy that supports multiple
inheritance and multi-method dispatch — and the only reason ANS
Forth users can't use them today is that we haven't given them a
surface syntax.

CLOS via Factor is the natural target because:

  - Factor's tuple/generic/method system *is* a CLOS descendant.
    Same dispatch semantics, same method-combination model
    (:before / :after / :around), same MOP shape.  Slava designed
    it that way after reading PAIP and AMOP.
  - We'd implement maybe 200 lines of desugar to expose machinery
    that's already there and JIT-optimised.
  - The integration story with what we shipped today is clean:
    classes get their own TYPEOF code (or each class gets a
    distinct code — open question below), instances live in
    VALUE slots like any other tagged value, the rest of the
    pipeline doesn't change.

## Surface syntax (proposal)

```forth
\ Define a class with two slots.  CLASS: parses like CONSTANT/VALUE:
\ it's a top-level defining word that consumes a name and body.
CLASS: point
    SLOT: x
    SLOT: y
;

\ Constructor is auto-generated as `<classname>`:
\   <point> ( x y -- p )
3.0e 4.0e <point>  \ stack: ( point )

\ Slot accessors are auto-generated using Forth-idiomatic naming:
\   point>x ( p -- x )     getter, namespaced to avoid collisions
\   point>y ( p -- y )
\   x>>point ( p x -- p )  setter, returns the object for chaining
\   y>>point ( p y -- p )

3.0e 4.0e <point>
dup point>x .              \ 3.0
dup point>y .              \ 4.0
5.0e x>>point              \ updates x slot to 5.0, returns point
point>x .                  \ 5.0

\ Single-inheritance via EXTENDS.  Slot order is parent-slots-first
\ which lines up with how Factor's TUPLE: subclassing works.
CLASS: colored-point EXTENDS point
    SLOT: rgb
;

255 0 0 3.0e 4.0e <colored-point>   \ ( rgb x y -- cp )

\ Generic function declaration with stack effect.
GENERIC: distance ( a b -- d )

\ Method definition: ANS-flavoured M: form.  Specialiser comes
\ from the slot-type-like syntax `param:class`.
METHOD: distance ( a:point b:point -- d )
    over point>x  over point>x  f-
    over point>y  over point>y  f-
    fsq  swap fsq  f+
    fsqrt
    nip nip ;

3.0e 4.0e <point>  0.0e 0.0e <point>  distance .  \ 5.0

\ Method on a subclass — dispatches more specifically than the
\ point method when both args are colored-point.  Standard
\ multi-method dispatch resolution.
METHOD: distance ( a:colored-point b:colored-point -- d )
    \ Could compare colors as well as distance — skipped for the
    \ example.  Just delegate to the point method:
    \ (Factor: call-next-method.  We need an ANS spelling.)
    SUPER:distance ;

\ Before/after/around method combinations.  Map to Factor's
\ BEFORE: / AFTER: / AROUND: word triplet.
METHOD-BEFORE: distance ( a b -- )
    ." computing distance " ;

METHOD-AFTER: distance ( a b -- )
    ." done" cr ;
```

## Translation to Factor

`CLASS: point SLOT: x SLOT: y ;` emits:

```factor
TUPLE: point { x } { y } ;
: <point> ( x y -- p ) point boa ; inline
: point>x ( p -- x ) x>> ; inline
: point>y ( p -- y ) y>> ; inline
: x>>point ( p x -- p ) >>x ; inline
: y>>point ( p y -- p ) >>y ; inline
```

`CLASS: colored-point EXTENDS point SLOT: rgb ;`:

```factor
TUPLE: colored-point < point { rgb } ;
: <colored-point> ( rgb x y -- cp ) colored-point boa ; inline
: colored-point>rgb ( cp -- rgb ) rgb>> ; inline
: rgb>>colored-point ( cp rgb -- cp ) >>rgb ; inline
\ Parent's accessors work on subclass instances without rewrap —
\ Factor's TUPLE: subclassing handles it.
```

`GENERIC: distance ( a b -- d )` emits:

```factor
GENERIC#: distance 2 ( a b -- d )
```

The `#: 2` means "multi-method dispatch on the top 2 stack items"
— that's the multiple dispatch.  For arity-1 we'd use plain
`GENERIC:`.  Arity follows from the effect annotation's input
count.

`METHOD: distance ( a:point b:point -- d ) body ;`:

```factor
M:: point point distance ( a b -- d ) body ;
```

Or for the multi-dispatch n-ary case, `HOOK:` or `GENERIC#:`'s
companion `M:` syntax — the exact Factor incantation depends on
arity.  Method body is the body the user wrote, lowered through
the rest of the pipeline (resolve / lower_exit / etc.) like any
other `:` def.

`SUPER:distance` (call-next-method) is Factor's `call-next-method`.

`METHOD-BEFORE:` / `METHOD-AFTER:` emit `BEFORE:` / `AFTER:`
forms — Factor's standard method combination.

## AST additions

```rust
pub enum Item {
    // existing variants...
    Class(ClassDef),
    Generic(GenericDef),
    Method(MethodDef),
    MethodBefore(MethodDef),
    MethodAfter(MethodDef),
}

pub struct ClassDef {
    pub name: String,
    pub name_span: Span,
    pub extends: Option<String>,  // parent class name
    pub slots: Vec<SlotDef>,
    pub span: Span,
}

pub struct SlotDef {
    pub name: String,
    pub name_span: Span,
    pub initial: Option<Vec<Expr>>,  // optional initial-value body
}

pub struct GenericDef {
    pub name: String,
    pub name_span: Span,
    pub effect: StackEffect,         // required, drives dispatch arity
    pub span: Span,
}

pub struct MethodDef {
    pub generic_name: String,
    pub specializers: Vec<MethodSpecializer>,  // one per dispatched input
    pub effect: StackEffect,
    pub body: Vec<Expr>,
    pub span: Span,
}

pub struct MethodSpecializer {
    pub param_name: String,    // for diagnostics
    pub class_name: String,    // the dispatch class
}
```

`Expr` adds nothing — class instances flow as opaque values
through `WordRef`s (the constructor `<point>`) and accessors
(`point>x`, `>>x`-suffixed setters).  These are normal word
references resolved against the synthesised dictionary entries.

## Parser additions

  - Top-level `CLASS:` arm, parallel to `VARIABLE`/`VALUE`.
    Consumes name token, optionally `EXTENDS parent-name`, then
    a sequence of `SLOT:` declarations terminated by `;`.
  - Top-level `GENERIC:` arm — consumes name and stack effect.
  - Top-level `METHOD:`, `METHOD-BEFORE:`, `METHOD-AFTER:` — each
    parses a body like `:`/`;` and a specialiser list embedded in
    the effect annotation (`a:point b:point`).
  - The specialiser-in-effect-annotation syntax is the one new
    parsing trick.  Reuse the colon-separated form Forth users
    expect: `a:point` becomes `param=a, class=point`.

## Resolve

`Item::Class` registers `<classname>`, `classname>slotname`, and
`slotname>>classname` in `user_words` so they're callable from
later code.  Method dispatch through the generic word name is
just a regular WordRef.

Specialiser class names must resolve to a registered class
(this-compile or prior).  Add a `ResolveError::NotAClass { name,
at }` for typos.

## Effect inference

  - Constructor `<classname>`: `( <one-per-slot> -- instance )`.
  - Getter `classname>slotname`: `( instance -- value )`.
  - Setter `slotname>>classname`: `( instance value -- instance )`.
  - Generic word: declared effect from `GENERIC:`.
  - Method body: must match generic's effect (or be a subset —
    same shape, possibly tighter input types).

## Integration with TYPEOF

Open question: how does TYPEOF interact with classes?

**Option A** — uniform tuple code:
```
6 = tuple   (any class instance)
```
With a separate `CLASS-OF ( obj -- class-name )` for finer
inspection.  Simpler.  CASE on TYPEOF can still detect
"some-class instance vs primitive," and user code dispatches on
generic methods anyway when it needs class-specific behaviour.

**Option B** — each class gets its own code:
Codes 100+ allocated dynamically as classes are defined.  Each
class's code is a stable identifier callers can CASE on.  More
useful but introduces global state (the code counter) and a
class-code registry that needs cross-eval persistence.

**Recommendation**: A first, B later if real code wants it.  The
generic-function dispatch already handles fine-grained class
behaviour; TYPEOF is for "is this a class instance at all?"

## Integration with VALUE

Falls out for free.  A VALUE can hold a class instance because
Factor's globals are tag-agnostic:

```forth
CLASS: point SLOT: x SLOT: y ;
3.0e 4.0e <point> VALUE origin

\ Later:
origin point>x .       \ 3.0
0.0e 0.0e <point> TO origin
```

The slot doesn't know or care it's holding a tuple vs an int.
`origin TYPEOF` returns the tuple type code.

## Implementation as a desugar pass

Same shape as `lower_qdup` / `lower_recurse` / `lower_exit`.  New
module `compiler/lower_classes.rs` runs in sema before resolve.
It:

  1. Walks `Item::Class` / `Item::Generic` / `Item::Method` items.
  2. Emits synthetic `Item::Definition` entries for constructor,
     accessors, setters (each a thin `:` def that wraps the
     Factor TUPLE: machinery).
  3. Emits synthetic Factor-side declarations for `TUPLE:` /
     `GENERIC:` / `M:` — these need their own Item variant or a
     "raw Factor injection" escape hatch.  Cleanest is probably a
     new `Item::RawFactor { source: String, span: Span }` that
     emit passes through verbatim.

The "raw Factor injection" item is interesting on its own — it'd
also be useful for any future feature that needs Factor surface
syntax we don't want to model in our AST.

## Cross-eval persistence

CompileContext gets two more fields:

```rust
pub classes: BTreeMap<String, ClassDef>,
pub generics: BTreeMap<String, GenericDef>,
```

So `METHOD: distance ( a:point b:point -- d ) … ;` defined in eval
N is visible to a method override or generic call in eval N+1.
F7 editor checker reads them from the snapshot like it reads
`values` today.

## What I'd punt on

  - **MOP**: Factor has its own MOP via `slots`, `superclass`,
    `instance-class`.  We could surface it through ANS words but
    nobody asks for it until they ask.  Defer until someone
    writes meta-circular Forth.
  - **Mixins / multiple inheritance**: Factor has `INTERSECTION:`
    and `UNION:` classes.  Most ANS users don't need these.  Single
    inheritance first.
  - **Slot type assertions**: `SLOT: x INT` would constrain `x` to
    integers and let TYPEOF predict it.  Useful, but needs
    effect-check integration.  Phase 2.
  - **Class-defined methods on existing classes**: `M:: integer
    print …` — extending Factor's builtin classes.  Doable, but
    crosses into "modifying the host" territory and is a sharp
    edge.  Defer.

## Test demos (eventual)

  - **`point` + `distance`** — the textbook example, exercises
    constructor + accessor + single-method.
  - **shape hierarchy** — `CLASS: shape ; CLASS: circle EXTENDS
    shape SLOT: radius ; CLASS: square EXTENDS shape SLOT: side ;`
    with `GENERIC: area ( s -- a )` and per-class methods.  Tests
    inheritance dispatch.
  - **account** — `CLASS: account SLOT: balance ; : deposit ( acct
    n -- acct ) over account>balance + balance>>account ;`.
    Tests mutation idiom and chained setter.
  - **before/after** — `METHOD-BEFORE: deposit … ;` for logging.
  - **multi-method dispatch** — `GENERIC: collide ( a b -- )` with
    `(asteroid, ship)`, `(ship, ship)`, `(asteroid, asteroid)`
    methods.  Tests 2-ary dispatch correctness.
  - **VALUE-of-instance** — `<point> VALUE origin` + TYPEOF
    integration.

## Effort estimate

  - AST + parser: ~150 lines
  - lower_classes desugar + Item::RawFactor escape hatch: ~200
  - resolve + effect: ~80 (class-name validation, accessor
    registration, generic-effect propagation)
  - emit: ~50 (TUPLE: / GENERIC: / M: text generation, accessor
    `:` defs)
  - sema field plumbing + CompileContext + F7 checker: ~50
  - Runtime tests: ~100 (each demo above)

Total: ~600 lines, roughly the size of VALUE/TO + TYPEOF together.
Probably a long session.  But every line slots into the existing
pipeline shape — no new infrastructure, no fight with Factor,
just more desugar.

## Open questions to resolve before coding

  1. **Specialiser syntax in effect annotation**: is `a:point` the
     right shape, or should specialisers come from a separate
     `( a b -- d ) SPECIALIZE: a point  SPECIALIZE: b point` form?
     The first is denser; the second is more parseable.  I lean
     dense.
  2. **TYPEOF — option A (uniform tuple) vs B (per-class)** —
     stated preference above, but worth pinging the user.
  3. **call-next-method spelling** — `SUPER:distance` (which
     looks ALGOL-y) vs `[ distance ] call-next-method` vs
     a Factor-style `next-method`.  CLOS calls it
     `call-next-method` so users who know CLOS will recognise that
     spelling.  Maybe just `CALL-NEXT-METHOD` as one word.
  4. **Slot initial values**: do we need them?  `SLOT: x INIT
     0.0e`.  Factor TUPLE: supports `{ x initial: 0.0 }`.  Useful
     for new-instance defaults.  Probably yes.
  5. **Constructor naming**: `<point>` is Forth-idiomatic but
     conflicts with comparison `<`.  Could use `(point)` (Smalltalk-y)
     or `point.new` (Java-y) but neither is Forthy.  Sticking with
     `<point>` and special-casing the lexer to recognise it is
     probably right.
  6. **Reserving `CLASS:`, `SLOT:`, `GENERIC:`, `METHOD:`,
     `EXTENDS`, `SUPER:`** — these become parsing-word keywords.
     Worth noting in the language doc.

## Architectural reflection

This continues the through-line from today's journal: each ANS
extension we add is a 100-300 line desugar pass plus its tests,
not a runtime word fighting Factor's optimiser or a knot in
emit.rs.  The IR Factor sees gets progressively more
idiomatic-Factor-shaped.  Classes follow this directly — they're
just Factor TUPLE:s with a Forth-friendly surface.

The deeper observation: **we're slowly building a meta-circular
Forth**.  ANS Forth itself has CREATE/DOES> as the metaprogramming
primitive, and Factor's CLOS is the modern descendant of the same
ideas.  By exposing Factor's class system through Forth syntax,
we're giving ANS users a more powerful CREATE/DOES> — one with
multiple dispatch and inheritance.  The boundary between
"language feature" and "library" gets thinner with each desugar
pass.

— end of design draft, 2026-05-27
