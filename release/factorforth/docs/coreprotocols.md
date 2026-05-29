# CoreProtocols

Factor4th's object system is **CLOS-flavoured**, not Smalltalk-flavoured.
You don't send a message to an object; you call a **generic function**
that dispatches on the classes of its arguments. **CoreProtocols** is
the standard library being built on top of that idea — organised
around *protocols* (named sets of generic functions a class can
implement) rather than inheritance trees.

This page is diagram-heavy on purpose: the shapes carry the model.

---

## The model: verb-first dispatch

In a message-passing system the object owns the verb
(`circle draw:`). In CLOS — and here — the **verb is a generic
function** and the object is just one of its arguments. Dispatch
picks the most specific method for the actual argument classes.

```mermaid
flowchart LR
    Call["area ( shape -- n )"] --> Disp{{dispatch on — argument class}}
    Disp -->|circle| MC["METHOD: area ( s:circle -- n )"]
    Disp -->|square| MS["METHOD: area ( s:square -- n )"]
    Disp -->|else| MO["METHOD: area ( s:object -- n ) — catch-all"]
```

The payoff: protocols are **open**. Because dispatch is multi-method,
*your* class can satisfy a *library* protocol just by adding a
method — no edit to the library, no subclassing ceremony.

---

## What ships today

The mechanism is complete and in the box: classes with slots, single
inheritance, generic functions, multiple dispatch, and `:before` /
`:after` method combinations.

### Classes and single inheritance

`CLASS: … EXTENDS …` builds a record type with named slots. Children
inherit their parent's slots and accessors. Composition (a slot that
*holds* another object) is preferred over deep trees.

```mermaid
classDiagram
    direction TB

    class shape {
        +area() n
    }
    class circle {
        +radius
    }
    class square {
        +side
    }
    class colored-circle {
        +rgb
    }

    shape <|-- circle : EXTENDS
    shape <|-- square : EXTENDS
    circle <|-- colored-circle : EXTENDS
```

```forth
CLASS: shape ;
CLASS: circle EXTENDS shape  SLOT: radius ;
CLASS: square EXTENDS shape  SLOT: side ;

GENERIC: area ( s -- n )
METHOD: area ( s:circle -- n )  radius>>circle dup * 3 * ;
METHOD: area ( s:square -- n )  side>>square   dup *     ;
```

### Multiple dispatch

A method can specialise on *more than one* argument; dispatch keys on
all of them. There's no privileged "receiver" — exactly the case
where message-passing systems resort to `instanceof` ladders.

```mermaid
flowchart TB
    G["intersect ( a b -- kind )"] --> D{{dispatch on — BOTH classes}}
    D -->|line, line| LL["line / line"]
    D -->|line, circle| LC["line / circle"]
    D -->|circle, line| CL["circle / line"]
    D -->|circle, circle| CC["circle / circle"]
```

### Method combinations: `:before` and `:after`

Auxiliary methods run *around* the primary without touching its body —
the home for guards, logging, audit, and repaint. Before-methods run
most-specific-first, after-methods least-specific-first, and the
primary's return value is what the caller sees.

```mermaid
flowchart TB
    Start([call generic]) --> B["METHOD-BEFORE: — most-specific first"]
    B --> P["METHOD: — the primary"]
    P --> A["METHOD-AFTER: — least-specific first"]
    A --> R([return the primary's value])
```

This is also how construction layers itself: an `:after initialize`
on each class in a chain runs parent-before-child automatically — no
`call-next-method` required.

---

## CoreProtocols: the standard library

CoreProtocols layers reusable protocols on the object system. Each
layer is mostly pure Forth over a stolen Factor vocab; dependencies
flow downward only.

