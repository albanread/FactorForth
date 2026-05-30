# 2026-05-30 — locals, all the way down

A day that started about *name mangling* and ended with a fully
re-entrant standard library, a Forth-2012 locals system, an
extension marker that we shipped with docs, and a number-parsing
bridge over Factor. Most of it was driven by one question the user
asked at about midday:

> "I am concerned about all the VALUES we use VALUES are global
> variables. what if methods are used in a re-entrant way???"

He was right. The collection algorithms in `lib/collections.f` —
`each`, `map`, `filter`, `fold`, `tally`, `find`, the whole kit —
were all written with the module-level `VALUE` pattern:

```forth
0 VALUE foo-c
0 VALUE foo-xt
: foo ( c xt -- ... )
    TO foo-xt  TO foo-c
    ... foo-c size 0 ?do ... loop ;
```

Which was *fine* until a user-defined `cmp` method recursively
called `sort` on another collection from inside the loop body of an
outer `sort`. Then `srt-c`, `srt-key`, `srt-j` would get clobbered
by the inner activation and the outer would unwind into garbage.
Hidden, intermittent, fatal under exactly the kind of compositional
use the protocols are *meant* to encourage.

So we built locals, and refactored *everything*.

## Mangling first

Before the big refactor, the morning got eaten by name mangling
(`c00f381`, `aa72edb`, `fb39636`). Every user word now compiles
through a `z-` prefix so the ANS surface can't ever collide with
Factor's vocabulary — `dup` the user word and `kernel:dup` the
Factor word coexist via `z-dup` in the IR. `WORDS` strips the
prefix on the way out, so the user never sees a `z-` in their face;
that's strictly for Factor's benefit. There was one bug worth
remembering: the explicit `b:object` specializer was being
mangled to `z-object`, which broke multi-method dispatch on the
catch-all. The fix in `fb39636` is small but the lesson is bigger
— *explicit* class names in method specializers stay raw, only
user-coined names get prefixed.

## The ordering protocol

Sprint-style: `cmp ( a b -- n )` lands as Layer 0's third generic
(`94cdc96`). Then `before?`, `after?`, `lesser`, `greater` derive
from it; then `sorted?`, `sort`, `min-of`, `max-of` derive from
*them*. Five new generics; the rest is layered. The naming detail
worth keeping: it's `cmp`, not `compare`, because Factor's
`math.order:compare` is in scope and we don't want to shadow it
without consent.

ASCII character predicates and case-flip (`1491cd4`) — `char-upper?`
`char-lower?`, `letter-char?`, `digit-char?`, `whitespace-char?`,
`upcase-char`, `downcase-char`. One-liners over code ranges. Boring
to type, ubiquitous in use.

Set algebra and dict iteration (`c0718da`), enriched Layer 1
conveniences (`956e162`) — `first`, `last`, `empty?`, `take`,
`skip`, `concat`, `partition`, `reduce`, `each-index`, `map-index`,
`set-union`, `set-intersect`, `set-difference`, `subset?`,
`dict-each`. Forty-odd words; the library went from "you can survive
with this" to "you can compose with this." Every one of them was
written with `VALUE` scratch.

Which is when the user noticed.

## Locals

The Forth-2012 standard already specifies `{: name1 name2 :}` as a
head-of-body lexical-binding form. We didn't have it. Implementing
it (`8a374ef`, then mid-body blocks in `5e3e48b`) was mostly
plumbing: a new `Expr::Locals` variant, a tiny parser arm, a
`Target::Local` resolver target, an effect-inference rule, and a
lowering pass that emits Factor's `::` form for head locals and
chained `:>` for mid-body blocks. Each lowering is two or three
lines; the work was making sure every other phase agreed.

One thing I almost did and the user stopped me cold: I was about to
recommend Factor's `:: name ( a b -- ) ... ;` syntax to users. He
replied:

> "remember the user is not meant to see factor that syntax is not
> 4th!"

The hard line stayed: ANS Forth at the surface, Factor only as the
compilation target, *never* leaks. So `{: :}` it is — actually
standard, actually portable to other Forth-2012 implementations,
actually what a Forth user expects. And it lowers internally to
`::`, which the user doesn't see.

The diag test for locals shadowing was instructive: a `{: dup :}`
inside a body shadows kernel `dup` for the rest of that body,
without affecting `dup` outside. It works because the resolver
threads a per-definition locals scope through expression resolution
and the locals lookup happens first. Three tests, all green.

## The refactor

Then the *real* work — converting every algorithm in
`collections.f` from VALUEs to locals. Five batches:

