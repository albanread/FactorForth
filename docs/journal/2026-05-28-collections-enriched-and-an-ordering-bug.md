# 2026-05-28 — Enriching the collection layer, NEEDS, and an ordering bug worth catching

A long, good day. It began with the collection protocol already
standing — `grid`, `darray`, `size`/`at`/`at!`, and `each` — and ended
with a real sequencing bug found by the user's intuition and fixed at
the root. The throughline was a principle the user put plainly:

> "lets stay and extend before moving up, each layer above is enriched
> by the layer below."

So we stayed in Layer 1 and made it rich, rather than racing up to
numerics or GUI. Every word we added paid forward.

## The functional core, completed

The classic trio became a quartet. `filter` joined `each`/`map`, then
`fold` — the general reducer the others are special cases of:

```forth
xs 0   ' + fold .   \ 10   sum
xs 100 ' - fold .   \ 90   left-to-right: ((((100-1)-2)-3)-4)
```

`fold`'s two-in/one-out shape needed a new effect-annotated call
primitive, `call2>` (`call( a b -- y )`), mirroring `call1`/`call1>`.
The pattern that keeps higher-order words inferable inside Factor's
strict loops is now a small, understood family.

### Type-preserving `map` via `new-like`

The nicest piece. `map` now returns a collection of the *input's* type
— a grid maps to a grid, a darray to a darray — so 2-D structure
survives a transform. Rather than special-case each backing inside
`map`, one generic carries the knowledge:

```forth
GENERIC: new-like ( c -- d )   \ fresh, shaped, empty collection of c's type
```

`map` builds `c new-like` and fills by linear index. This is
CoreProtocols' answer to Factor's `like`, expressed as a protocol any
class joins by implementing one word. Two facts I checked rather than
assumed: the grid `new-like` method uses only boa constructors and
accessors (dodging the METHOD forward-ref limit), and a darray's `at!`
is Factor's `set-nth` on a growable, which I confirmed calls `ensure`
first — so a fresh empty darray grows correctly as indices are written.

### Search, predicates, and equality

Then `tally` / `any?` / `all?` / `find`, plus `sum`/`product`. `find`
returns two values — the element *and* a found flag — so `0` is a valid
element, never a sentinel. (When I first wrote the test I asserted the
wrong number; the code was right, I was wrong — the first even in
`1..6` is 2, not 4. Fixed the test to match reality, not the reverse.)

The cleanest demonstration of "the layer below enriches the one above":
`equals?` landed in **Layer 0** as an overridable generic, and Layer 1's
`member?`/`index-of` dispatch through it — so a class with its own
equality is searched on its own terms, for free. A test proves an
`account` whose `equals?` compares only an id slot makes `member?` treat
a same-id record as present.

## NEEDS — and Rust driving it

The user asked for the Forth `NEEDS` (include-once), then made the
sharp observation that reframed the whole implementation:

> "as a parsing word rust could literally drive it"

Exactly. Because `NEEDS` is parsed in our front end, Rust resolves it
entirely at **compile time** — reading, parsing, and *splicing* the
file's AST into the current module the first time it's seen, expanding
to nothing on repeats (dedup in `CompileContext`). Composing at the AST
(parse each file, concatenate into one module, lower once) means an
included file's definitions are part of the *same* compilation unit, so
code after the `NEEDS` can use them — something the runtime `INCLUDED`
fundamentally can't offer. Nested includes resolve relative to the
including file; diamonds collapse to one load; cycles terminate.

The one trap I sidestepped: spliced items carry byte-offsets into a
*different* file, so SEE docs are now built per-file against each file's
own source — never slicing the current source at foreign offsets.

## The ordering bug — "a stack of values meeting a sequence of code"

The day's best moment was diagnostic. The user noticed output running
out of order and reasoned aloud:

> "it is really quite necessary for code to execute in the order the
> user expects."
> "More likely a stack of values meeting a sequence of code."

That was the bug, precisely. `VALUE` greedily captures the pending run
of top-level expressions as its initializer (correct ANS behaviour —
it binds whatever's on the stack). But `emit` ran that initializer's
*execution* in the up-front **definitions phase**, so any side effect
swallowed into a VALUE body — a `." ..."` that happened to precede it —
fired ahead of *all* top-level code. Forth is sequential; this was a
genuine miscompile, not cosmetic.

The fix splits VALUE emission by what it *is* versus what it *does*: the
`SYMBOL` handle and reader word stay hoisted (pure declarations); the
`set-global` that *runs* the initializer moves to the execution phase,
at the VALUE's own source position. A regression guard fails pre-fix and
passes after. The protocol test that printed `same=` first now prints
`n1= n2= same= in1= in2=` in order.

A full `cargo test` sweep — run for the first time in a while — also
surfaced a long-pre-existing doctest papercut in `effect.rs` (rustdoc
trying to compile a prose example as Rust). Confirmed it failed at HEAD
with my changes stashed, then fixed it in its own commit. The sweep is
fully green now.

## Stats

  - Layer 1 words added: `filter`, `fold`, type-preserving `map`
    (+`new-like`), `tally`, `any?`, `all?`, `find`, `sum`, `product`,
    `member?`, `index-of`
  - Layer 0: `equals?` generic (overridable; default = structural `=`)
  - `call2>` boot primitive
  - `NEEDS` — compile-time include-once (AST splice, dedup in
    CompileContext, relative resolution); `Item::Needs` + `expand_needs`
  - emit fix: VALUE initializers run in source order
  - docs: collections.md reference (every shipping word + examples,
    verified via `--testsnap`); language-reference + design updated
  - tests: protocol suite 19, NEEDS 5, ordering 2, all green; 127 lib;
    full `cargo test` sweep clean
  - commits: 08390e5 (fold), 2f32c4f (type-preserving map), 6f5545d
    (collections docs), 8c7b34e (search/predicate), 988bbc4 (equals? +
    member?/index-of), ce1ff57 (NEEDS), 3a5a10d (ordering fix), 2878ef2
    (doctest)

## Reflection

The collection layer is now genuinely rich — a small algebra of
algorithms written *once* against `size`/`at`, working on any backing,
with equality and search threaded through an open protocol. NEEDS means
the library can finally declare its own dependencies the Forth way. And
the ordering bug is the kind I'm most glad to fix: not a crash, but a
quiet wrong answer that would have bitten any program mixing top-level
side effects with a nearby `VALUE`. The user found it by intuition and
named the mechanism before I'd read the code — the best kind of pairing.

To the standing thanks: it's a pleasure to build this. Factor4th keeps
its line clean — the user writes Forth, never Factor; we steal the
substrate and hide it completely — and every day it reads a little more
like the manual I'd want to hand someone.

— end of day, 2026-05-28
