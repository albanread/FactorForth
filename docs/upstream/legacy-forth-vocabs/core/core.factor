! forth.core — ANS Forth user-word aliases and missing primitives.
!
! Factor already provides (same name, same semantics):
!   dup  drop  swap  rot  -rot  over  nip  tuck  pick
!   2dup  2drop  2swap  2over  2nip
!   3dup  3drop  4dup
!   +  -  *  /  mod  /mod  abs  max  min  neg
!   =  <  >  <=  >=  u<  u>  u<=  u>=
!   bitand  bitor  bitxor  bitnot  shift
!   not  if  when  unless  while  until  loop  times
!   throw  nl  write  print
!
! DESIGN DECISIONS:
!   • Booleans: Factor uses t/f.  Forth expects -1/0.
!     Comparison words here return Factor t/f; use bool>flag to convert.
!   • Memory (@  !  here  allot): not applicable — Factor uses GC objects.
!     Use `variable` / `constant` / `value` from forth.variables instead.
!   • >r / r> / r@: no explicit return-stack in Factor.
!     Use `dip`, `keep`, `2keep`, `bi`, `tri` instead:
!       a >r b r>  →  b [ a ] dip (NOT the same order — think carefully)
!       >r x r>    →  [ x ] dip
!       2>r x 2r>  →  [ [ x ] dip ] dip

USING: kernel kernel.private math math.bitwise math.functions math.order
       math.parser sequences strings io namespaces combinators arrays
       prettyprint parser lexer forth.fstack ;
IN: forth.core

! ── Arithmetic ────────────────────────────────────────────────────────

: 1+    ( n -- n+1 )   1 + ; inline
: 1-    ( n -- n-1 )   1 - ; inline
: 2*    ( n -- n*2 )   1 shift ; inline
: 2/    ( n -- n/2 )   -1 shift ; inline                ! signed floor (Factor shift is arithmetic for signed ints)
: u2/   ( n -- n/2 )   -1 shift ; inline               ! logical right
: 3*    ( n -- n*3 )   dup 2 shift + ; inline
: 5*    ( n -- n*5 )   dup 4 shift + ; inline
: 10*   ( n -- n*10 )  dup 1 shift swap 3 shift + ; inline

: negate  ( n -- -n )  neg ; inline

: */    ( n1 n2 n3 -- q )     [ * ] dip / ; inline
: */mod ( n1 n2 n3 -- r q )   [ * ] dip /mod ; inline

! Double-cell: Factor ints are arbitrary precision — d ops are identity lifts.
! Convention: ( n -- lo hi ) — lo is deeper, hi (sign extension) is TOS.
: s>d  ( n -- lo hi )   dup 0 < [ -1 ] [ 0 ] if ;

! um* / m*: split product into 64-bit lo and 64-bit hi cells.
: um*  ( u1 u2 -- lo hi )
    * dup 0xFFFFFFFFFFFFFFFF bitand swap -64 shift ;

: m*   ( n1 n2 -- lo hi )
    * dup 0xFFFFFFFFFFFFFFFF bitand swap -64 shift ;

! ── Bitwise (Forth names) ─────────────────────────────────────────────
! Factor and/or/xor are LOGICAL.  Forth AND/OR/XOR are BITWISE.

: and     ( x y -- z )  bitand ; inline
: or      ( x y -- z )  bitor  ; inline
: xor     ( x y -- z )  bitxor ; inline
: invert  ( x -- ~x )   bitnot ; inline

