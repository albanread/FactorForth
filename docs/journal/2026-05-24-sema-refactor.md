# 2026-05-24 — Sema spine, phase dumps, corpus design

**Shipped**

- `Sema` struct as the whole-program semantic database — aggregates
  resolve + effect outputs and adds call_graph + use_sites built
  via a single AST walk.
- Dump infrastructure (`compiler::dump`) for every pipeline stage:
  tokens, AST, sema, effects, IR, and `all`.  Human- and AI-
  readable, with spans for cross-reference back to source.
- CLI surface: `newfactor --dump=<stage>` and `newfactor --eval=EXPR`
  let the user inspect compiler state at any layer.
- 87 lib + 22 integration tests, all green.

**Open**

- M2.8 (VARIABLE/CONSTANT/FCONSTANT + variable narrowing) lands
  on top of this spine.
- Corpus / replay model (see below) is sketched but not implemented
  — current single-source compile is one-element corpus.

---

## Why a sema spine matters

Before today the compiler was a sequence of independent walkers.
Each pass (resolve, effect) did its own AST walk, produced a
narrow output, and forgot what it learned beyond that output.
Variable narrowing (M2.8) forces the issue: deciding "is `x`
narrow or wide?" requires seeing **every use of `x` across every
definition** before answering.  So we need a whole-program fact
database that's built once and queried many times.

`Sema` is that database.  The struct exposes its tables directly
— no hidden state, no setter machinery — and each sub-pass owns
one slot it writes into.  Mutable across sub-passes (simpler
than fully immutable), conservative on uncertain cases (false
negatives go to the slow path, which is correct; false positives
would be silently miscompiled).

What lives in Sema today:

```
program            — the AST
word_targets       — per-WordRef span → emit target  (from resolve)
user_words         — name → def info                 (from resolve)
user_effects       — name → inferred stack effect    (from effect)
effect_errors      — declared-vs-inferred mismatches (from effect)
call_graph         — caller name → callee names      (from sema)
use_sites          — referenced name → ref spans     (from sema)
escape             — variable → escape state         (M2.8+, empty)
```

The escape map sits empty for now; M2.8 fills it.

## Phase dumps: the spec

The user asked for a way to see what the compiler sees at every
stage.  Rich diagnostics for a small compiler.  Five stages plus
`all`:

- **TOKENS** — every token with span and kind.
- **AST** — the parsed structure, indented; control-flow nodes
  show their sub-bodies; spans on every leaf.
- **SEMA** — user words with declared/inferred effects, variables
  and constants (placeholder for M2.8), call graph, use sites,
  escape analysis (placeholder).
- **EFFECTS** — focused subset of sema; one line per user word
  with its current effect.
- **IR** — the emitted Factor source, header + body.
- **ALL** — concatenation with thick separators.

Example session:

```
$ newfactor --eval=": square ( n -- n^2 ) dup * ; 5 square ." --dump=sema
SEMA
═════════════════════════════════════════
User words (1):
  square           declared ( 1 → 1 )           inferred ( 1 → 1 )
                     def @ 1:3-1:9

Variables: (M2.8 — not yet collected)
Constants: (M2.8 — not yet collected)

Call graph:
  square  →  *, dup

Use sites:
  *                @ 1:27-1:28
  .                @ 1:40-1:41
  dup              @ 1:23-1:26
  square           @ 1:33-1:39

Escape analysis:
  (M2.8 — not yet collected)
```

The audience here is dual: a human debugging the compiler, and
Claude (or another LLM) reading state in a future session without
re-deriving from source.  The format is regular enough to
machine-parse if needed but readable as plain text.

`dump_all` exists so we can snapshot every stage of a compile
into one document — useful for reproducible bug reports and for
journal entries when something interesting happens.

## The corpus conversation

The user pushed on a subtle point: ANS Forth state is **sequence-
dependent**.  `: foo 1 ;` then `: foo 2 ;` shadows the first; ANS
`MARKER / FORGET` rolls back arbitrary additions.  Our current
batch compile (single source string → fresh Sema) doesn't grapple
with this at all, but Phase 3's REPL will.

After working through alternatives (epoch-per-entry, mutable
incremental Sema, etc.), we landed on:

**The corpus is the source of truth.  Sema is `analyze(corpus)`.**

A `Corpus` is a `Vec<SourceUnit>`.  Each addition appends a unit
and triggers a full re-analysis.  No epoch bookkeeping inside
Sema, no incremental mutation logic.  Just sequential re-derivation.

Performance: O(N) per addition, fine for interactive sessions in
the tens-to-hundreds of units.  Whole-program analysis runs in
milliseconds.

### What about FORGET

We went around on it.  Conclusion:

- **FORGET is out.**  We don't implement it.
- **Replay is the tool we have if we ever want it.**  Definitions
  are idempotent under replay (colon defs, VARIABLE, CONSTANT,
  CREATE, MARKER); interpreter expressions are not (they have
  side effects).  Our AST already classifies via `Item::Definition`
  vs `Item::TopLevel`, so the filter is free.
- **"Start over" = restart the process.**  Same as Python or any
  other REPL.  ANS doesn't actually require FORGET (it's optional
  in the 1994 spec).
- **If a real user asks for FORGET later**, we'll do the replay-
  only-definitions version and document the trade-off: variable
  data state past the marker dies, and init code that happened to
  run at top level needs to be re-fired manually.

### Why this matters even though we don't implement it

Two things:

1. **It told us not to put epoch fields in Sema.**  I was about to
   add `epoch: u32` to every dictionary entry.  Corpus-as-source-
   of-truth subsumes that — the corpus has natural ordering, so
   per-entry epochs are redundant.

2. **It clarifies the Phase 3 REPL shape.**  Each `add_source(s)`
   call appends to the corpus, runs `analyze()`, and emits only
   the IR for the new unit (Factor's dictionary already has the
   prior units' compiled forms).  Monotonic, simple, debuggable.

## What surprised me

The corpus design is meaningfully simpler than what I was about
to implement.  My instinct was to bake epochs into Sema for
flexibility; the user's framing (corpus is canonical, Sema is
derived) collapses the design.  I had it backward.  Worth
catching.

Also: how much value the dump infrastructure adds even without
any new analysis.  Just being able to type `--dump=ast` and see
the parsed structure with spans makes every future bug-hunt
easier.  Should have shipped this earlier.

## Pipeline state

```
[ corpus: Vec<SourceUnit>  ←  Phase 3's REPL appends here ]
                            |
                            ▼
                       analyze(corpus)
                            |
                            ▼
              ┌─────────────────────────────┐
              │ Sema                        │
              │  program (AST)              │
              │  word_targets               │
              │  user_words                 │
              │  user_effects               │
              │  effect_errors              │
              │  call_graph                 │
              │  use_sites                  │
              │  escape           (M2.8+)   │
              └─────────────────────────────┘
                            |
                            ▼
                       emit(&Sema)
                            |
                            ▼
                       Factor IR
                            |
                            ▼
                  nf_eval_string  →  output
```

For the batch compiler today, corpus has one element and is
implicit.  Phase 3's `Session` makes it explicit.