| commit  | batch | what                                                              |
|---------|-------|-------------------------------------------------------------------|
| 731df9f | A     | `each` / `map` / `filter` / `fold`                                |
| b2ac7db | B     | `tally` / `any?` / `all?` / `find` / `member?` / `index-of`       |
| 7bd7e68 | C     | `sorted?` / `sort` / `reverse` (sort is the gnarly one)           |
| 8efcb01 | D     | `each-index` / `map-index` / `reduce` / `partition` / `take` / `skip` / `concat` |
| 24821c6 | E     | `set-union` / `set-intersect` / `set-difference` / `subset?` / `set-each` / `dict-each` |

Plus `b1185b1` for `streams.f`'s `split` and `join`, the other
two re-entrancy holes.

The pattern across all of them was identical: capture the
arguments as locals at entry, keep per-iteration accumulators on
the data stack, eliminate every module-scope VALUE. By Batch E,
not a single `VALUE` or `VARIABLE` remained in `collections.f` or
`streams.f` except in `lib/othello.f`, which is a single-player
game fixture, not a library.

Batch B had a moment that briefly looked alarming — a
`STATUS_STACK_BUFFER_OVERRUN (0xc0000409)` crash from the test
process. I ran each test individually expecting to isolate a real
stack-effect bug; every one of them passed. The next sweep also
passed clean. It was an intermittent VM teardown thing, not the
refactor. (Worth keeping in mind: Factor's process exit through our
embedded DLL occasionally drops a signal. Doesn't repeat reliably.
Cargo reports it as exit 1 even when every test logged "ok".)

