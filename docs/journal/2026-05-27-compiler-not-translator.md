# 2026-05-27 — compiler, not translator

The day the front-end shape settled.  Three new desugar passes
shipped, two new ANS surface words landed with cleaner semantics
than the standard mandates, and the image got 7.7× smaller for
distribution.  Underlying all of it: a structural shift from
"FORTH-to-Factor translator" to "compiler with FORTH as the
surface language and Factor as the IR target."  Each ANS sharp
edge is now a self-contained AST pass with unit + runtime tests,
instead of a knot in `emit.rs` or a runtime word fighting Factor's
optimiser.

## What ships today

```
release/factorforth/
├── factorforth-ui.exe        (1.95 MB, GUI subsystem)
├── factor.dll                (222 KB)
├── factorforth.image.zst     (17.5 MB — compressed shippable)
├── factorforth.image         (134 MB — inflated on first run)
├── compress-image.exe        (one-shot tool, dev only)
├── doc-crate.exe             (606 KB)
├── demos/                    (gfx-mandelbrot.f using natural EXIT shape)
└── docs/
```

User download size: **~19 MB** (was 137 MB).  First run inflates
`factorforth.image.zst` → `factorforth.image` in ~1 second, atomic
rename via a `.tmp` sidecar so a crash mid-inflate never leaves a
half-written image.  Subsequent runs read the inflated image
directly with no pause.

## The pipeline

