! forth.doubles — ANS Double-cell word set.
!
! Stack convention: ( lo hi ) where lo is deeper, hi is TOS.
! For positive n: lo=n, hi=0.  For negative n: lo=n, hi=-1.
! Factor integers are arbitrary precision so bignum ops are correct.

USING: forth.core kernel math math.functions math.order sequences ;
IN: forth.doubles

! ── Single ↔ Double ──────────────────────────────────────────────────

: d>s   ( lo hi -- n )   drop ; inline
: d>f   ( lo hi -- f )   drop >float ; inline

! ── Double arithmetic ────────────────────────────────────────────────

:: d+  ( lo1 hi1 lo2 hi2 -- lo3 hi3 )  lo1 lo2 +  hi1 hi2 + ;
:: d-  ( lo1 hi1 lo2 hi2 -- lo3 hi3 )  lo1 lo2 -  hi1 hi2 - ;

: dnegate  ( lo hi -- lo' hi' )   [ neg ] bi@ ; inline

: dabs  ( lo hi -- lo' hi' )   dup 0 < [ dnegate ] when ; inline

:: d2*  ( lo hi -- lo' hi' )   ! Shift 128-bit value left by 1
    lo 1 shift 0xFFFFFFFFFFFFFFFF bitand   ! lo': low 64 bits
    hi 1 shift lo 0 < [ 1 bitor ] when ;   ! hi': shifted, carry lo MSB

:: d2/  ( lo hi -- lo' hi' )   ! Arithmetic right shift 128-bit by 1
    lo 0xFFFFFFFFFFFFFFFF bitand -1 shift
    hi 1 bitand 63 shift bitor           ! lo': shifted + carry from hi
    hi -1 shift                          ! hi': arithmetic right shift
    swap ;                               ! -> ( lo' hi' )

! ── Double comparisons ───────────────────────────────────────────────

: d0=   ( lo hi -- ? )   bitor zero? ; inline
: d0<   ( lo hi -- ? )   nip 0 < ; inline
: d0>   ( lo hi -- ? )
    2dup d0= not [ nip 0 > ] [ 2drop f ] if ; inline
: d0<>  ( lo hi -- ? )   d0= not ; inline
: d0>=  ( lo hi -- ? )   d0< not ; inline
: d0<=  ( lo hi -- ? )   2dup d0= [ 2drop t ] [ d0< ] if ; inline

:: d=   ( lo1 hi1 lo2 hi2 -- ? )
    lo1 lo2 = [ hi1 hi2 = ] [ f ] if ;

:: d<   ( lo1 hi1 lo2 hi2 -- ? )
    hi1 hi2 = [ lo1 lo2 u< ] [ hi1 hi2 < ] if ;

:: d>   ( lo1 hi1 lo2 hi2 -- ? )
    hi2 hi1 = [ lo2 lo1 u< ] [ hi2 hi1 < ] if ;

:: d<=  ( lo1 hi1 lo2 hi2 -- ? )
    hi1 hi2 = [ lo1 lo2 u<= ] [ hi1 hi2 < ] if ;

:: d>=  ( lo1 hi1 lo2 hi2 -- ? )
    hi1 hi2 = [ lo1 lo2 u>= ] [ hi1 hi2 > ] if ;

:: du<  ( lo1 hi1 lo2 hi2 -- ? )
    hi1 hi2 = [ lo1 lo2 u< ] [ hi1 hi2 u< ] if ;

:: du>  ( lo1 hi1 lo2 hi2 -- ? )
    hi2 hi1 = [ lo2 lo1 u< ] [ hi2 hi1 u< ] if ;

:: du<= ( lo1 hi1 lo2 hi2 -- ? )
    hi1 hi2 = [ lo1 lo2 u<= ] [ hi1 hi2 u< ] if ;

:: du>= ( lo1 hi1 lo2 hi2 -- ? )
    hi1 hi2 = [ lo1 lo2 u>= ] [ hi1 hi2 u> ] if ;

! ── Double min / max ─────────────────────────────────────────────────

: dmax  ( lo1 hi1 lo2 hi2 -- lo hi )
    4dup d< [ 2swap ] when 2drop ; inline

: dmin  ( lo1 hi1 lo2 hi2 -- lo hi )
    4dup d> [ 2swap ] when 2drop ; inline

! ── m+ — add single to double ────────────────────────────────────────

: m+    ( lo hi n -- lo' hi' )   s>d d+ ; inline

! ── Division ─────────────────────────────────────────────────────────

:: um/mod  ( lo hi u -- rem quot )
    hi 64 shift lo bitor u /mod ;

:: sm/rem  ( lo hi n -- rem quot )
    hi 64 shift lo bitor n /mod ;

:: fm/mod  ( lo hi n -- rem quot )
    hi 64 shift lo bitor n [ mod ] [ / floor ] 2bi ;

: ud/mod  ( ud u -- rem ud' )   /mod ; inline
