! forth.strings — ANS string word set plus WF64 string utilities.
!
! In NewFactor, strings are Factor string objects, NOT c-addr/u pairs.
! ANS words that expect c-addr/u take Factor strings instead.
! This is the main deviation from strict ANS compliance.
!
! Words provided:
!   count  /string  cmove  cmove>  compare  search  fill  erase  blank
!   move   -trailing  -leading
!   starts-with?  ends-with?  contains?
!   s"  (Factor already provides this as a string literal)
!   substitute / replaces  (variable-substitution facility)

USING: arrays assocs combinators io kernel math math.order
       namespaces sequences splitting strings ;
IN: forth.strings

! ── Basic string ops ─────────────────────────────────────────────────

! Factor's count: ( seq quot -- n ) — counts elements matching predicate.
! ANS count: ( c-addr -- c-addr+1 u ) — length of counted string.
! We define ANS count on Factor strings (trivially: addr=string, u=length):
: count  ( str -- str u )   dup length ; inline

: /string  ( str n -- str' )   tail ; inline

! cmove / cmove>: sequence copy (not raw memory in Factor)
: cmove   ( src dst len -- )   ! copy len chars from src to dst (forward)
    [ head-slice ] dip [ replace-slice ] keep 2drop ; ! approximate
: cmove>  ( src dst len -- )   ! copy backward (for overlapping copies)
    cmove ;  ! Factor handles overlap in sequence ops; same here

: compare  ( s1 s2 -- n )
    <=> dup { +lt+ +eq+ +gt+ } [ = ] with map [ ] find drop
    { -1 0 1 } nth ; inline  ! -1 / 0 / 1

: search  ( hay needle -- hay' flag )
    ! ANS: search ( c-addr1 u1 c-addr2 u2 -- c-addr3 u3 flag )
    ! Here: both are Factor strings.  Returns ( remainder t ) or ( hay f ).
    2dup subseq-start [
        [ tail ] dip t
    ] [
        drop f
    ] if* ; inline

: move  ( src dst len -- )
    drop replace-slice 2drop ; ! approximate; Factor sequence copy

: fill  ( str n char -- )
    [ ] dip <string> swap [ replace ] 2keep 2drop ; inline

: erase  ( str n -- )
    over 0 fill ; inline

: blank  ( str n -- )
    CHAR: space fill ; inline

! ── WF64 string extensions ───────────────────────────────────────────

: -trailing  ( str -- str' )
    [ CHAR: space = ] trim-tail ; inline

: -leading   ( str -- str' )
    [ CHAR: space = ] trim-head ; inline

: starts-with?  ( str prefix -- ? )
    head? ; inline

: ends-with?  ( str suffix -- ? )
    tail? ; inline

: contains?  ( str substr -- ? )
    subseq? ; inline

! ── SUBSTITUTE / REPLACES (Forth 2012 String-Ext) ────────────────────
!
! REPLACES: bind a name string to a value string.
! SUBSTITUTE: walk a source string, expand %name% references.
!
! We implement this using a dynamic assoc table.

SYMBOL: subst-table

: subst-init  ( -- )
    H{ } clone subst-table set-global ;

subst-init

: replaces  ( val-str name-str -- )
    subst-table get-global set-at ;

: (subst-expand)  ( src -- dst count )
    0 swap "" swap
    [ dup empty? not ]
    [
        unclip-slice swap
        dup CHAR: % = [
            drop
            ! find closing %
            dup [ CHAR: % = ] find [
                head-slice >string  ! name
                [ tail ] dip        ! advance past name
                dup first CHAR: % = [  ! skip closing %
                    rest
                    over subst-table get-global at [
                        [ append ] dip
                        [ 1 + ] dip
                    ] [
                        [ "%" prepend "%" append append ] dip
                    ] if*
                ] [
                    [ "%" prepend append ] dip
                ] if
            ] [
                drop [ "%" append ] dip
            ] if
        ] [
            1string [ append ] dip
        ] if
    ] while
    swap ;

: substitute  ( src dst-buf dst-max -- dst-str count )
    drop drop  ! dst-buf and dst-max ignored; we build a new string
    (subst-expand) ;
