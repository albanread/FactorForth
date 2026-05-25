\ let-algebra.f - LET, FactorForth's infix algebra DSL
\
\ Plain Forth makes you think in postfix.  For math-heavy code
\ that's a chore — LET lets you write the algebra the way
\ paper-and-pencil does it, and the compiler lowers it to
\ stack ops for you.

\ Pythagoras: hypotenuse of a right triangle
: hypot   ( a b -- c )
    LET (a b) -> (c) =
        sqrt (a * a + b * b)
    END
;

\ Quadratic formula's discriminant: b^2 - 4ac
: discriminant   ( a b c -- disc )
    LET (a b c) -> (d) =
        b * b - 4 * a * c
    END
;

\ Quadratic root (positive branch only, assumes disc >= 0)
: quad-pos-root   ( a b c -- root )
    LET (a b c) -> (r) =
        (-1 * b + sqrt (b * b - 4 * a * c)) / (2 * a)
    END
;

\ Try it:
\   3 4 hypot f.                  ( prints 5.0 )
\   1 5 6 discriminant .          ( prints 1   )
\   1 -5 6 quad-pos-root f.       ( prints 3.0 )

." Loaded: hypot, discriminant, quad-pos-root.  Try '3 4 hypot f.'" cr
