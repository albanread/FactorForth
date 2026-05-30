# Factor4th Documentation

ANS Forth on Factor's VM, in a Direct2D IDE.

## Start here

- [Getting started](getting-started.md) — install, first session,
  what the panes do
- [Forth tutorial](forth-tutorial.md) — learn the language from
  scratch if you've never used Forth before
- [IDE guide](ide-guide.md) — every menu, every keyboard shortcut,
  every pane

## Language

- [Language reference](language-reference.md) — every word
  Factor4th ships, organised by topic
- [Stack effects](stack-effects.md) — what the `( a b -- c )`
  notation means and how the compiler uses it
- [Locals](locals.md) — Forth-2012 `{: a b :}` lexical bindings,
  plus the `_` anonymous-discard extension
- [Classes and methods](classes.md) — CLASS:, SLOT:, GENERIC:,
  METHOD:, polymorphic slots, two setter idioms
- [CoreProtocols](coreprotocols.md) — the CLOS object model and the
  standard library design, with the layer map and diagrams
- [Core protocol](core.md) — Layer 0: `show`, `equals?`, `clone` —
  the root protocol every class opts into
- [Collections](collections.md) — Layer 1: the collection protocol
  (grid, darray, dict, set) and the each/map/filter/fold algorithms
- [Numerics](numerics.md) — Layer 2: vec2 and complex, a shared
  arithmetic protocol written in LET
- [Text & streams](streams.md) — Layer 3: the `string` value type
  and the stream protocol (eof-as-object), plus split/join/read-line
- [LET algebra](let-algebra.md) — the infix DSL for math-heavy
  code
- [Managed strings](managed-strings.md) — the `$-suffix` vocab
  for string handling that doesn't use raw memory

## How it works

- [Architecture](architecture.md) — the compiler pipeline,
  Factor's role, how the IDE talks to the VM
- [Embedding Factor](embedding.md) — why we use Factor as a
  back-end, how the patched VM differs from stock

## Reference

- [License](license.md) — BSD-3-Clause, third-party credits
- [Release notes](release-notes.md) — what shipped in each version

## Project

- Source: <https://github.com/yourname/Factor4th> (placeholder)
- Sibling projects: WF64 (64-bit STC Forth), NewCormanLisp,
  NewOpenDylan, NewAudio
