\ demo.fth — Pure ANS Forth source loaded via forth-load.
\ Comments use  \  (Forth convention). ! is the store word, not a comment.
\ Control structures: IF ELSE THEN  BEGIN WHILE REPEAT  BEGIN UNTIL  BEGIN AGAIN
\
\ Load from Factor:
\   "E:\\NewFactor\\demo.fth" forth-load

USING: forth.all forth.variables forth.memory io kernel math prettyprint ;
FROM: forth.variables => @ ;
IN: scratchpad

\ ── variable / store / fetch ───────────────────────────────────────────

"--- variable ---" print

variable oranges
100 oranges !
oranges @ .                  \ should print 100

50 oranges !
oranges @ .                  \ should print 50

3 oranges +!
oranges @ .                  \ should print 53

\ ── IF / THEN ──────────────────────────────────────────────────────────

"--- IF THEN ---" print

\ Compiles to:  n [ "positive" print ] when
: positive-msg ( n -- )
    0 > IF "positive" print THEN ;

5 positive-msg               \ should print positive
-3 positive-msg              \ should print nothing

\ ── IF / ELSE / THEN ───────────────────────────────────────────────────

"--- IF ELSE THEN ---" print

\ Compiles to:  n [ "yes" print ] [ "no" print ] if
: yes-no ( n -- )
    0 > IF "yes" print ELSE "no" print THEN ;

7 yes-no                     \ should print yes
0 yes-no                     \ should print no

\ ── Nested IF ──────────────────────────────────────────────────────────

"--- nested IF ---" print

: classify ( n -- )
    dup 0 = IF
        drop "zero" print
    ELSE
        0 > IF "positive" print ELSE "negative" print THEN
    THEN ;

0 classify                   \ zero
5 classify                   \ positive
-2 classify                  \ negative

\ ── BEGIN / UNTIL ──────────────────────────────────────────────────────

"--- BEGIN UNTIL ---" print

\ Compiles to:  [ n dup . 1 - dup 0 < ] until
: countdown ( n -- )
    BEGIN
        dup .
        1 -
        dup 0 <
    UNTIL
    drop ;

3 countdown                  \ should print 3 2 1 0

\ ── BEGIN / WHILE / REPEAT ─────────────────────────────────────────────

"--- BEGIN WHILE REPEAT ---" print

\ Compiles to:  [ n dup 0 > ] [ n dup . 1 - ] while
: count-down ( n -- )
    BEGIN dup 0 > WHILE
        dup .
        1 -
    REPEAT
    drop ;

4 count-down                 \ should print 4 3 2 1

\ ── BEGIN / AGAIN (infinite loop with EXIT equivalent) ─────────────────
\ Factor's `loop` runs forever; escape via throw or return.
\ Uncomment to test — it would run until the counter expires.
\
\ : loop-n ( n -- )
\     BEGIN
\         dup 0 = [ drop return ] when
\         dup .
\         1 -
\     AGAIN ;

\ ── 2variable ──────────────────────────────────────────────────────────

"--- 2variable ---" print

2variable point
10 point !
20 point 8 + !
point 2@ . .                 \ should print 20 then 10  (hi TOS)

\ ── value / to ─────────────────────────────────────────────────────────

"--- value ---" print

0 value temperature
temperature .                \ should print 0
37 to temperature
temperature .                \ should print 37

\ ── constant ───────────────────────────────────────────────────────────

"--- constant ---" print

6 constant sides
sides .                      \ should print 6

\ ── address arithmetic ─────────────────────────────────────────────────

"--- address arithmetic ---" print

variable a
variable b
b a - .                      \ should print 8  (one cell apart)
5 a !
a @ .                        \ should print 5
a cell+ @ .                  \ reads the cell after a (= b, initialised to 0)

"demo.fth done" print
