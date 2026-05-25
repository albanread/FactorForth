! forth.numeric — ANS Forth pictured numeric output and number printing.
!
! Implements: <#  #  #s  hold  holds  sign  #>
!             .  u.  d.  ud.  .r  u.r
!             decimal hex octal binary
!             hex. dec. oct. bin.
!
! The <# / #> buffer is a dynamic variable (not thread-safe by default).
! `number-base` (from prettyprint.config) controls the numeric base.

USING: combinators io kernel math math.order math.parser
       namespaces prettyprint.config sequences strings ;
IN: forth.numeric

! ── Base switching ────────────────────────────────────────────────────

: decimal ( -- )  10 number-base set-global ; inline
: hex     ( -- )  16 number-base set-global ; inline
: octal   ( -- )   8 number-base set-global ; inline
: binary  ( -- )   2 number-base set-global ; inline

! ── Pictured Numeric Output buffer ───────────────────────────────────

SYMBOL: pno-buf   ! string accumulator (built right-to-left)

: <#  ( -- )    "" pno-buf set ;

! BUG FIX (2026-05-24): old code used `prepend` here, but combined with
! `change`'s argument order (old value on top), that produces
! `old + new` — digits accumulated in wrong order ("24" for 42).
! `append` with stack `new old` does `new + old`, putting the new
! digit at the FRONT — which is what ANS Forth `hold` requires.
: hold  ( char -- )    1string pno-buf [ append ] change ;
: holds ( str -- )     pno-buf [ append ] change ;

: #  ( u -- u' )
    ! BUG FIX (2026-05-24): old code had `swap` here, which left the
    ! QUOTIENT on top to be held as a digit (wrong) and returned the
    ! REMAINDER (wrong) — making #s never terminate for u >= base.
    ! Factor's /mod is ( x y -- quot rem ) with rem on TOP; that's
    ! exactly what we want.  No swap, no dip — just convert top, hold.
    number-base get /mod         ! ( quot rem )  — rem on top
    dup 9 > [ 7 + ] when         ! ( quot rem' ) — digit char-offset
    CHAR: 0 + hold ;             ! ( quot )      — quot remains on stack

: #s  ( u -- 0 )
    ! BUG FIX (2026-05-24): ANS Forth #s is defined to emit AT LEAST ONE
    ! digit, even for input 0.  The old `[ dup 0 > ] [ # ] while` skipped
    ! the loop body when u=0, producing an empty number string.
    ! Run # once unconditionally, then loop while quotient is nonzero.
    # [ dup 0 > ] [ # ] while ;

: sign  ( n -- )    0 < [ CHAR: - hold ] when ;

: #>  ( 0 -- str )    drop pno-buf get ;

! ── Number printing ───────────────────────────────────────────────────
! Note: no trailing `bl` here so callers can control spacing.

: u.  ( u -- )
    <# #s #> write " " write ;

: .  ( n -- )
    dup abs <# #s swap 0 < [ CHAR: - hold ] when #> write " " write ;

: d.   ( lo hi -- )    drop . ;
: ud.  ( ud -- )       drop u. ;

! ── Right-justified printing ─────────────────────────────────────────

: (pad-to)  ( str width -- str )
    over length - 0 max CHAR: \s <string> prepend ;

: .r  ( n width -- )
    [ dup abs <# #s swap 0 < [ CHAR: - hold ] when #> ] dip
    (pad-to) write ;

: u.r  ( u width -- )
    [ <# #s #> ] dip (pad-to) write ;

: d.r  ( d width -- )    drop .r ;
: ud.r ( ud width -- )   drop u.r ;

! ── Fixed-base printing ──────────────────────────────────────────────

: hex.   ( n -- )   16 number-base [ . ] with-variable ;
: dec.   ( n -- )   10 number-base [ . ] with-variable ;
: oct.   ( n -- )    8 number-base [ . ] with-variable ;
: bin.   ( n -- )    2 number-base [ . ] with-variable ;

! ── .byte — print one byte in hex ───────────────────────────────────

: .byte  ( n -- )   >hex 2 CHAR: 0 pad-head write " " write ;

! ── .s — print stack ─────────────────────────────────────────────────

: .s  ( -- )
    "<" write get-datastack length . "> " write
    get-datastack [ . ] each nl ;
