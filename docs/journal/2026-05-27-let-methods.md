# 2026-05-27 — LET-methods: destructure at the binding list

Eighth journal entry today.  LET methods shipped — class
instances destructured at the LET binding, body stays in pure
infix math.  The user's design objection ("`obj.slot` is
natural everywhere except here, and not natural anywhere else
in this compiler") was the driver of the design and is
honoured.

## What you can write now

```forth
CLASS: point  SLOT: x  SLOT: y  ;

METHOD: distance ( a:point b:point -- d )
    LET ( a:point as ax ay, b:point as bx by ) -> ( d ) =
        sqrt((bx - ax)^2 + (by - ay)^2)
    END ;

0.0e 0.0e <point>  3.0e 4.0e <point>  distance .   \ 5.0
```

The `a:point as ax ay` clause:
  - Asserts the top stack item is a `point`, bound as local `a`
  - Exposes `a`'s slot-0 under the local name `ax`, slot-1 as `ay`
  - Slots are position-paired against the class's declared slot
    list — `as ax ay` works because `point` declared `SLOT: x
    SLOT: y` in that order
  - Two destructured points get distinct local names per slot
    (`ax`/`ay` vs `bx`/`by`) — no collision

The body is unchanged LET syntax: infix math, function calls,
`^` for power, `sqrt(...)` etc.  No dots, no operator-bearing
identifiers, no syntactic island inside the expression
grammar.

## The design rule

> Whitespace delimits words.  Nothing in our compiler ever
> splits inside an identifier.  `?dup`, `>>`, `point.x!` are all
> single tokens — the dot in `point.x!` is part of the word
> name, not a slot-access operator.

The dot-objection was: introducing `a.x` in LET would have been
the *only* place where we split within a word.  Even though
LET is already a syntactic island (infix instead of postfix), it
respects this rule — its tokens are whitespace-delimited too.

The destructure-at-binding approach preserves the rule:
  - The LET binding list is where you NAME things from the stack
  - Destructuring is just a richer form of naming
  - The body sees plain locals, no special syntax needed

Same architectural shape that's been compounding across the
project: a single syntactic invariant, applied consistently,
with every new feature finding a way to live inside it.

## Pieces of the implementation

About 100 lines across three files:

**`compiler/let_lang/parser.rs`** (~50 lines)
  - `LetInput` struct with optional `DestructureClause { class, slots }`
  - New tokens: `Colon` and `AsKw`
  - `input_list()` parser: per-input optional `:class as slot...`
  - Commas separate top-level inputs when destructuring is
    present (otherwise the `as` slot list has no terminator)
  - **Bug fix found in the process**: LET's `skip_ws` was
    treating `( ... )` with inner space as a Forth-style block
    comment, silently eating the input list.  Removed.  All
    existing tests passed because they coincidentally used
    `(a b)` with no inner space; any user typing natural
    `( a b )` would have hit the same wall.
  - **Bonus**: added `^` as alias for `**` (math users reach
    for `^` first; both produce the same Pow node).

**`compiler/let_lang/codegen.rs`** (~30 lines)
  - `lower_to_factor` now takes `&HashMap<String, Vec<String>>`
    for class slot resolution
  - Each destructure clause emits `name class>actualslot :>
    alias` for each (alias, actual-slot) pair
  - Position-paired: user's i-th alias maps to class's i-th slot
  - Errors at codegen if alias count exceeds class slot count

**`compiler/emit.rs`** (1 line)
  - Threads `&r.class_slots` (from Sema) into the LET call

## Output IR

A LET-method body lowers to a clean Factor `[| ... |` locals
block:

```factor
: distance ( a b -- d )
    [| nfl-a nfl-b |
        nfl-a point>x :> nfl-ax
        nfl-a point>y :> nfl-ay
        nfl-b point>x :> nfl-bx
        nfl-b point>y :> nfl-by
        nfl-bx nfl-ax math:- 2.0 math.functions:^
        nfl-by nfl-ay math:- 2.0 math.functions:^
        math:+
        math.functions:sqrt
    ] call( nfl-a nfl-b -- nfl-d ) ;
```

The `:>` lines are the destructure unpacking.  Body is the same
infix-to-postfix translation LET already did, just with the
destructured locals available.  All inlined by Factor's JIT —
same speed as the hand-rolled stack-juggle would have been.

## Two unexpected fixes that came along

**Block-comment / list ambiguity.**  LET's `skip_ws` recognised
`( ... )` Forth-style block comments.  But LET ALSO uses `(...)`
for its input and output lists.  When a user wrote `LET ( a b )
->` with spaces, the lexer ate everything from `(` to `)` as a
comment and the parser then saw `LET -> = ... END` and reported
"expected LParen, got Arrow".  The error was confusing and the
existing tests had been masking the bug for months.  Fix:
removed block-comment recognition from LET.  Users can still use
`\` line comments inside a LET block.

**`^` as power operator.**  Math users will reach for `^` before
`**`.  Added as a lexer alias.  Backwards compatible.

Both bugs were caught during this session by trying to write
realistic LET source and watching things explode.  Same probe-
test pattern that's been compounding.

## Test sweep

  - 17 LET parser unit tests (was 12, +5 for destructure +
    space-handling + runtime-shape regression)
  - 4 new LET-method runtime tests through the embedded VM:
    distance, mixed inputs, in-a-METHOD, plain-LET regression
  - 80 runtime tests total, all green
  - 122 lib unit tests, all green
  - Release binary 2.04 MB

## What's left

The user has access to a CLOS-flavoured object system with
polymorphic slots, a chainable and an ANS-style setter per slot,
cross-eval class persistence, single-dispatch generic methods,
AND now a LET-methods syntax that reads as algebraic math.

Remaining sprint-2 items on task #64:
  - Multi-method dispatch (GENERIC#: arity-N)
  - METHOD-BEFORE: / METHOD-AFTER: / :around combinations
  - SUPER: / call-next-method
  - Slot initial values
  - Per-class TYPEOF codes + CLASS-OF

None of those block real programs from being written today.

## Architectural reflection (still compounding)

This session has been an unusually clean instance of the
through-line.  The user posed a real design question (LET-methods
should exist), I proposed dot-notation, the user pushed back
("not consistent with the rest of the compiler"), I retreated
to a different design (destructure-at-binding) that aligns with
the existing rules, we shipped it.

The rule "whitespace delimits words; nothing splits inside an
identifier" is now load-bearing for the language's feel.  It
ruled out a feature I'd have happily implemented.  It's pulling
its weight.

— end of let-methods entry
