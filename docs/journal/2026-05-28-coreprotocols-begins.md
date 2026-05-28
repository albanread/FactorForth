# 2026-05-28 — CoreProtocols begins (and manuals worth reading)

Two threads this afternoon: a documentation pipeline that finally
feels first-class, and the first real bricks of the standard class
library.  They turned out to be related — the docs were the warm-up
that made building the library feel inevitable.

## Manuals, with diagrams

The user dropped in the new DocCrate — their native Direct2D markdown
browser — now with **Mermaid support via the Selkie engine**, rendered
straight to Direct2D (no browser, no JavaScript).  And a confession:

> "I never mentioned this but I love a good computer manual, and now
> we can create them for users."

So do I, as it turns out.  We wrote `coreprotocols.md` as a
diagram-heavy showcase — verb-first dispatch, the class hierarchy, the
six layers, the stream EOF state machine, the event-loop sequence —
then retrofitted Mermaid into `architecture.md` (the compiler pipeline
as a flowchart, the VM round-trip as a sequence diagram),
`classes.md` (the `CLASS:` desugar fan-out, single inheritance), and
`let-algebra.md` (the LET lowering flow).

### The snapshot feature, built for an agent

The quiet gift was in DocCrate's source: a `--testsnap <file>
[--scroll <px>] [--scrollto <line>]` mode that opens a specific page,
scrolls, and writes a PNG — then I read it back.

> "we create that snapshot feature for you glad you like it :)"

It changed how I work on docs.  Instead of fragile full-screen grabs,
I drive DocCrate like a CLI and *see the page as the user will*.  That
loop caught two real rendering bugs in my own markdown the moment I
made them:

  - Selkie renders `<br/>` **literally** inside node labels — so
    multi-line labels showed the tag as text.  Switched to a ` — `
    separator that wraps naturally.
  - `<`, `>`, `>>` are hazardous in Mermaid node labels (shape
    syntax).  Describe in the label; keep exact spellings in the
    adjacent table.

Author → snapshot → look → fix, in seconds.  A documentation tool you
can *see through* is worth a great deal.

## CoreProtocols, Layer 0: `show`

Then the library itself.  The governing rule held: CoreProtocols is
written in **ordinary ANS Forth on the object system** — nothing
special-cased in the compiler.  Layer 0's first brick, in
`lib/core.f`:

```forth
GENERIC: show ( x -- )
METHOD:  show ( x:object -- )  drop ." <object>" ;   \ total fallback
: show-ln ( x -- )  show cr ;                          \ reuse over the generic
```

`show` is the pretty, class-defined view (distinct from `DUMP`, the
raw debugging view).  The object catch-all keeps it *total*; `show-ln`
is written once over the generic and works for every class that
implements `show`.  Tests `include_str!` the shipped `lib/core.f` so
the artifact and the test can't drift — the discipline the design doc
asked for: *the library is ANS Forth source, tested by loading it the
way users will.*

## Layer 1: `grid`, 0-based, (x, y)

The board game needs a grid, which raised the indexing question.  The
user settled it from experience:

> "I think 0 based I created a 1 based BASIC and feel like I regret it
> a bit, x,y I like for a grid."

So: **0-based, addressed `(x, y)`**, row-major (`index = y*w + x`) —
matching canvas coordinates so the grid and the future GUI layer
agree.  `lib/collections.f`:

```forth
3 2 new-grid VALUE board
11  0 0 board at-xy!      \ set (x=0, y=0)
0 0 board at-xy .         \ -> 11
3 0 board in-bounds? .    \ -> 0   (x == width, out)
```

### Per-instance storage, stolen and hidden

A grid's cells live in a Factor fixed array tucked into a slot.  To
reach it from ANS Forth without exposing Factor, three new
boot-defined primitives wrap `<array>` / `nth` / `set-nth`:
`<cells>` / `cells@` / `cells!`.  Same "steal the substrate, hide it
completely" move as the Programming-Tools.  Crucially, **no
object-system change was needed** — `grid` is a plain `CLASS:` with
slots; `new-grid` allocates the cells and calls the auto `<grid>` boa.
The `initialize`-lifecycle prerequisite I'd flagged turned out not to
block anything; a class that needs setup just calls it from its own
constructor.

### A small ergonomic lesson

`at-xy! ( v x y g -- )` takes four arguments with the collection on
top.  `rot` (three-element) can't both keep the grid *and* deliver it
to the top, so the clean idiom is a `VALUE` holding the board —
`11 0 0 board at-xy!`.  That friction is itself an argument for the
convenience layer we're building, and a good thing to show users.

## Stats

  - docs: CoreProtocols page + Mermaid in architecture / classes /
    let-algebra; ~12 diagrams, all verified rendering via `--testsnap`
  - `lib/core.f` (Layer 0 show) + `lib/collections.f` (Layer 1 grid)
  - 5 CoreProtocols tests (3 show, 2 grid); 127 lib tests green
  - 3 boot primitives (`<cells>`/`cells@`/`cells!`) + resolver/effect
  - commits: 3163216 (Layer 0), 391d4d0 (Layer 1), plus the doc
    commits

## Reflection

The pattern for the whole library is now established and proven on two
layers: write the protocol as shippable ANS Forth, test by loading it
the way users will, ship it in `lib/`, document it with a diagram you
can actually see.  PowerMOPS had a class library; ours is taking shape
the same way good Forth always does — small words, composed, each one
load-tested before the next.  Text Othello — the Phase 1 capstone — is
now just a matter of writing the game on top of `grid` and `show`.

— end of CoreProtocols-begins entry
