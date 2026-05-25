\ stack-tour.f - a guided tour of the data stack
\
\ Type each line at the > prompt in order; watch the stack
\ change.  Use  .S  at any time to see what's there.

\ ── pushing values ──
\   1 2 3            ( stack: 1 2 3 )
\   .s               ( shows: <3> 1 2 3 )

\ ── stack shufflers ──
\   dup              ( a -- a a       )    duplicate top
\   drop             ( a --           )    discard top
\   swap             ( a b -- b a     )    exchange top two
\   over             ( a b -- a b a   )    copy 2nd item to top
\   rot              ( a b c -- b c a )    rotate top three left
\   nip              ( a b -- b       )    drop 2nd item
\   tuck             ( a b -- b a b   )    duplicate top under 2nd

\ ── arithmetic consumes its inputs ──
\   3 4 +            ( -- 7 )
\   10 4 -           ( -- 6 )
\   5 6 *            ( -- 30 )
\   20 4 /           ( -- 5 )

\ ── words that produce nothing ──
\   ." Hello"        ( -- ; prints "Hello" )
\   cr               ( -- ; emits newline  )
\   42 .             ( n -- ; pretty-prints n )

." Loaded: see the comments above and replay each line at the prompt." cr
." Try ' 1 2 3 .s  '  then  ' swap .s ' " cr
