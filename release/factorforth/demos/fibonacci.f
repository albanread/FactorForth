\ fibonacci.f - the classic recurrence in three styles
\
\ FactorForth lets you write the same idea three ways.  Pick
\ whichever reads best for you and keep going.

\ ── 1. plain recursion (clear, slow for large n) ────────────────
: fib-rec   ( n -- fib-n )
    dup 2 < if exit then
    dup 1 - recurse
    swap 2 - recurse
    +
;

\ ── 2. iterative with two accumulators (fast) ───────────────────
: fib-iter   ( n -- fib-n )
    0 1 rot 0 ?do
        over + swap
    loop
    drop
;

\ ── 3. tabular printer ──────────────────────────────────────────
: fib-table   ( n -- )
    0 ?do
        i fib-iter . space
    loop
    cr
;

\ Try it:
\    10 fib-iter .  cr             ( prints 55 )
\    20 fib-table                  ( prints the first 20 fibs )

." Loaded: fib-rec, fib-iter, fib-table.  Try '15 fib-table'" cr
