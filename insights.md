# NewFactor — insights

A standing account of what this project *is* and why it's shaped the
way it is — the mental model behind the code, for anyone (human or
agent) picking it up cold. Companion to `MANIFESTO.md` (normative) and
`current_status.md` (point-in-time).

## The core bet

NewFactor is a **Rust ANS Forth compiler that targets Factor's VM as a
back-end**. Factor's runtime is a mature, JIT-quality engine for stack
languages — tree-IR → CFG → linear-scan codegen, polymorphic inline
caches, a generational GC — and ANS Forth *is* a stack language. So
rather than build a Forth VM from scratch, we reuse Factor's and spend
our effort on the front end and the language surface.

## Two things that make this pay off

**1. The GC is free.** This is the quiet headline. NewFactor inherits
Factor's generational collector as part of the VM — we never write or
debug our own. That is a deliberate, strategic difference from the
sibling projects, each of which spends real blood on its collector:
NCL's page-heap GC, NewOpenDylan's precise roots / `gc.statepoint`
work, WF64's STC machinery. Memory management is the single hardest,
most non-deterministic subsystem in a language implementation, and here
it's somebody else's solved problem. We get to think about *language*,
not about troubleshooting a collector.

**2. The full image is an asset, not bloat.** An earlier plan was to
strip Factor down to a single-digit-MB embeddable image (drop `ui.*`,
`tools.*`, export an `nf_vm_*` C API). **That slim-image work is
parked** — on purpose. The current direction is to build a *rich
protocol library on top of Factor's own libraries*, so the whole point
is to keep Factor's extensive vocab ecosystem available to draw on.
Slimming the image would amputate exactly what we want to exploit.
The slim/embeddable path remains a possible future, not a current goal.

## The hard line: Factor-as-target, not Factor-as-language

The user writes ANS Forth and **never sees Factor** — not in source,
not in error messages, not in tooling. Rust does the entire Forth front
end and emits *canonical, machine-generated Factor source as an IR*;
Factor's parser + optimizer + JIT then run it. The Factor text is an
implementation detail (swappable later for direct VM-cell emission).
The trap we refuse is letting Factor's surface syntax, listener, or
stock image leak into the user's world — *Factor-as-target is the
architecture; Factor-as-language is the trap.*

## The pipeline

```
ANS Forth source
  → lex      tokenise (number/char/string prefixes; LET blocks captured whole)
  → parse    AST (defs, control flow, CLASS:/GENERIC:/METHOD:, LET)
  → sema     resolve + effect inference + escape analysis + lower_* desugars
  → emit     canonical Factor IR text
  → Factor   its parser → optimizing compiler → JIT
```

A thin `forth.runtime` Factor vocab supplies what ANS needs and Factor
doesn't express directly: the byte-array memory model, an ANS return
stack, ANS booleans (`-1`/`0` vs Factor's `t`/`f`), floored `MOD`, the
pictured-numeric DSL, and so on. Everything else maps onto words Factor
already has.

### Resolve — the dictionary

`builtin_table()` in `resolve.rs` is the authoritative ANS→Factor word
map ("the single source of truth"). The standard-library `.f` files add
the rest, loaded via `NEEDS`/`INCLUDED`. A word in neither does not
resolve.

### Effects — why they exist

The stack-effect pass looks like a linter but isn't, mainly. Factor's
`:` requires a stack-effect annotation and Factor runs its own *strict*
inferencer; so our pass's load-bearing job is to **synthesise a
concrete `( N -- M )` annotation from each body** that Factor will
accept and the JIT can inline against. When it can't size a body it
falls back to row variables `( ..a -- ..b )`, which compiles but kills
optimization. Two cooperating systems: ours (count-based, permissive,
warnings) feeds Factor's (typed, strict, errors). See
`docs/design/effects.md`.

## The language surface

**Object system (CLOS by desugar).** `CLASS:`/`SLOT:`/`EXTENDS`,
`GENERIC:`/`METHOD:`, `:before`/`:after` lower onto Factor's
`TUPLE:` + `multi-methods` — no reimplemented dispatch. Dispatch is
genuinely multi-method (keys on every specialized argument) and *open*
(your class joins a library protocol by adding a method). Each such
feature is a 100–300 line desugar pass, not a runtime word fighting the
optimizer.

**CoreProtocols.** A layered standard library written in ordinary ANS
Forth on the object system: Layer 0 core (`show`/`equals?`/`clone`),
Layer 1 collections, Layer 2 numerics, Layer 3 text & streams. Files
(Layer 4) and GUI/events (Layer 5) are designed, not yet shipped. This
is the "rich protocol library" the project is now leaning into — and
the reason the full Factor image stays.

**Extensions.** LET (an infix-algebra DSL with its own sub-lexer),
managed strings (the GC'd, immutable `$-suffix` vocab), character
literals (`'a'` + escapes), and a persistent REPL modeled on Factor's
listener — definitions *and* data-stack values survive across evals.

## The IDE / runtime

A native Direct2D / DirectWrite MDI IDE (`factorforth-ui.exe`,
Windows-only): Forth console, editor, data-stack viewer, and a doc-pane
(DocCrate's markdown renderer hosted as a pane type). Graphics reach the
screen via `gpane-*` FFI primitives that queue onto the GUI thread —
Factor never touches Direct2D directly.

## Working principles

- **Correctness gates performance.** The ANS suite passing under the
  Factor build is the hard gate; perf is a later trajectory.
- **Thin desugars over Factor's machinery.** The IR we emit gets
  progressively more idiomatic-Factor-shaped; we don't fight the
  optimizer or reimplement what Factor already does well.
- **Docs are part of the record, and must match the code.** Verify
  claims against `builtin_table` + the `.f` files (and `--dump=ir`),
  not against comments.

— living document; update as the shape changes.
