# M2.x #43 — managed strings ($-vocab) ported from WF64 (2026-05-24)

WF64's `$`-suffix managed-string library is now available in
NewFactor, backed by Factor's native `string` type instead of
WF64's hand-rolled GC-tagged String / MutStringBuilder objects.
The user's framing was correct: **forth strings suck**, and the
$-vocab is the modern replacement.

## Shipped surface

```text
S$" literal"     compile-time string literal  (parse-side wired)
$len $clen       length (byte / codepoint — same on Factor strings)
$+               concatenation
$upper $lower    case conversion (Unicode-aware)
$find            substring search; -1 on miss
$contains?       substring presence; ANS -1/0
$starts? $ends?  prefix / suffix check; ANS -1/0
$slice           ( s from len -- s' ) substring extraction
$cmp             lex compare; -1 / 0 / 1
$hash            hashcode (Factor-VM-stable)
$. $.cr          print (with/without newline)
int>$ $>int      number ↔ string
>$ $>addr        bridge to legacy (c-addr u) ANS pairs
```

## Why this port was small (vs. WF64's ~1900 LoC)

WF64 had to hand-roll everything: GC-tagged String + MutStringBuilder
heap objects, allocator, hash-table integration, FNV-64 hash, UTF-8
validation, all the asm-level slot accessors. **All ~1900 lines
were substrate.**

Our port leverages Factor's existing infrastructure:
- `string` type is GC-tracked, immutable, Unicode (codepoints) by
  default
- `length`, `append`, `subseq`, `head?`, `tail?`, `subseq-index`,
  `<=>`, `hashcode`, `>upper`, `>lower` all already exist
- `byte-arrays` + `io.encodings.utf8` give us the bridge to / from
  legacy `(c-addr u)` representation

Result: **~80 LoC of Factor wrappers + ~60 LoC of Rust resolver/
effect/emit changes.** Total ~140 LoC, vs WF64's ~1900. Same
user-visible surface, same lifetime guarantees.

## The three bugs we hit (good ones)

### 1. Lexer treated `$` as a hex-prefix unconditionally

`$gg` had been declared a malformed hex literal, with the lexer
erroring out before the parser saw it. With the `$`-vocab landing,
`$.`, `$+`, `$len`, etc. all start with `$` and need to be valid
word tokens.

Fix mirrored what `#` already did (`#42` is decimal, `#S`/`#>`
are words). Only treat `$<...>` as a hex literal when the tail
is entirely hex-digit characters:

```rust
if let Some(rest) = raw.strip_prefix('$') {
    if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return parse_int_with_base(rest, 16, NumBase::Hex, raw, span);
    }
}
```

Updated the lexer unit-test to match the new behaviour
(`$gg` → `Word("$gg")`, not a malformed-number error).

### 2. `$find` had a stray `swap` and a broken fallback

Original buggy version:

```factor
: $find ( haystack needle -- index )
    swap subseq-index dup [ drop -1 ] unless ;
```

Two bugs:
- `subseq-index` is `( seq subseq -- i/f )` — already the
  order we have. The `swap` inverted the args.
- `dup [ drop -1 ] unless` runs `drop -1` when the input is
  falsy.  But with the dup'd value still under it, the result is
  `( f -1 )` not `( -1 )`.

Fixed:

```factor
: $find ( haystack needle -- index )
    subseq-index dup [ ] [ drop -1 ] if ;
```

### 3. Factor's `<` doesn't work on strings

`$cmp` initially used Factor's `<` as the less-than test. That's
defined in `math.order` for numbers only — calling it on a string
throws a generic-dispatch error.

Use Factor's `before?` instead, which dispatches via `<=>` (the
polymorphic compare returning `+lt+`/`+eq+`/`+gt+` symbols) and
works on strings:

```factor
: $cmp ( a b -- n )
    2dup = [ 2drop 0 ] [ before? -1 1 ? ] if ;
```

This was a useful reminder: **`<` is for math, `before?` is for
order-comparable objects** in Factor.

## Tests

30 new lock-in tests in `tests/session_managed_strings.rs`, all
passing:

- Literal printing
- `$len` byte length (incl. empty string)
- `$+` concatenation (binary and chained)
- `$upper` / `$lower`
- `$find` present, absent, at-zero
- `$slice` (mid-string and to-end)
- `$contains?` / `$starts?` / `$ends?` all polarities
- `$cmp` equal / less / greater
- `$hash` stability within a VM run
- `int>$` / `$>int` round-trip (positive + negative)
- `>$` and `$>addr` bridge to legacy `(c-addr u)`
- Pipeline composition (mirrors WF64 demos/strings.f)
- Strings as args to a user `:`-def

Plus the lexer unit tests covering the new `$`-prefix policy.

## What's deferred to #46 (or its own ticket)

- **`sb-*` builder family** (`sb-new`, `sb-append$`, `sb>string`,
  etc.). Not in this batch — Factor's `sbuf` exists and the
  wrappers would be one-line each, but the test surface area
  doubles and we don't have a current user. Will land when a real
  use-case shows up.
- **`$split`** — Factor has `split` in `splitting`, but the result
  type (sequence-of-strings) needs to be exposed to ANS code as
  something usable. Defer.
- **`$replace`** — Factor has it but with different semantics.
  Wrap when needed.
- **`$trim`** / `$ltrim` / `$rtrim` — trivial wrappers around
  Factor's `trim`. Add when needed.

## Test summary after this milestone

```
session_smoke              5/5  ok
session_io                 5/5  ok
session_floats             3/3  ok
session_quickwins          7/7  ok
session_ans_booleans      19/19 ok
session_ans_core          19/19 ok
session_managed_strings   30/30 ok   (NEW)
─────────────────────────────────────
Session-based total       88/88 ok
smoke_runtime (legacy)    40/40 ok   (run separately, #31)
Grand total              128/128 ok
```

## Files touched

- `src/compiler/lex.rs` — `StringKind::SDollarQuote` variant +
  3-char peek for `S$"`, `$`-prefix policy change, lexer unit
  tests
- `src/compiler/ast.rs` / `parse.rs` / `dump.rs` / `emit.rs` /
  `effect.rs` — exhaustive-match additions for the new variant
- `src/compiler/resolve.rs` — 18 new `$` word entries
- `factor/forth/runtime/runtime.factor` — managed-string vocab
  (~80 LoC of wrappers, USING: of unicode/byte-arrays/utf8)
- `tests/session_managed_strings.rs` — 30 lock-in tests
- `images/nf-mandelbrot.image` — rebuilt

## What's next per the plan

The visible-win sprint is complete. Next per the layered plan:

- **#35 error translation** — half day. Improves test reporting.
- **#33 DEFER/IS** — half day. Needed for vectored ERROR in
  the upcoming test runner.
- **#32 file access** — 1-2 days. Unblocks `INCLUDED`.
- **#41 test runner** — assembles it all.
- **#44 LET** — the heavyweight; standalone.