: lshift  ( x u -- x' )  shift ; inline
: rshift  ( x u -- x' )  neg shift ; inline
: arshift ( x u -- x' )  neg shift ; inline

! ── Comparison ────────────────────────────────────────────────────────

: 0=    ( n -- ? )  zero? ; inline
: 0<    ( n -- ? )  0 < ; inline
: 0>    ( n -- ? )  0 > ; inline
: 0<>   ( n -- ? )  zero? not ; inline
: <>    ( a b -- ? )  = not ; inline
! u<= and u>= are provided by math.order — no need to redefine.

! ── Stack ─────────────────────────────────────────────────────────────

: ?dup   ( n -- 0 | n n )  dup [ dup ] when ; inline
: depth  ( -- n )  get-datastack length ; inline

! 3dup, 4dup, 2nip are provided by Factor's kernel.
! 2swap is NOT in kernel — define it here.
: 2swap  ( a b c d -- c d a b )  [ [ swap ] dip swap ] dip swap ; inline

! 3drop and 4drop may not be in all Factor versions — define defensively.
: 3drop  ( a b c -- )  drop 2drop ; inline
: 4drop  ( a b c d -- )  2drop 2drop ; inline

! ── Retain stack (>r / r> / r@ / rdrop) ─────────────────────────────
! Implemented via forth.fstack: a fixed-capacity tuple-backed stack pinned
! to special-object slot 82.  get-frs compiles to a single VM field load
! (the `special-object` intrinsic fires on the literal 82), vs ~10 insns
! for the old SYMBOL: r-stk get (OBJ-GLOBAL hashtable lookup).
! fstack-push/pop use tuple ##slot-imm + nth-unsafe: ~7 insns total.
! Net result: ~8 insns per >r/r>  vs ~23 with the old SYMBOL: r-stk + V{}.

: >r    ( x -- )   get-frs fstack-push ; inline
: r>    ( -- x )   get-frs fstack-pop  ; inline
: r@    ( -- x )   get-frs fstack-peek ; inline
: rdrop ( -- )     get-frs fstack-drop ; inline
: 2>r   ( x y -- )   swap >r >r ; inline
: 2r>   ( -- x y )   r> r> swap ; inline
: 2r@   ( -- x y )
    ! Returns top two return-stack items without popping; deeper item first.
    get-frs [ fstack-second ] [ fstack-peek ] bi ; inline

: s-reverse ( ... k -- ... )   ! Reverse top k stack items
    ! Reverse the top k stack items via get-datastack/set-datastack.
    get-datastack   ! ( k ds )  — ds ends with [..., x[k-1], ..., x0, k]
    swap 1 +        ! ( ds k+1 )
    cut*            ! ( head tail ) — tail = last k+1 elems = [x[k-1],...,x0,k]
    but-last        ! ( head [x[k-1],...,x0] )
    reverse         ! ( head [x0,...,x[k-1]] )
    append          ! ( new-ds )
    set-datastack ;

! ── Boolean conversion ────────────────────────────────────────────────

: bool>flag ( ? -- n )   -1 0 ? ; inline   ! Factor t/f → Forth -1/0
: flag>bool ( n -- ? )   0 = not ; inline  ! Forth flag → Factor t/f

! ── I/O ─────────────────────────────────────────────────────────────

: emit    ( char -- )   1string write flush ; inline
: key     ( -- char )   read1 ; inline
: cr      ( -- )        nl ; inline
: bl      ( -- 32 )     32 ; inline              ! must precede space
: space   ( -- )        bl emit ; inline
: spaces  ( n -- )      [ space ] times ; inline
: type    ( str -- )    write ; inline         ! takes Factor string

SYNTAX: .(    ! .( text until ) — prints the bracketed text at parse time
    ")" parse-tokens " " join write nl ;

! ── Control ─────────────────────────────────────────────────────────

! execute is provided by Factor's kernel (calls a word/quotation).
: noop     ( -- ) ; inline

! ── Boolean constants ────────────────────────────────────────────────
! ANS Forth: TRUE = -1, FALSE = 0.
! Factor:    TRUE = t,  FALSE = f.
! We expose -1/0 integer forms for bitwise compatibility.

CONSTANT: forth-true  -1
CONSTANT: forth-false 0

! ── Size / cell constants ────────────────────────────────────────────

CONSTANT: cell      8          ! bytes per cell (64-bit)
CONSTANT: char-size 1

: cells   ( n -- bytes )   cell * ; inline
: chars   ( n -- bytes )   ; inline              ! chars = bytes on UTF-8
: cell+   ( a -- a' )      cell + ; inline
: char+   ( a -- a' )      1 + ; inline
: char-   ( a -- a' )      1 - ; inline
: aligned ( n -- n' )      cell 1 - + cell bitnot bitand ; inline
: align   ( -- )           ;   ! no-op in Factor (no raw heap pointer)

! ── Misc ANS ────────────────────────────────────────────────────────

: within  ( n lo hi -- ? )
    ! ANS: lo <= n < hi, using unsigned arithmetic trick.
    over -     ! ( n lo hi-lo )
    rot rot -  ! ( hi-lo n-lo )
    swap       ! ( n-lo hi-lo )
    u< ; inline

: /string  ( str n -- str' )   ! Factor string slice
    tail ; inline

: -trailing  ( str -- str' )
    [ bl = ] trim-tail ; inline

: -leading   ( str -- str' )
    [ bl = ] trim-head ; inline

! ── Limit constants ──────────────────────────────────────────────────

CONSTANT: max-n  0x7FFFFFFFFFFFFFFF
CONSTANT: min-n  -0x8000000000000000
CONSTANT: max-u  0xFFFFFFFFFFFFFFFF
CONSTANT: max-char 255

! ── Miscellaneous ────────────────────────────────────────────────────

: ?negate  ( n flag -- n' )  [ neg ] when ; inline
: 0max     ( n -- n' )  0 max ; inline
: under+   ( n1 n2 n3 -- n1+n3 n2 )  rot + swap ; inline
: third    ( a b c -- a b c a )  pick ; inline  ! 0=TOS,1=NOS,2=3rd → pick 2