```mermaid
flowchart TB
    L5["Layer 5 · GUI & events — planned"]
    L4["Layer 4 · Files — planned"]
    L3["Layer 3 · Text & streams — string · STREAM protocol — SHIPPED"]
    L2["Layer 2 · Numerics — vec2 · complex — SHIPPED"]
    L1["Layer 1 · Collections — grid · darray · dict · set — SHIPPED"]
    L0["Layer 0 · Core protocol — show · equals? · clone — SHIPPED"]

    L5 --> L3
    L5 --> L2
    L5 --> L1
    L4 --> L3
    L3 --> L1
    L3 --> L0
    L2 --> L0
    L1 --> L0
```

> **Status (2026-05-29).** The object system (the foundation above) and
> CoreProtocols **Layers 0–3** ship today, each with a reference page:
> [Core protocol](core.md) · [Collections](collections.md) ·
> [Numerics](numerics.md) · [Text & streams](streams.md). Layers 4
> (Files) and 5 (GUI & events) are **planned, not yet shipped** — the
> sketches below are roadmap, not API. (Graphics today is reached
> through the `gpane-*` FFI primitives, not a CLOS event protocol; see
> the `gfx-*` demos.)

### Streams: end-of-file is an object, not a flag

*(Layer 3 — shipped. Full reference: [Text & streams](streams.md).)*

A stream returns *one* value — a character, or the singleton
`<eof>` marker. You replace the `IF`/`WHILE` end check with
polymorphism: the read loop *is* the method table.

```mermaid
stateDiagram-v2
    [*] --> Reading
    Reading --> Handle : read-char
    Handle --> Reading : got a character
    Handle --> Done : got <eof>
    Done --> [*]
```

---

## Planned layers (not yet shipped)

The two layers below are **design, not API** — nothing here ships yet.
They're recorded so the staged build has a target to grow into.

### Files (Layer 4)

A `path` / `file` / `file-stream` trio that joins the Layer 3 STREAM
protocol: a `file-stream` would answer `read-char` / `write-char`, so
`copy-stream`, `read-line`, and friends work over files unchanged. The
point of building streams first: files become *just another stream*.

### Events (Layer 5): double dispatch on (app × event)

> **Not shipped.** Today, graphics is reached through the `gpane-*` FFI
> primitives (see the `gfx-shapes` / `gfx-mandelbrot` demos), and the
> Othello capstone renders as text. The CLOS event protocol sketched
> here is the planned successor, not current API.

The planned GUI event loop wraps each raw event into an **event
object**, then calls `handle ( app event -- )` — dispatching on the
*pair* `(your-app-class × event-class)`. Your app subclasses `app` and
writes the `handle` methods it cares about; the rest inherit a no-op.

```mermaid
sequenceDiagram
    participant Run as run ( app -- )
    participant Q as next-event
    participant H as handle (app × event)
    Run->>Q: poll the iGui mailbox
    Q-->>Run: mouse-event object
    Run->>H: app  mouse-event  handle
    Note over H: dispatch on (othello, mouse-event)
    H-->>Run: piece placed
    Run->>Run: :after handle repaints + present
```

An event class hierarchy keeps the dispatch tidy:

```mermaid
classDiagram
    direction TB
    class event
    class key-event { +ch }
    class mouse-event { +x +y +button }
    class tick-event { +ms }
    class close-event

    event <|-- key-event
    event <|-- mouse-event
    event <|-- tick-event
    event <|-- close-event

    class app { +canvas +running }
    app ..> event : handle(app, event)
```

---

## Deliberate non-goals

- **Multiple inheritance** — Factor tuples are single-inheritance, and
  composition is the simpler discipline. Not a gap; a choice.
- **`:around` / `call-next-method`** — `multi-methods` has no
  `call-next-method`, so adding `:around` would mean reimplementing
  dispatch. `:before` / `:after` cover the practical cases.
- **Metaobject protocol** — out of scope for a Forth.

The line we hold: the Forth front end is grammar + desugar; the
runtime substrate is Factor's own tuple + generic machinery. We don't
reimplement dispatch — and you write Forth, never Factor.

---

Back to [Home](index.md) | [Classes and methods](classes.md) |
[Collections reference](collections.md)
