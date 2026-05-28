# Factor4th CoreProtocols — design

Status: **design / not yet built.**  This document proposes
**CoreProtocols**, a standard framework layered on Factor4th's
CLOS-flavoured object system (see `object-system.md`).  It is driven
by concrete toy programs rather than abstract taxonomy — the scenarios
pull each layer into existence, and we build bottom-up, shipping a
runnable toy at the end of each phase.

The name is the thesis: the unit of design is the **protocol** (a
named set of generic functions), not a class hierarchy.  We
deliberately did **not** call it a "class library" — that framing
belongs to message-passing systems where behaviour hangs off a
receiver.  Here the classes are incidental; the protocols are the
product.

---

## 1. The governing idea: this is CLOS, not Smalltalk

PowerMOPS and Win32Forth were *message-passing* object systems —
`obj message: arg`, deep inheritance trees, behaviour attached to a
receiver.  Factor4th is **CLOS**: you never send a message to an
object, you call a **generic function** that dispatches on the
classes of its arguments.  Every design choice below follows from
that one fact:

1. **Protocols, not hierarchies, are the unit of design.**  A
   protocol is a named set of generic functions a class can
   implement (`size`/`at`/`each` is the *sequence protocol*; `draw`
   is the *drawable protocol*).  We organise the library around
   protocols; classes are just things that satisfy them.

2. **Protocols are open.**  Because dispatch is multi-method, a
   *user's* class can satisfy a *library* protocol after the fact,
   with no edit to the library — `METHOD: show ( x:my-thing -- ) ...`
   and your type now prints like everything else.  Message-passing
   Forths can't do this without surgery.

3. **Composition over inheritance.**  Single inheritance only,
   shallow trees, prefer a slot that *holds* an object over a
   subclass that *is* one.  (Matches the project's standing taste:
   multiple inheritance is a deliberate non-goal.)

4. **Double dispatch where the problem is genuinely 2-D.**  Event
   handling (`app × event`) and equality/arithmetic (`a × b`) are
   naturally two-argument; multimethods make them clean instead of
   the `instanceof` ladders other languages reach for.

5. **Steal the substrate, hide it completely.**  Each layer wraps a
   battle-tested Factor vocab (sequences, assocs, math, io) behind a
   Forth-native protocol.  The user writes Forth and reads Forth;
   Factor never surfaces (the `SEE` rule — "we do not write Factor").

6. **Method combinations for cross-cutting concerns.**  `:before` /
   `:after` carry logging, buffer-flips, and validation without
   threading them through every primary method.

### A blunt note on `call-next-method` (it isn't here)

In a full CLOS, an effective method runs all `:before`s, then the
**most specific primary**, then all `:after`s — and a primary can call
`call-next-method` (CNM) to invoke the *next* most specific primary,
i.e. tweak the parent's work.  We deliberately don't have CNM.

The consequence must be stated plainly so it never surprises anyone:
**overriding a primary method completely REPLACES the parent's
primary.**  There is no automatic delegation up the chain.  If a
subclass's primary needs the parent's behaviour, it must call an
explicit parent helper word — composition, not inheritance:

```forth
\ Instead of relying on (absent) call-next-method:
: draw-shape-base ( s -- ) ... shared drawing ... ;     \ explicit helper
METHOD: draw ( s:circle -- )  dup draw-shape-base  ... circle extras ... ;
```

**But there is one place we get CLOS-style layering for free** — and
it's the important one: `:after` methods chain automatically,
least-specific-first.  That is *exactly* the `initialize-instance
:after` order CLOS uses for construction (see Layer 0, §3 below).  So
the lifecycle case — the most common reason people reach for CNM —
needs no CNM at all: each class adds an `:after initialize` and they
run parent-before-child by construction.  CNM's absence bites only on
*primary* overrides, where composition is the answer.

The shallow-tree, prefer-composition discipline means primary
overrides that want the parent are rare in the first place.

