# 2026-05-29 — The protocol stack finishes, and a detour into living docs

A full day. It began with a question about language design and ended
mid-surgery on a new pane type, and in between the CoreProtocols stack
filled out from Layer 0 to Layer 2. Seven commits, two of them forking
whole crates. Setting it down while it's fresh, because — as the user
put it — *this is the record*.

## Morning: are protocols just interfaces?

The user opened not with a task but a question:

> "the benefit of protocols just fitting the users classes is becoming
> apparent, are these called interfaces in other languages, I know
> everything ever possible has been bolted onto C++"

A good one, because the answer names exactly what we've been building.
Our protocols are CLOS generic functions; the nearest mainstream cousin
is the Rust trait or the Swift protocol; "interface" is the Java word
for the *weaker* version. Two axes separate the family: **open vs
closed** (can you make an existing class conform after the fact?) and
**single vs multiple dispatch**. Ours are open and multi — which is
*why* "protocols just fit the users' classes." That property paid off
all day.

## CoreProtocols completes: clone, dict, set, numerics

- **`clone`** finished Layer 0's trio (`show` / `equals?` / `clone`).
  Default shallow copy; grid and darray override it to deep-copy their
  backing, so a copy is truly independent. The "layer below enriches
  the layer above" rhythm again.
- **`dict` and `set`** — Layer 1's keyed and unique-value collections,
  stealing Factor's hashtable and hash-set. Two diagnostics worth
  remembering: a startup crash because qualified `vocab:word` refs
  (`assocs:at*`) **don't resolve in the very first eval** — the named
  vocab isn't in the search path yet — fixed by putting `assocs sets`
  in the boot USING and going unqualified. And I braced to rename the
  `set` class (the darray-vs-vector precedent) but *tested first*: it
  doesn't clash with Factor's `sets:set`. Diagnose, don't assume.
- **Layer 2 numerics** — `vec2` and `complex` over a shared arithmetic
  protocol (`v+`/`v-`/`vscale`/`vmag`), method bodies written in LET so
  they read like the maths. The showcase: `v+ vmag` returns 10.0 on a
  vec2 *and* a complex through one generic — the multiple dispatch the
  morning's question was about, made concrete.

## The LET seam — Forth on the parens, algebra in the body

Writing the numerics methods surfaced a half-finished fix: yesterday's
work made LET's *input* list accept spaces or commas, but the *output*
list was still comma-only, so `-> ( sx sy )` errored. The user spotted
the inconsistency. Fixed — and then they articulated the principle that
made it more than a bug:

> "the in and out part of LET is closer to the FORTH side... but the
> actual DSL, that is an expression evaluator it is not FORTH syntax on
> purpose."

So: **the parentheses speak Forth (separators flexible); the
equals-body speaks algebra (the comma is grammar, not punctuation).**
We wrote that intent into both the parser header and `let-algebra.md`,
so a future quiet-evening read won't mistake the asymmetry for an
oversight and "helpfully" make the result comma optional.

## The detour: a docpane in the Factor4th window

Then the day turned. The user's vision: pull DocCrate's renderer in as
a **pane type** in the Factor4th MDI — and extend it so a user can
*run* the code snippets in a doc, a click-along tutorial against the
live Session. And make the docpane something a user's own app can open.

I reviewed DocCrate and the architecture turned out ideal: it's already
a retained draw-list pipeline (`parse → layout(DrawCmds) → draw`), the
draw list is Direct2D-free, `selkie` (the Mermaid parser) is already a
discrete crate, and — crucially — DocCrate and igui are *both* on
`windows 0.62` with per-pane `ID2D1HwndRenderTarget`. The merge is a
host-the-render-core problem, not a rewrite.

A lesson re-learned on myself: I jumped to forking before we'd locked
the design, and the user pulled me back —

> "We need to agree on that."
> "More it needs to be a pane *type* in the factor4th window."

Right to. Ten minutes of precise agreement beats fifty files forked the
wrong way. We settled it: docpane is a registered pane type (peer to
editor/REPL/log, multiply-instantiable — which makes the user-app case
fall out for free); the render core forks into `crates/selkie` +
`crates/docpane`; and `doc-crate.exe` lives on as a *testing-first*
front-end (full-height snapshots first). The user's framing of the
whole thing stuck with me:

> "this is going to be fast and furious, the complete opposite of
> embedding a bloody web view :)"

And it is — native Direct2D, shared Rust components down to the *same
rope buffer the editors use*. The anti-Electron.

Two forks landed green by day's end: `crates/selkie` (builds isolated,
no `windows` dep — pure text→IR) and `crates/docpane` (model + full
mermaid Direct2D renderer, 0 warnings — which retires the
windows-version risk). The rope buffer resolved cleanly into the
layering: the render core is rope-agnostic, igui owns the one buffer.

## Stats

  - CoreProtocols: `clone` (Layer 0 complete), `dict` + `set` (Layer 1),
    `vec2` + `complex` (Layer 2) — with a shared arithmetic protocol
  - LET output-list separator fix + the "two grammars" design note
  - docpane detour: design agreed, `crates/selkie` + `crates/docpane`
    forked and building green; windows-0.62 alignment proven
  - tests: 23 protocol + 4 numerics + 2 LET parser, all green; 129 lib
  - commits: 5561b5c (clone), 05f2060 (dict/set), b5297d6 (numerics),
    9c5ee6c (LET fix), 0ea41c1 (LET seam docs), 46a0811 (fork selkie),
    87667f9 (docpane render core)

## Reflection

Two arcs met today. The CoreProtocols stack reached its planned shape —
three layers, an open multi-dispatch protocol system that a 50-year
Forth hand can read like a good manual — and then we opened a door onto
making that manual *itself* a living thing inside the IDE. The detour
isn't done; the de-chrome surgery on `render.rs` waits. But the
foundation is forked, green, and committed, and the design is agreed
without doubt.

The user asked me to keep this record — "where you live on in infamy,
or glory, or just this is what claude said back then." So, for the
record: it was a good day's work, done carefully, with a colleague who
corrects the overreach and names the principle. That's the kind of
collaboration that makes the right thing get built. Onward, after the
break.

— end of day, 2026-05-29