Sort was the test of the design. Insertion sort needs a *mutable*
cursor `j` that walks down the array. Locals are immutable in our
lowering (and in Factor's `::`). The clean answer: factor the inner
shift loop into its own word `insert-at-i ( c i -- )` that takes
`c`, `i`, and `key` as locals, and keeps `j` on the data stack
through `begin/while/repeat`. Each call to `insert-at-i` gets its
own fresh locals frame, so even a `cmp` method that recursively
sorts another collection during sort is safe. The outer `sort`
shrinks to one line:

```forth
: sort ( c -- ) {: c :}
    c size 1 ?do  c i insert-at-i  loop ;
```

This is the kind of refactor that makes you ask why it was ever
hard. The answer is that it wasn't hard once `{: :}` existed; it
was *impossible* before.

## The underscore extension

When everything was green I floated the idea, and the user pushed:

> "I wonder if it would also be useful to use _ for ignore this
> item in locals? it would be a useful extension we can document
> and express intent leave _ on stack. it may be too complex,
> discuss"

We discussed. Two interpretations: A, "consume and discard" (Rust's
`let _ =`); B, "leave on stack." I made the case that A is a
small clean extension and B is a sharp-edged feature that hides
non-pops in the locals declaration when the better answer is just
"don't declare it." He agreed. We shipped A (`fe42cab`).

The implementation is satisfyingly small. Parser already accepts
`_` as a token. The resolver excludes `_` from the locals lookup
scope, so a user reference to `_` fails with the normal "undefined
word" error — exactly right. Emit rewrites each `_` to a fresh
`_dN` placeholder via a process-local atomic counter, so multiple
`_`s in one block don't collide. Three new tests, including a
negative test that user code *cannot* reference `_`. Total
diff: ~30 lines of compiler, plus 60 lines of docs.

The docs page (`release/factorforth/docs/locals.md`) is the first
dedicated locals documentation, covering head + mid-body forms,
the lowering table, the re-entrancy rationale (with a worked
"before/after" of the VALUE pattern), and the `_` extension's
*and-what-it-isn't* section. The user named the principle:

> "I would like you to add to our journal, please do that, casual
> style"

So this entry is also part of that — making sure the *why* travels
with the code.

## METHOD: bodies, and the helper-word trick

Almost shipped, almost stopped. Then I noticed the three obvious
catch-all methods in the library still used the `drop`-the-receiver
idiom:

```forth
METHOD: show ( x:object -- ) drop ." <object>" ;
METHOD: new-like ( d:darray -- e ) drop <rawvec> <darray> ;
METHOD: new-like ( s:string -- d ) drop new-darray <string> ;
```

The `_` extension was *born* for these. I rewrote them, ran tests,
and... 35 failures. The METHOD: body parser didn't accept `{:`.
Added the arm. 35 *different* failures: `Undefined word: z-show-{
object }`. Bizarre.

The cause was structural: `multi-methods:METHOD:` is a Factor
parsing word and does *not*, by itself, open a `::` locals scope
for `:>` to bind into. Mid-body locals in `:` defs work because the
whole def is wrapped in `::`, so the locals context exists. Inside
a `multi-methods:METHOD:` body, `:>` has nothing to bind into and
Factor's recovery makes a confused mess of the rest of the line.

The fix I landed (`93cf8f1`) routes any method-with-locals through
a freshly-generated `::` helper word:

```factor
:: z-show-mh1 ( _d1 -- ) "<object>" print-string ;
multi-methods:METHOD: z-show { object } z-show-mh1 ;
```

The helper name (`-mh1`, `-mh2`, …) comes from a per-process
counter. Users never see it; `SEE show` still shows the original
source. `MethodDef` gained a `locals: Vec<LocalDecl>` field, the
parser captures head locals between effect and body just like
colon defs, and the emit grew a branch that wraps. ~80 lines total,
clean three catch-alls, all 96 tests across 10 suites green.

The reason this matters past the catch-alls is that *any* method
with a complex body now gets the same locals power that colon defs
do. Multi-method dispatch fixes the stack effect for you; locals
let you ignore or rename the args you don't care about, without
the noisy `drop` / `dup` / `swap` boilerplate.

## Text, then numbers

With locals in place I wanted to actually *use* them on something
user-facing. Strings already implemented `size` and `at` but not
`new-like` and `at!`, so half the collection algorithms didn't
reach them. Two-line fix (`364b3ab` Part 1) made string a
fully-fledged collection — `' upcase-char map` over a string now
returns a string, not a darray of char codes.

On top of that, the missing text utilities every user reaches for:
`subseq`, `upcase-string`, `downcase-string`, `trim-left` /
`trim-right` / `trim`, `starts-with?` / `ends-with?` / `contains?`
(all built on one `substring-at?` primitive), `pad-left` /
`pad-right`, `repeat-char` / `repeat-string`. Six new diag tests,
all re-entrant by construction.

There was one small gotcha I want to mark for next time. My first
draft of `skip-ws-left` declared `{: s :}` for input `( s i -- )` —
and `{: s :}` with *one* name binds the **top** of the stack,
which is `i`, not the deeper `s`. The body then read garbage. The
effect-error from the inferencer caught it; the fix was `{: s i :}`
then push `i` back onto the stack to walk it. Worth a memory entry.

Then numbers (`05729d6`). Two runtime bridges (`nf-num>str-chars`,
`nf-str>num`) over Factor's `number>string` and `string>number`,
plus Forth-side wrappers `n>string` and `s>n` that speak our
`string` class. The `s>n` two-return shape — `( n -1 )` on
success, `( 0 0 )` on failure — means a successfully parsed `"0"`
is not confused with "couldn't parse." Test the flag, never the
value.

Composability paid off immediately: `42 n>string 8 '0' pad-left
show` → `00000042`. Number formatting is just text manipulation
once `n>string` exists.

## Stats

  - re-entrancy refactor: 5 batches in `collections.f` (A–E),
    plus `streams.f` `split`/`join`; not a VALUE remains outside
    the othello fixture
  - Forth-2012 `{: :}` locals: head + mid-body, both `:` and
    `METHOD:` bodies, `_` discard extension
  - 3 catch-all methods rewritten with `{: _ :}`
  - 22 new text utility / locals / conversion words in the lib
  - tests: 35 coreprotocols, 15 streams, 8 locals, 6 classes, 8
    numerics, 4 ordering, 7 method-combinations, 4 multi-dispatch,
    4 let-methods, 6 typeof, 5 value-to — all green
  - commits: 18 today, from `94cdc96` (ordering) to `05729d6`
    (number conversion)
  - docs: new `release/factorforth/docs/locals.md`; `streams.md`
    gained a "Number ↔ string" section

## Reflection

The pattern I want to remember: a re-entrancy concern raised by
the user → locals as the structural answer → mechanical refactor of
five batches → a small extension (the `_` marker) that fell out
naturally → propagation of the extension into `METHOD:` bodies →
text utilities that *use* the now-strong substrate → number
conversion that *uses* the text utilities. Each step shorter than
the last, because each previous step put more leverage in the box.

The user's framing of his own concern — "what if methods are used
in a re-entrant way???" — wasn't just a bug report. It was the
right question at the right time, because the protocol library was
about to get bigger and the bug would have hidden itself
indefinitely behind the very compositional patterns we're
encouraging. Catching it *now*, before users hit it, means the
library ships safe.

The other note: he held the ANS-Forth line hard. Twice today I was
about to leak Factor surface into user docs or tests, and both
times he caught it. "Remember the user is not meant to see factor
that syntax is not 4th!" That's the kind of correction that keeps
a project honest. Factor is our compilation target; ANS Forth is
the language. They are not the same, even when the lowering is
trivial, and treating them as if they were would erode the
*reason* this project exists.

Quiet day ending, three docs richer, every test green. Onward.

— end of day, 2026-05-30
