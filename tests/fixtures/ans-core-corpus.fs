\ tests/fixtures/ans-core-corpus.fs
\
\ A hand-curated subset of canonical Forth 2012 Core T{ -> }T
\ assertions, restricted to words NewFactor currently ships.
\ The Rust runner (tests/session_test_runner.rs) parses this
\ file, extracts each T{ -> }T block, and runs both sides as
\ independent evals against a shared Session.
\
\ Each PASS proves NewFactor's behaviour matches ANS for that
\ word with those inputs.  This is the working test ground for
\ the canonical Hayes/Jackson suite — we'll grow the corpus
\ over time as more words land.

\ ── Stack manipulation ────────────────────────────────────────────────────

T{ 1 2 3 swap -> 1 3 2 }T
T{ 1 2 over -> 1 2 1 }T
T{ 1 2 drop -> 1 }T
T{ 1 2 3 rot -> 2 3 1 }T
T{ 1 2 3 -rot -> 3 1 2 }T
T{ 1 2 nip -> 2 }T
T{ 1 2 tuck -> 2 1 2 }T
T{ 1 dup -> 1 1 }T
T{ 1 2 2dup -> 1 2 1 2 }T
T{ 1 2 3 4 2drop -> 1 2 }T
T{ 1 2 3 4 2swap -> 3 4 1 2 }T
T{ 1 2 3 4 2over -> 1 2 3 4 1 2 }T

\ ── Arithmetic ────────────────────────────────────────────────────────────

T{ 2 3 + -> 5 }T
T{ 5 3 - -> 2 }T
T{ 4 5 * -> 20 }T
T{ 20 4 / -> 5 }T
T{ 7 3 mod -> 1 }T
T{ -7 3 mod -> 2 }T
T{ 7 -3 mod -> -2 }T
T{ 5 negate -> -5 }T
T{ -5 abs -> 5 }T
T{ 3 5 max -> 5 }T
T{ 3 5 min -> 3 }T
T{ 1 1+ -> 2 }T
T{ 5 1- -> 4 }T
T{ 3 2* -> 6 }T
T{ 8 2/ -> 4 }T
T{ 17 5 /mod -> 2 3 }T
T{ -17 5 /mod -> 3 -4 }T
T{ 100 3 4 */ -> 75 }T
T{ 100 3 7 */mod -> 6 42 }T

\ ── Comparison (ANS booleans -1 / 0) ──────────────────────────────────────

T{ 5 5 = -> -1 }T
T{ 5 6 = -> 0 }T
T{ 5 6 <> -> -1 }T
T{ 5 5 <> -> 0 }T
T{ 3 5 < -> -1 }T
T{ 5 3 < -> 0 }T
T{ 5 3 > -> -1 }T
T{ 3 5 > -> 0 }T
T{ 0 0= -> -1 }T
T{ 5 0= -> 0 }T
T{ -3 0< -> -1 }T
T{ 3 0> -> -1 }T
T{ 0 0<> -> 0 }T
T{ 5 0<> -> -1 }T

\ ── Bitwise ──────────────────────────────────────────────────────────────

T{ -1 -1 and -> -1 }T
T{ -1 0 and -> 0 }T
T{ -1 invert -> 0 }T
T{ 0 invert -> -1 }T
T{ 1 4 lshift -> 16 }T
T{ 16 4 rshift -> 1 }T

\ ── Control flow ─────────────────────────────────────────────────────────

T{ : tb1 -1 if 99 else 88 then ; tb1 -> 99 }T
T{ : tb2  0 if 99 else 88 then ; tb2 -> 88 }T
T{ : tb3  5 if 99 else 88 then ; tb3 -> 99 }T
T{ : sum10  0 10 0 do i + loop ; sum10 -> 45 }T

\ ── ANS float arithmetic ─────────────────────────────────────────────────

T{ 2.0e0 3.0e0 f+ -> 5.0e0 }T
T{ 5.0e0 2.0e0 f- -> 3.0e0 }T
T{ 4.0e0 3.0e0 f* -> 12.0e0 }T
T{ 6.0e0 2.0e0 f/ -> 3.0e0 }T

\ ── Conversion ───────────────────────────────────────────────────────────

T{ 7 s>d -> 7 }T
T{ 7 d>s -> 7 }T
