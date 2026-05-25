\ factorial.f - recursion and the return stack
\
\ Demonstrates: recursive word definition, DO/LOOP with i,
\ a recursive style with IF/THEN/ELSE.

: factorial-iter   ( n -- n! )
    1 swap                  \ ( accumulator counter )
    1+ 1 ?do
        i *
    loop
;

: factorial-rec   ( n -- n! )
    dup 1 <= if
        drop 1
    else
        dup 1 - recurse *
    then
;

\ Try it:
\   5 factorial-iter .  cr     ( prints 120 )
\   6 factorial-rec  .  cr     ( prints 720 )
\  10 factorial-iter .  cr     ( prints 3628800 )

." Loaded: factorial-iter and factorial-rec.  Try '6 factorial-iter .'" cr
