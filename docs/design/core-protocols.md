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

Where the absence of `call-next-method` (a deliberate non-goal)
would otherwise bite — chaining to a parent's behaviour — we use
**composition**: the subclass holds/calls a helper explicitly rather
than implicitly climbing the class precedence list.  In practice the
shallow-tree discipline means this rarely comes up.

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
Layer 0  Core protocol      show · equals? · hash · clone
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
GENERIC: show    ( obj -- )       \ human-readable, what DUMP/. lean on
GENERIC: equals? ( a b -- ? )     \ value equality (double-dispatched)
GENERIC: hash    ( obj -- n )     \ for dict/set keys
GENERIC: clone   ( obj -- obj' )  \ shallow copy

\ Sensible catch-alls so an un-extended class still "works":
METHOD: show    ( x:object -- )      ." <" ... class-name ... ">" ;
METHOD: equals? ( a:object b:object -- ? )  2dup eq? ;   \ identity
```

`DUMP` (already shipped) and `show` cooperate: `DUMP` is the raw
type+bytes view for debugging; `show` is the pretty, class-defined
view for users.

### Layer 1 — Collections

One protocol, several backings.  The protocol:

```forth
GENERIC: size  ( c -- n )
GENERIC: at     ( c i -- x )
GENERIC: at!    ( x c i -- )
GENERIC: each   ( c quot -- )      \ quot: ( x -- )
GENERIC: push   ( x c -- )
GENERIC: pop    ( c -- x )
```

Concrete classes, each stealing a Factor vocab:

| class    | backing (stolen)        | used by         |
|----------|-------------------------|-----------------|
| `vector` | Factor `vector`         | markdown blocks |
| `grid`   | vector + (w,h) slots    | Othello board   |
| `dict`   | Factor `hashtable`      | link refs, counts |
| `set`    | Factor `hash-set`       | legal-move sets |
| `queue`  | Factor `dlist`          | BFS / flood     |
| `stack`  | Factor `vector`         | parser state    |

`grid` is worth its own class because 2-D indexing (`g x y at-xy`) and
bounds-checking recur in board games and image work.

```forth
CLASS: grid SLOT: w SLOT: h SLOT: cells ;
: <grid> ( w h -- g ) ... ;
GENERIC: at-xy  ( g x y -- v )
GENERIC: at-xy! ( v g x y -- )
GENERIC: in-bounds? ( g x y -- ? )
```

Othello board render via the `show` protocol (text milestone):
```forth
METHOD: show ( b:board -- )
    b h>>grid [ ... each row ...
        row w>>grid [ at-xy cell>char emit ] ...
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
from or go:

```forth
GENERIC: read-char ( s -- ch ? )    \ ? = false at end
GENERIC: read-line ( s -- str ? )
GENERIC: write     ( str s -- )
GENERIC: write-line ( str s -- )
GENERIC: at-end?   ( s -- ? )
GENERIC: close     ( s -- )

CLASS: string-stream SLOT: buf SLOT: pos ;   \ in-memory, pure Forth
\ file-stream lives in Layer 4 (needs FFI)

\ RAII: open, run a quotation, always close (even on error).
: with-stream ( stream quot -- )  ... ;      \ guard via :after-style cleanup
```

Markdown reads from *a stream* and writes to *a stream*; in tests it's
`string-stream` both ends (fast, no files), in the CLI it's a
`file-stream` in and out.  Same parser code.  That substitutability is
the whole point of the protocol.

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

## 6. What "natural for a CLOS mind" means here, concretely

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

## 7. Open questions to settle before Phase 1

1. **Name.**  ✅ Settled: **CoreProtocols**.  Vocab/file naming
   follows from it — e.g. Forth source under a `core-protocols/`
   tree, layer vocabs like `core.seq`, `core.stream`, `core.gui`
   (final spelling TBD when the first file lands).
2. **`grid` indexing convention** — `(x,y)` vs `(row,col)`, 0- vs
   1-based.  Pick once, document loudly.
3. **`equals?` and `hash` contract** — needed together for `dict`/`set`
   keys; decide whether value classes auto-derive them from slots or
   require explicit methods.
4. **Error style** — do collection bounds errors / stream EOF throw
   (ANS `THROW`/`CATCH`) or return a flag?  Forth tradition leans on
   flags; pick a consistent convention library-wide.

None of these block starting Phase 1 (text Othello); they just want a
decision before the code calcifies around an accident.
