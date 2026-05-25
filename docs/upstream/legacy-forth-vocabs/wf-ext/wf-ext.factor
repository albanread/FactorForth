! forth.wf-ext — WF64 DSL extensions and ANS Core-Ext words.
!
! Ports the high-level words from WF64/lib/core.f that are written in
! Forth (not assembly) and have direct Factor equivalents.

USING: combinators forth.core io kernel lexer math math.functions
       math.order math.parser namespaces parser prettyprint
       quotations sequences strings vocabs.parser words ;
IN: forth.wf-ext

! ── Control / logic extensions ───────────────────────────────────────

: ?: ( flag a b -- a|b )   rot [ drop ] [ nip ] if ; inline

: assert ( flag str -- )
    swap [ drop ] [ . " assertion failed" print flush 0 throw ] if ; inline

! ── Stack helpers ────────────────────────────────────────────────────
! third, under+, ?negate, 0max are defined in forth.core.

: fourth  ( a b c d -- a b c d a )  3 pick ; inline

! ── Math power words ─────────────────────────────────────────────────

: square  ( n -- n^2 )  dup * ; inline
: cube    ( n -- n^3 )  dup dup * * ; inline
: quad    ( n -- n^4 )  square square ; inline
: sixth   ( n -- n^6 )  cube square ; inline

! ── Synonym ──────────────────────────────────────────────────────────
! Create newname as an alias for oldname.

SYNTAX: synonym
    scan-token current-vocab lookup-word   ! ( -- old-word )
    scan-token create-word-in              ! ( old-word new-word )
    swap 1quotation define ;               ! new-word: [ old-word ]

! ── Compile-time conditionals (Tools-Ext) ────────────────────────────

SYNTAX: [defined]
    scan-token current-vocab lookup-word [ drop t ] [ f ] if* ;

SYNTAX: [undefined]
    scan-token current-vocab lookup-word [ drop f ] [ t ] if* ;

! [if]/[else]/[then] — stub implementations
! TODO: proper implementation needs token-stream skipping.
! For now, [if] consumes the flag and always runs the [then] branch.
SYNTAX: [if]    ;        ! stub: no conditional token skipping (always takes then-branch)
SYNTAX: [else]  ;        ! no-op
SYNTAX: [then]  ;        ! no-op

! ── ANSI terminal control (Facility word set) ────────────────────────

: at-xy  ( col row -- )
    27 emit CHAR: [ emit
    1 + number>string write CHAR: ; emit
    1 + number>string write CHAR: H emit ;

: page  ( -- )
    "\e[2J\e[H" write flush ;

! ── File access method constants ─────────────────────────────────────

CONSTANT: r/o 1    ! read-only
CONSTANT: w/o 2    ! write-only
CONSTANT: r/w 3    ! read-write
: bin ( fam -- fam ) ;   ! no-op; binary mode implicit in Factor

! ── Floating-point extensions ────────────────────────────────────────

: fabs   ( r -- |r| )  abs ; inline
: fmax   ( r1 r2 -- max )  max ; inline
: fmin   ( r1 r2 -- min )  min ; inline

: f=     ( r1 r2 -- ? )  = ; inline
: f<>    ( r1 r2 -- ? )  = not ; inline
: f>     ( r1 r2 -- ? )  > ; inline
: f<=    ( r1 r2 -- ? )  <= ; inline
: f>=    ( r1 r2 -- ? )  >= ; inline
: f0<>   ( r -- ? )  0.0 = not ; inline

: f2*    ( r -- 2r )    2.0 * ; inline
: f2/    ( r -- r/2 )   2.0 / ; inline
: ftrunc ( r -- r' )    truncate ; inline
: fround ( r -- r' )    round ; inline
: falog  ( r -- 10^r )  10.0 swap ^ ; inline

: f.  ( r -- )
    dup 0.0 = [
        drop "0.000000" write
    ] [
        dup 0.0 < [ "-" write neg ] when
        dup truncate >integer number>string write
        CHAR: . emit
        dup truncate - 1000000.0 * truncate >integer abs
        number>string 6 CHAR: 0 pad-head write
    ] if space ;
