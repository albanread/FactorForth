# 2026-05-28 — Programming-Tools word set (.S / WORDS / DUMP)

The user noticed the Programming-Tools wordset was thin: "I feel
like some ANS tools are missing, do we have DUMP WORDS etc."  And
a sharp design instinct about DUMP: "We need to make DUMP make
sense, DUMP data about the item on the stack rather than dump an
address ... a hex/ascii dump may be appropriate or just a type
and value report."

Three words landed.

## `.S` — non-destructive stack print

gforth-style: `<depth> a b c`.  Walks `get-datastack`, prints the
count in angle brackets then each value, and leaves the stack
untouched.  The classic "what's on my stack right now" REPL aid.

```
1 2 3 .s          =>  <3> 1 2 3
```

## `WORDS` — list user definitions

Lists the names in the `scratchpad` vocab, which is where our `:`
definitions land.  Not the whole Factor dictionary (thousands of
words, useless) — just what the user has defined this session.

## `DUMP` — re-imagined for our value model

ANS `DUMP ( addr u -- )` hex-dumps a span of raw memory.  Against
our value model that's meaningless: our "addresses" are opaque
`nf-addr` tuples (a byte-array + offset), so dumping one as a
pointer would print Factor internals, not the user's data.

So `DUMP ( x -- x )` inspects the VALUE on top of the stack:

  - prints a type tag (`INT` / `FLOAT` / `STRING` / `XT` / `ADDR`
    / `OTHER`)
  - then the value: decimal + hex for ints, the number for
    floats, the text for strings
  - and for strings and nf-addrs, a classic 16-byte hex+ASCII
    dump of the backing bytes

It's non-destructive — leaves `x` in place — so you can drop it
into a pipeline as a debugging tap without disturbing the stack.

```
s" Hi!" drop dump     =>  ADDR  3 bytes
                          0000  48 69 21   Hi!
255 dump              =>  INT  255  (hex ff)
```

## The debugging saga: `bl` is a constant, not a word

Getting `.S` working took an embarrassing amount of bisection,
and the lesson is worth recording.  `.S` iterates the stack with
`[ (nf-pp1) " " write ] each` — but my first cut used Factor's
`bl` to write the separating space:

```factor
[ (nf-pp1) bl ] each      ! WRONG
```

Every run died with `unbalanced-branches-error`, and I chased it
through `cond`, `>base`, `inline`-vs-not, type narrowing — every
plausible inference culprit — for a dozen rebuilds.  None of them
was it.

The actual cause: **our `forth.runtime` vocab defines `bl` as the
ANS ASCII-space CONSTANT** (`( -- 32 )`), shadowing Factor's
`io:bl` (which writes a space, `( -- )`).  Because `forth.runtime`
sits late in the `USING:` list, `bl` resolved to the constant.
So the quotation `[ (nf-pp1) bl ]` had effect `( x -- 32 )` — it
left an extra integer on the stack each iteration — and `each`,
expecting `( x -- )`, reported the mismatch as "unbalanced
branches."

The fix was one token: `bl` → `" " write`.

Two lessons:
  1. ANS Forth and Factor disagree on `bl`.  In ANS it's the
     blank *character constant* (32).  In Factor it's a *word*
     that emits a space.  We (correctly) implement the ANS
     meaning, which means inside our own `forth.runtime` source
     `bl` is a constant — never reach for it expecting the Factor
     behaviour.
  2. "unbalanced-branches-error" is Factor's generic complaint
     when a combinator's quotation has the wrong net effect.  It
     does NOT necessarily mean a literal `if`/`cond` imbalance —
     here it meant a stray value on the stack.  When you see it,
     check the quotation's actual stack effect first, not the
     branch structure.

## A real find along the way: the slim image has no prettyprint

The first instinct was to print values with Factor's `pprint`.
That fails at runtime with "Generic dispatch failure (no method)"
because our slim bootstrap image doesn't carry the full
prettyprint method suite.  Our existing `.` sidesteps this by
using `number-base get >base write` (from `math.parser`, which IS
in the image).  The tools follow the same rule: `>base` for
integers, `number>string` for floats, raw `write` for strings,
and a class-name placeholder for anything exotic.  No prettyprint
dependency.

## What changed

- **`src/session.rs`** — new `TOOLS_SETUP_SRC` (a raw Factor
  string, no format! brace-escaping) evaluated as a second boot
  step.  Defines `nf-hexdump`, `nf-dump`, `nf-.s`, `nf-words` and
  helpers in `forth.runtime`.  Boot-defined (like the type
  introspection helpers) so we can iterate without an image
  rebuild.
- **`src/compiler/resolve.rs`** — `.s` / `words` / `dump` mapped
  to the `nf-*` words.
- **`src/compiler/effect.rs`** — effects registered: `.s` (0,0),
  `words` (0,0), `dump` (1,1).
- **`tests/diag_tools.rs`** — 5 tests: `.s` non-destructive,
  dump int / float / addr-hex-ascii, words-lists-user-defs.

## Stats

  - 89 → 94 runtime/diag tests (+5)
  - 127 lib unit tests, green
  - No image rebuild (boot-defined)

— end of programming-tools entry