---

## 2. The scenarios that drive the design

Theory is cheap; these four toys are the acceptance tests for the
whole library.  Each one is small enough to finish and broad enough
to exercise a real cross-section.

### 2a. Othello (Reversi)
A board game.  Pulls in: a **2-D grid** collection (8×8 cells), a
**value enum** for cell state (empty/black/white), **game logic** as
generic functions, a **console renderer** (`show` protocol) first,
then a **graphical** one (pane + mouse events).  This is the
collections-and-events workout, and a perfect first milestone in two
stages: text Othello (no GUI) proves Layer 0+1; graphical Othello
proves Layer 5.

### 2b. Markdown → HTML
Text processing.  Pulls in: **streams** (read from a file *or* a
string, write to either — the parser shouldn't care), a **document
model** that is a small **class hierarchy of nodes** (heading,
paragraph, list, code-block, emphasis, link), and a **render
protocol** — `GENERIC: render ( node stream -- )` with one method per
node class.  The classic "visitor pattern" *is* generic dispatch
here; there's no visitor boilerplate, you just add methods.  This is
the streams-and-hierarchy workout.

### 2c. Bouncing balls (physics toy)
Animation.  Pulls in: **`vec2`** numerics (position/velocity),
**tick events** for the simulation step, **canvas** drawing, and an
**animation loop**.  The numerics-and-realtime workout, and the
bridge to...

### 2d. Mandelbrot viewer (callback to our roots)
Pulls in: **`complex`** numerics (the iteration is literally
`z = z² + c`), canvas, and **mouse events** to recentre/zoom.  Ties
the new framework back to the very first demo this project shipped,
now expressed in objects instead of bare words.

Collectively these touch every layer: core protocol, collections,
numerics, text, streams, files, canvas, events.

---

## 3. The layers

```
Layer 0  Core protocol      initialize · show · equals? · hash · clone
Layer 1  Collections        seq protocol · vector grid dict set queue stack
Layer 2  Numerics           vec2 vec3 · complex · (matrix/rational deferred)
Layer 3  Text & streams     string · string-builder · STREAM protocol · string-stream
Layer 4  Files              path · file · file-stream  (needs runtime FFI)
Layer 5  GUI & events       color rect · canvas/pane · event hierarchy · app loop
```

Dependencies flow downward only.  Each layer is mostly pure Forth
class definitions over a stolen Factor vocab; the spots needing new
runtime/Rust work are called out in §5.

### Layer 0 — Core protocol

The root behaviours every class may opt into.  Not a forced base
class — "being an object" means "implements these generics."

```forth
GENERIC: initialize ( obj -- obj )   \ lifecycle hook, run after allocation
GENERIC: show       ( obj -- )       \ human-readable, what DUMP/. lean on
GENERIC: equals?    ( a b -- ? )     \ value equality (double-dispatched)
GENERIC: hash       ( obj -- n )     \ for dict/set keys
GENERIC: clone      ( obj -- obj' )  \ shallow copy

\ Sensible catch-alls so an un-extended class still "works":
METHOD: initialize ( o:object -- o )           ;          \ no-op
METHOD: show       ( x:object -- )    ." <" ... class-name ... ">" ;
METHOD: equals?    ( a:object b:object -- ? )  2dup eq? ;  \ identity
```

`DUMP` (already shipped) and `show` cooperate: `DUMP` is the raw
type+bytes view for debugging; `show` is the pretty, class-defined
view for users.

#### The initialization lifecycle (feedback #3)

CLOS doesn't construct with a bespoke per-class word; it allocates,
then calls the generic `initialize-instance`, so users hook in
generically.  CoreProtocols adopts the same shape, and our method
combinations make it *fall out for free*:

- A constructor allocates the instance (slots given or defaulted) and
  then calls `initialize` before returning it.  **The object system's
  auto-generated `<name>` constructor will be changed to append
  `initialize`** so the hook fires for every class, hand-written
  constructor or not (see §6, object-system prerequisites).
- A class sets up its *derived/internal* state with
  `METHOD-AFTER: initialize` — e.g. `grid` allocates its backing
  `cells` vector from its `w`/`h` slots:

```forth
CLASS: grid SLOT: w SLOT: h SLOT: cells ;
METHOD-AFTER: initialize ( g:grid -- )
    dup w>>grid over h>>grid *  <vector>  swap cells!!grid ;
```

Because `:after` methods chain **least-specific-first**, a
`colored-grid EXTENDS grid` runs `grid`'s `:after initialize`
(allocate cells) *then* `colored-grid`'s (allocate colours) — exactly
CLOS's `initialize-instance :after` order, with no
`call-next-method` and no manual chaining.  This is the single
strongest argument that the before/after machinery we built was worth
it: the construction lifecycle is just method combination.

`initialize` returns the object (`( obj -- obj )`) so constructors
read as a pipeline and `:before initialize` can validate inputs
before any setup runs.

### Layer 1 — Collections

One protocol, several backings.  The protocol:

```forth
GENERIC: size ( c -- n )
GENERIC: at   ( i c -- x )         \ index below, COLLECTION ON TOP
GENERIC: at!  ( x i c -- )         \ value, index, COLLECTION ON TOP
GENERIC: each ( c quot -- )        \ quot: ( x -- )  — QUOTATION ON TOP
GENERIC: map  ( c quot -- c' )     \ quot: ( x -- y )
GENERIC: push ( x c -- )
GENERIC: pop  ( c -- x )
```

**Argument-order rule (feedback #2).**  The draft had
`at! ( x c i -- )` with the collection buried in the middle — the
worst spot, forcing a `rot` every call.  Two conventions resolve it,
and we adopt both:

- **Accessors / mutators put the collection on top.**  `at ( i c )`,
  `at! ( x i c )`, `at-xy ( x y g )`.  This matches Factor's own
  `nth ( i seq )` / `set-nth ( elt i seq )` family, so when we steal
  those vocabs the wrapper is a near-passthrough — no reordering —
  and a collection held as a loop invariant stays reachable with
  `dup` instead of `rot`/`-rot`.
- **Combinators put the quotation on top.**  `each ( c quot )`,
  `map ( c quot )` — so code reads `coll [ ... ] each`, matching
  Factor's combinator convention.

The cost is that 2-D reads no longer spell left-to-right like
`grid[x][y]` (`x y grid at-xy` rather than `grid x y at-xy`); we take
the ergonomic win over the readability one, per the feedback, and
document it loudly (open question #2 is now settled this way).

Concrete classes, each stealing a Factor vocab:

| class    | backing (stolen)        | used by         |
|----------|-------------------------|-----------------|
| `vector` | Factor `vector`         | markdown blocks |
| `grid`   | vector + (w,h) slots    | Othello board   |
| `dict`   | Factor `hashtable`      | link refs, counts |
| `set`    | Factor `hash-set`       | legal-move sets |
| `queue`  | Factor `dlist`          | BFS / flood     |
| `stack`  | Factor `vector`         | parser state    |

`grid` is worth its own class because 2-D indexing and bounds-checking
recur in board games and image work.  Accessors follow the
collection-on-top rule (`x y g at-xy`):

```forth
CLASS: grid SLOT: w SLOT: h SLOT: cells ;
: <grid> ( w h -- g )  grid boa initialize ;   \ initialize allocates cells
GENERIC: at-xy      ( x y g -- v )
GENERIC: at-xy!     ( v x y g -- )
GENERIC: in-bounds? ( x y g -- ? )
```

Othello board render via the `show` protocol (text milestone):
```forth
METHOD: show ( b:board -- )
    dup h>>grid [ ... each row ...
        ... w>>grid [ ... at-xy cell>char emit ] ...
        cr ] each ;
```

### Layer 2 — Numerics

Value classes with an arithmetic protocol.  `vec2`/`complex` first
(the graphics toys need them); `vec3`, `matrix`, `rational` deferred
under the standing "land it when a demo forces it" rule.

```forth
CLASS: vec2 SLOT: x SLOT: y ;
GENERIC: v+ ( a b -- c )    GENERIC: v- ( a b -- c )
GENERIC: v* ( a k -- c )    GENERIC: dot ( a b -- n )
GENERIC: norm ( a -- n )

CLASS: complex SLOT: re SLOT: im ;
GENERIC: c+ ( a b -- c )    GENERIC: c* ( a b -- c )
GENERIC: c-mag2 ( a -- n )  \ |z|² without the sqrt, for the escape test
```

These compose *with* graphics, not just beside it: a `vec2` is a
point you hand straight to the canvas; a `complex` is a Mandelbrot
sample.

### Layer 3 — Text & streams

`string` is a thin face over our managed `$`-strings; `string-builder`
accumulates.  The real prize is the **stream protocol** — the
abstraction that lets the markdown parser ignore *where* bytes come
from or go.

#### EOF is an object, not a flag (feedback #1)

The first draft returned dual values — `read-char ( s -- ch ? )` with
a trailing boolean.  That's stack noise, it forces ANS `IF`/`WHILE`
control flow, and worst of all it leans on Factor's primitive `f`
sentinel — leaking the substrate's "`f` is false *and* null *and*
end-of-everything" feel, exactly the thing we keep off the table.

Instead, end-of-stream is a **singleton object** and `read-char`
returns *exactly one* thing — a character, or the EOF marker:

```forth
CLASS: eof-marker ;
\ one shared instance, handed out by <eof>:
eof-marker boa  VALUE the-eof
: <eof> ( -- obj ) the-eof ;
: eof?  ( obj -- ? ) <eof> eq? ;      \ for the rare flag-style check

GENERIC: read-char  ( s -- obj )      \ a char-code, or <eof>
GENERIC: read-line  ( s -- obj )      \ a string,    or <eof>
GENERIC: write      ( str s -- )
GENERIC: write-line ( str s -- )
GENERIC: close      ( s -- )

CLASS: string-stream SLOT: buf SLOT: pos ;   \ in-memory, pure Forth
\ file-stream lives in Layer 4 (needs FFI)

\ RAII: open, run a quotation, always close (even on error).
: with-stream ( stream quot -- )  ... ;
```

**Why this is the CLOS move:** you replace the conditional with
polymorphism.  Reading a whole stream becomes a generic function that
dispatches on what came back — a character keeps going, the EOF
marker stops — and the loop *is* the method table:

```forth
GENERIC: feed ( obj s -- )                      \ consume one item, recurse
METHOD:  feed ( ch:integer s:stream -- )        \ a char: handle, read next
    ... use ch ...  dup read-char swap feed ;
METHOD:  feed ( e:eof-marker s:stream -- )       \ EOF: stop, naturally
    2drop ;

: drain ( s -- )  dup read-char swap feed ;       \ kick it off
```

That buries Factor's `[ dup ] [ ... ] while` idiom entirely — no
flags, no `f`, just dispatch.  (Note our chars are integer code
points, so the "is it a char" method specialises on `integer`, not a
`character` class; we don't wrap chars in objects — ANS already treats
them as cells, and a class per character would be all cost, no
benefit.)

Markdown reads from *a stream* and writes to *a stream*; in tests it's
`string-stream` both ends (fast, no files), in the CLI it's a
`file-stream` in and out.  Same parser code.  That substitutability is
the whole point of the protocol.

The principle generalises: **graceful, expected state transitions
(EOF, "key not found", "empty") return sentinel objects you can
dispatch on; genuine faults throw** (see §8, open question #3 —
error style).

### Layer 4 — Files

```forth
CLASS: path SLOT: text ;
GENERIC: join ( p name -- p' )   GENERIC: ext ( p -- str )   GENERIC: basename ( p -- str )

CLASS: file SLOT: path ;
GENERIC: exists? ( f -- ? )      GENERIC: size ( f -- n )
GENERIC: open-read  ( f -- stream )   \ returns a file-stream
GENERIC: open-write ( f -- stream )
```

`file-stream` satisfies the Layer 3 stream protocol over real OS
handles.  **This is the layer that needs Rust** (see §5).

### Layer 5 — GUI & events

Drawing protocol over the existing `gpane-*` primitives, plus small
value classes:

```forth
CLASS: color SLOT: r SLOT: g SLOT: b SLOT: a ;
CLASS: rect  SLOT: x SLOT: y SLOT: w SLOT: h ;
\ point = vec2 (reuse Layer 2)

CLASS: canvas SLOT: handle ;
GENERIC: clear      ( cv color -- )
GENERIC: fill-rect  ( cv rect color -- )
GENERIC: line       ( cv p0 p1 color -- )
GENERIC: circle     ( cv center r color -- )
GENERIC: text-at    ( cv p str color -- )   \ needs text-draw FFI
GENERIC: present    ( cv -- )                \ flip the buffer
```

The **event hierarchy** + the **double-dispatch loop** — the
centrepiece, and the clearest payoff of multimethods:

```forth
CLASS: event ;
CLASS: key-event    EXTENDS event SLOT: ch SLOT: code ;
CLASS: mouse-event  EXTENDS event SLOT: x SLOT: y SLOT: button ;
CLASS: resize-event EXTENDS event SLOT: w SLOT: h ;
CLASS: tick-event   EXTENDS event SLOT: ms ;
CLASS: close-event  EXTENDS event ;

CLASS: app SLOT: canvas SLOT: running ;
GENERIC: handle ( app ev -- )
METHOD:  handle ( a:app e:event -- ) 2drop ;   \ inherited no-op

: run ( app -- )
    begin
        dup next-event       \ wraps gpane-next-event into an event object
        handle               \ dispatches on (app-subclass × event-subclass)
        dup running>>app
    while repeat drop ;
```

Graphical Othello then reads as pure intent:
```forth
CLASS: othello EXTENDS app SLOT: board SLOT: turn ;
METHOD: handle ( a:othello e:mouse-event -- )  ... place a piece ... ;
METHOD: handle ( a:othello e:key-event   -- )  ... 'r' resets ... ;
METHOD: handle ( a:othello e:tick-event  -- )  ... animate flips ... ;
METHOD-AFTER: handle ( a:othello e:event -- )  a redraw + present ;
```

`:after handle` repaints after *every* event without each handler
remembering to — exactly the cross-cutting use `:after` was added
for.

---

## 4. Build order (in layers, each ending in a runnable toy)

The user's instinct is right: build bottom-up, but validate each
phase by pulling a real scenario through it.

- **Phase 1 — Core + Collections.**  Layer 0 (`show`/`equals?`) +
  Layer 1 (`vector`, `grid`, `dict`).  Milestone: **text Othello** —
  full game logic and a console board, no GUI.  Proves the protocol
  style end-to-end with zero new runtime work.

- **Phase 2 — Text & streams.**  Layer 3 (`string-builder`, stream
  protocol, `string-stream`).  Milestone: **markdown → HTML, string
  to string** — parse a `string-stream`, emit to a `string-stream`,
  assert on the result.  Still no FFI.

- **Phase 3 — Files.**  Layer 4 + the Rust file FFI (§5).  Milestone:
  **markdown CLI** — read `foo.md`, write `foo.html`.  Same parser as
  Phase 2; only the stream construction changes.  First runtime work.

- **Phase 4 — Numerics + GUI/events.**  Layer 2 (`vec2`, `complex`) +
  Layer 5 (canvas, events, app loop).  Milestones: **bouncing balls**,
  **graphical Othello**, **Mandelbrot viewer**.  The big visible
  payoff, and where double dispatch earns its keep.

Layer 0's root protocol gets *refined* during Phases 2–4 as concrete
classes reveal what they actually need from it — design the root from
the leaves, not ahead of them.

---

## 5. Runtime / Rust integration plan

The user has green-lit deeper Rust/runtime work to make this an
integrated whole rather than a Forth veneer.  Honest inventory of
what each layer needs from below:

- **Phases 1–2: nothing new.**  Collections steal Factor vocabs;
  `string-stream` is pure Forth over managed strings.  This is
  deliberate — we get two shipped toys before touching the runtime.

- **Phase 3 — file I/O FFI.**  Add `rt_file_open / read / write /
  close / seek / size / exists` exports in the Rust runtime, surfaced
  through a `forth.io` vocab (sibling to `forth.runtime`).  The Forth
  `file-stream` class calls these.  Modest, well-scoped FFI.

- **Phase 4 — GUI enrichment.**  Mostly already present (`gpane-*`,
  `ev-*`, `EV_TICK`).  Likely additions:
  - **Text drawing** (`text-at`) — a `rt_gpane_text` FFI (DrawText /
    DirectWrite), since the current pane API draws shapes but not
    glyphs.
  - **Richer event payloads** — confirm `gpane-next-event` returns
    mouse x/y/button and key code/char, not just a kind; enrich the
    Rust side if not.
  - **Timer/tick requests** — a way to ask for periodic `tick-event`s
    at N ms for animation (the balls/Mandelbrot loops).  May be a
    `rt_gpane_set_timer` export.
  - **Double-buffered present** — `gpane-present` exists; confirm it
    flips cleanly for flicker-free animation.

Design rule for all of the above (from the `SEE` discussion): the FFI
and Factor vocabs are plumbing; the *only* thing the user ever sees or
writes is the Forth class + generic surface.  No Factor, no raw
handles, no FFI spelling leaks upward.

---

## 6. Object-system prerequisites this design surfaced

The review exposed two object-system changes CoreProtocols depends on.
Both are compiler-side (not FFI), and both should land *before*
Phase 1 so the library is built on solid ground.

### 6a. Auto-constructor calls `initialize` (enables feedback #3)

For the initialization lifecycle (Layer 0, §3) to be universal — to
fire for every class whether or not someone hand-wrote a constructor —
the object system's auto-generated `<name>` constructor must append a
call to the `initialize` generic:

```
<name> ( slots... -- obj )   ==   name boa  initialize
```

with a default `METHOD: initialize ( o:object -- o ) ;` (no-op) so
the call always resolves.  Small emit change in `emit_class`; the
payoff is that `:after initialize` becomes the universal,
CLOS-faithful setup hook with automatic parent-before-child layering.

### 6b. Full-arity positional specializer emit (fixes a latent bug; answers feedback #6)

Feedback #6 asked which arguments participate in dispatch, and worried
about exponential cost.  Investigating it surfaced a **real latent bug
in the current `emit_method`**: it emits a class list containing *only
the specialised parameters*, not one entry per input.  So a method
that dispatches on a non-top argument emits a class list that
multi-methods aligns to the **wrong stack position**.

Example of the bug today:
```
METHOD: foo ( a:cat b -- )      \ intend: dispatch on a (the deeper arg)
\ current emit →  multi-methods:METHOD: foo { cat } ...
\ but { cat } aligns to the TOP arg (b), so it dispatches on the wrong one.
```

This happens to work *only* when the single dispatched argument is
already on top — which is why no test has caught it yet (every method
so far dispatches on top, or on all args).

**The fix, and it's the CLOS-faithful one:** every method input gets a
positional specializer, defaulting to `object` when written bare, and
emit produces a **full-arity, positionally-aligned** class list:

```
METHOD: foo ( a:cat b -- )   →   multi-methods:METHOD: foo { cat object } ...
METHOD: at! ( x i c:vector -- ) → multi-methods:METHOD: at! { object object vector } ...
```

This matches CLOS exactly (every required parameter has a specializer,
`t`/`object` for "don't care"), removes all positional ambiguity, and
keeps dispatch cheap: positions that are `object` across *all* methods
of a generic collapse and cost nothing — so the "exponential" worry
only materialises if you genuinely write methods that branch on many
arguments, which is pay-for-what-you-use.

The dispatch *profile* (which slots branch) is therefore inferred from
the methods, exactly as CLOS does — no new `GENERIC:` syntax required.
An optional `GENERIC:`-level annotation marking dispatch slots could
be added later purely for documentation/clarity, but it isn't needed
for correctness once emit is full-arity.  This also reinforces the
collection-on-top argument-order rule (§Layer 1): with full-arity
emit, *any* order dispatches correctly, but collection-on-top still
minimises stack churn.

**Action:** fix `emit_method` to pad to full arity with `object`, add
a regression test that dispatches on a non-top argument, and verify
multi-methods' position alignment with a fixture.  Tracked as a task.

---

## 7. What "natural for a CLOS mind" means here, concretely

A user steeped in CLOS should be able to predict our API:

- You **define a class** with slots, and **add methods** to existing
  generics — you never edit a library file to make your type
  participate.
- You **reach for a generic function**, not a method-on-receiver:
  `board show`, `node stream render`, `app event handle`.
- You **extend behaviour by adding a method**, including on a class
  combination the library author never anticipated (multimethods).
- You use **`:before` / `:after`** for the aspect-y concerns
  (logging, repaint, validation) and keep primaries clean.
- You compose: a `grid` *holds* cells, an `app` *holds* a `canvas`, a
  document *holds* nodes — shallow trees, no inheritance gymnastics.

If those reflexes produce working programs without surprises, the
library has done its job.

---

## 8. Open questions to settle before Phase 1

1. **Name.**  ✅ Settled: **CoreProtocols**.  Vocab/file naming
   follows from it — e.g. Forth source under a `core-protocols/`
   tree, layer vocabs like `core.seq`, `core.stream`, `core.gui`
   (final spelling TBD when the first file lands).

2. **Argument order.**  ✅ Settled (feedback #2): accessors/mutators
   put the **collection on top** (`at ( i c )`, `at! ( x i c )`,
   `at-xy ( x y g )`), matching Factor's `nth`/`set-nth`; combinators
   put the **quotation on top** (`each ( c quot )`).

3. **Error style.**  ✅ Settled (feedback #4): **genuine faults
   THROW** (out-of-bounds index, file-not-found) via ANS
   `THROW`/`CATCH` — flags there wreck stack ergonomics and pollute
   composition.  **Graceful, expected state transitions return
   sentinel objects** you can dispatch on (EOF = `<eof>`, "not found"
   = a `<missing>` singleton, etc.).  The dividing line: *is this an
   error, or a normal outcome the caller will branch on?*  Errors
   throw; outcomes are objects.  (Needs ANS `THROW`/`CATCH` wired in
   the runtime — verify before Phase 1; tracked as a task.)

4. **`grid` indexing convention** — ✅ Settled: **0-based, addressed
   `(x, y)`** (column then row), row-major (`index = y*w + x`).
   Matches canvas coordinates so the grid and the GUI layer agree.
   Shipped in `lib/collections.f`.

5. **`equals?` / `hash` contract** — needed together for `dict`/`set`
   keys.  Decide whether value classes auto-derive both from their
   slots (convenient, needs object-system support) or require explicit
   methods (simple, more boilerplate).  Leaning explicit-for-now,
   auto-derive as a later ergonomic.

None of these block starting Phase 1 (text Othello); they just want a
decision before the code calcifies around an accident.  Items 1–3 are
now decided; 4–5 have a recommended default to confirm.