Before today (annotated for what's new):

```
lex → parse → resolve → effect-infer → escape → emit
```

After:

```
lex → parse → expand_templates
          → lower_qdup       ← new (?DUP IF peephole)
          → lower_recurse    ← new (RECURSE self-binding)
          → resolve
          → lower_exit       ← lifted from a prior session into the pass list
          → effect-infer
          → escape
          → emit
```

Three desugars at ~150 lines each.  None of them write IR to
disk; they each rewrite the AST so that emit sees a shape Factor's
`compiler.tree` can JIT without falling off any cliffs.

## The arcs in roughly the order they happened

### 1. The F7 checker review

User had wired `wf64::igui::install_checker` to NewFactor's
compiler on their own branch — F7 in the editor now runs lex +
parse + sema and reports diagnostics inline.  Did a recall-biased
code review; surfaced five findings, two of which mattered:

  - The `if_else_effect` fix (correctly removing the double
    flag-counting that bit IF/ELSE inference) broke three CASE
    tests.  `infer_case_effect` was relying on the old
    "if_else_effect consumes the flag" behaviour to handle the
    flag that `=` produces during arm dispatch.  Cleanest fix:
    make `if_else_effect` pure-branches uniformly, and have the
    CASE call site add its own `flag.then(...)` mirroring what
    the `Expr::If` call site already does.  Added a docstring
    pinning the contract so a future edit doesn't re-split the
    responsibility.
  - F7 used `build_sema(program)` not `build_sema_with_prior`,
    so any word defined in an earlier eval showed as "unknown
    word" in the editor.  Fix below.

### 2. F7 sees session state via EDITOR_SNAPSHOT

Shared `OnceLock<RwLock<CompileContext>>` between the IDE worker
(writer, after each successful compile) and the checker closure
(reader).  Three publish points: at worker boot (empty ctx so F7
has something to lock from the get-go), after every successful
`compile_in_context` in `handle_eval` / `handle_eval_repl`, and
on `ForthRestart` (resets to empty matching the freshly-booted
dictionary).  RwLock means many F7 readers can run concurrently
with at most one writer per eval — negligible contention.

The checker uses `build_sema_with_prior_and_templates` (also
re-exported from `compiler/mod.rs` for this) when the snapshot
is populated, falls back to plain `build_sema` when not.  Editor
now lights up `: greet … ;` followed by `42 greet` in another
pane without false errors.

### 3. lower_qdup — ?DUP IF peephole (#45)

The classic ANS bug.  `?DUP` has effect `( x -- 0 | x x )` —
polymorphic, the number of items it produces depends on the input
value.  Factor's `compiler.tree.propagation` uses SSA-shaped
dataflow and refuses to compile any branch that leaves a
different number of items on the stack.  No Factor-side body
exists that the JIT will accept.

But: ~all real-world Forth code writes `?DUP IF …`, precisely so
the IF can consume the polymorphic top.  At the AST level we can
rewrite the pair into a balanced shape that's semantically
identical:

```
?DUP IF t THEN         →  DUP IF t ELSE DROP THEN
?DUP IF t ELSE e THEN  →  DUP IF t ELSE DROP e THEN
```

Why it's correct:
  - Input `x ≠ 0`: DUP produces `x x`, IF consumes the truthy top,
    runs `t` with `x` still on stack.  Same as ANS `?DUP IF`.
  - Input `0`: DUP produces `0 0`, IF consumes the top falsy `0`,
    runs the ELSE which is `DROP <original-else>`.  DROP removes
    the remaining `0`, leaving the stack as it was before ?DUP.
    Same as ANS `?DUP IF` (which never duplicated the 0).

Bare `?DUP` (not followed by IF) is left alone; resolve then
fails with "unknown word ?dup" since it isn't in builtins.  The
error is the right outcome — standalone ?DUP has no
Factor-compilable shape, and almost certainly indicates a
missing IF in source.

Subtle bug caught during implementation: my first cut synthesised
both `dup` and `drop` WordRefs with the same span (the original
`?dup` token's).  `resolve.rs::word_targets` is keyed by Span, so
the second insertion overwrote the first — **both** AST nodes
emitted as the same word.  Fixed by adding a `synth_span` helper
that fabricates unique spans from an atomic counter (line/col
carried over from the source token, byte_offset picked from
0xFFFF_0000 downward).  Future transforms that synthesise
multiple WordRefs from one source token can reuse the helper.

### 4. lower_recurse — RECURSE self-binding (#58)

In classical threaded Forth, RECURSE is an immediate word
because the dictionary entry for the word being compiled doesn't
become visible until `;` runs.  NewFactor parses whole defs and
could trivially know the enclosing name *if* it were told.  This
pass tells it: every `Expr::WordRef { name: "recurse" }` inside a
`:` body is rewritten to a `WordRef` targeting the definition's
own name.  Resolve then handles it as an ordinary self-call (pass 1
has already registered the def's name in `user_words`).

ANS doesn't formally require a stack-effect annotation on
recursive words, but Factor's strict effect inference *needs*
one — without it the synth falls back to row-vars (`( ..a -- ..b )`)
and Factor refuses to compile.  Surface this upfront with a
`ResolveError::RecurseNeedsEffect` carrying the word name and a
teachable message: "uses RECURSE but has no stack-effect
annotation — add `( ... -- ... )` after the name".

The TCO interaction is what makes this worth shipping.  Factor's
JIT performs Tail Call Optimisation on recursive calls at tail
position — *unless* the word is wrapped in
`continuations:with-return`, which forces the call frame to
survive for potential non-local unwinding.  `lower_exit`
(yesterday's work) removed the wrap for the common cases of
EXIT, so RECURSE without EXIT-inside-a-loop gets full TCO today.

Verified by running a 100,000-deep `: down dup 0= if drop exit
then 1 - recurse ;`  in the test suite.  Completes in ~0 ms with
no stack growth.  Before `lower_exit`, the same word would have
been wrapped in with-return because of the EXIT, TCO would have
been disabled, and 100k frames would have blown the stack.  The
two transforms compose — `lower_exit` unblocks RECURSE.

### 5. VALUE and TO — polymorphic settable slots (#59)

User asked: do we have VALUE and TO?  Should VALUE accept any
type, since we're on a tagged stack?

Short answer was no, not yet, and yes the polymorphic design is
right.  Long answer became this section.

VARIABLE is intentionally complex: it exposes `@`/`!`/`+!` on
addresses, which needs cell-addressable storage with a wide/narrow
escape-analysis split for safety.  VALUE has none of that
responsibility — it's a named getter/setter pair with no
address surface.  Map straight to Factor's `get-global` /
`set-global`, which are tag-agnostic by construction:

```forth
42 VALUE x                   \ Item::Value with initial=[42]
```

emits as:

```factor
SYMBOL: nf-value-x
42 nf-value-x set-global
: x ( -- v ) nf-value-x get-global ; inline
```

And `100 TO x` emits as the bare `nf-value-x set-global`.  Three
lines per VALUE, plus one Factor word per TO use site.  All
inlined by the JIT.

The polymorphism falls out for free: a single VALUE slot accepts
ints, floats, strings, quotations — whatever Factor can tag.
Test result:

```forth
0 value v
v .                          \ 0     (int)
3.14e to v                   \ slot now holds a float
v drop ." float-ok " cr
s$" hello" to v              \ slot now holds a managed-string
v $.                         \ "hello"
```

Output: `0 float-ok hello`.  Same physical slot, three observable
types.

Cross-eval persistence: added `values: HashMap<String, Span>` to
`CompileContext`, threaded through a new
`build_with_prior_state` signature (back-compat with
`build_with_prior_and_templates` which wraps it with empty
values).  `TO target` is validated as a known VALUE at resolve
time — `ResolveError::ToNotValue` if the target is anything else,
with the message "`x` is not a VALUE (TO only works on VALUEs)".

This is a deliberate departure from ANS — ANS Forth has an
integer-only VALUE and a separate FVALUE for floats.  That split
existed because traditional Forths needed it for cell-vs-float
layout.  We don't.  One VALUE, any type.

### 6. TYPEOF + type predicates (#60)

User: "now to add insult to injury can we TYPEOF value or TOS and
what should we return so code can check a type if it needs to?"

The companion to polymorphic VALUE.  If a slot can hold any type,
user code that wants to do anything non-trivial with the contents
needs a way to ask "what's in there."  Added:

  - `TYPEOF ( x -- code )` — consumes top, returns a stable
    small int
  - `INT?` / `FLOAT?` / `STRING?` / `XT?` / `ADDR?` — predicates
    returning ANS -1/0
  - `INT-TYPE` / `FLOAT-TYPE` / `STRING-TYPE` / `XT-TYPE`
    `ADDR-TYPE` / `OTHER-TYPE` — type-code CONSTANTs for CASE
    dispatch

Type codes (chosen as small stable ints so user code can
CASE on them; 100s left for future tuple types):

```
1 = int       (Factor integer? — covers fixnum AND bignum)
2 = float
3 = string    (the s$" form, Factor strings)
4 = xt        (quotation? OR word? — covers `'` tick output)
5 = addr      (nf-addr, VARIABLE backing)
99 = other    (catch-all)
```

Implemented as a session-boot Factor eval that extends the
`forth.runtime` vocab with `nf-typeof` (a cond chain), the
constants, and the predicates.  Avoids an image rebuild.  Each
new Session::new defines them fresh; Factor accepts re-`:` of
the same name without complaint.

The user-side experience:

```forth
: describe ( x -- )
    typeof case
        int-type    of ." int "    endof
        float-type  of ." float "  endof
        string-type of ." string " endof
        ." other "
    endcase ;

42      describe                 \ "int "
3.14e   describe                 \ "float "
s$" hi" describe                 \ "string "
```

CASE on TYPEOF folds at JIT time because the type-code CONSTANTs
inline to their literal integers — same machine code as if you'd
hand-rolled `dup 1 = [ ... ] [ dup 2 = [ ... ] ...] if` directly.

Polymorphic VALUE + TYPEOF compose:

```forth
0 value slot
slot typeof .       \ 1
3.14e to slot
slot typeof .       \ 2
s$" x" to slot
slot typeof .       \ 3
```

The slot is structurally one Factor global; TYPEOF asks Factor
its class via `nf-typeof`'s cond chain.  No metadata, no
type tag we maintain — Factor was already doing the tagging.

### 7. Image compression — 134 MB → 17 MB

Mundane but important.  The bootstrap image was 134 MB raw, which
is intimidating in a downloaded artifact.  Added `compress-image`
binary using the `zstd = "0.13"` crate at level 19 (slow encode,
fast decode — appropriate when we encode once at release time and
ship to many users):

```
compressing factorforth.image (134,705,688 bytes)
  → factorforth.image.zst at zstd level 19
done in 45.56s — 17,486,907 bytes (7.70× smaller)
```

Session inflate-on-startup wired into `Session::new`: if
`factorforth.image` is absent but `.zst` is present, inflate
through a `.tmp` sidecar then atomic-rename into place.  New
error variant `SessionError::ImageInflateFailed(io::Error)`
catches disk-full / permissions / corrupt-archive cases with the
underlying I/O error preserved.

User download dropped from ~137 MB to ~19 MB.  First start pays
~1 second one-time inflation cost; subsequent starts see the raw
image already there and skip.

## v1 milestone closed (#10)

Side-by-side Mandelbrot rendering, FactorForth on the left,
WF64 on the right, both instant, both visually identical, both
running the same `gfx-mandelbrot.f` shape with the only
difference being WF64 using its hand-written MASM `fractal-iter`
primitive while FactorForth uses the pure-Forth port through
the new pipeline.

The screenshot is in the conversation log.  We're not going
to be able to look at it together again, but it happened.

## Architectural reflection

The line we've been crossing all session: this isn't a
FORTH-to-Factor translator anymore.  It's a compiler whose
surface language is FORTH and whose IR target is Factor.  The
distinction matters because:

  - Each ANS sharp edge gets its own ~150-line pre-emit pass,
    not a knot in emit.rs or a runtime word
  - Each pass has its own unit tests and runtime tests
  - The IR Factor sees gets progressively more
    idiomatic-Factor-shaped — `?dup if` doesn't fight the
    SSA pass anymore, `exit` doesn't disable TCO, `recurse`
    doesn't blow the stack, `value`/`to` use the same
    globals machinery Factor uses for its own state
  - When perf matters, we benchmark the *emitted Factor* and
    reason about it as Factor code.  No need to explain perf
    cliffs as "well, Forth doesn't really have that concept"

The other thread worth pulling on: today's design choices were
mostly *architectural* rather than tactical.  "Should EXIT be a
runtime continuations:return or a pre-emit AST rewrite?" isn't
a question about which is cleverer, it's a question about which
side of the FORTH-vs-Factor language gap you live on.  Once you
decide, the code is straightforward.

We're now at:
  - 60 tasks total, 53 completed
  - ~150 lines of new compiler code per ANS feature, +tests
  - 122 lib unit tests, all green
  - 5 EXIT + 6 ?DUP/RECURSE + 6 VALUE/TO + 6 TYPEOF runtime tests
  - 41 smoke_runtime + 1 Mandelbrot + 1 force-compile, all green
  - Distribution size 19 MB (was 137 MB)

## What's still open

Selected items:

  - **#46** ANS Core stragglers: U< U> COUNT KEY? .S MOVE PICK ROLL
  - **#49** DEFER / IS — likely another desugar pass shape
  - **#55** Loop-internal EXIT (Rec 2 — recursive lowering)
  - **#56** XT-in-wide-variable type-deformation bug
  - **#57** Strict LEAVE via tail-swallow + flag (will reuse the
    `lower_terminator` skeleton once two passes exist)
  - **#37** Graphics command-queue design

Each one slots into the same pipeline shape.  None of them require
new infrastructure — they're all ~150-line additions to the desugar
stage with their own tests.

## What's next (probably)

The user mentioned NewOpenDylan in passing — a Rust+LLVM Dylan
JIT at `E:\NewOpenDylan\`.  We talked about whether Factor would
be a viable backend for Dylan as an alternative or parallel
path.  The structural fit is excellent: Dylan inherits multiple
dispatch, conditions/restarts, the numeric tower, and the
sequence-as-protocol abstraction directly from CLOS, and Factor
implements all of those.  A Dylan-to-Factor port would be ~3-4
desugar passes plus the infix-to-concatenative front-end, and
would skip the Sprint 11d `gc.statepoint` machinery entirely
because Factor's GC contract is already paid.

Not on the roadmap.  Just noting the shape for if it ever wants to
become real.

— end of arc
