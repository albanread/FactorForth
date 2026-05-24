! forth.runtime — NewFactor's runtime support vocab.
!
! ARCHITECTURE
!
!   The Rust ANS Forth compiler (src/compiler/) emits canonical Factor
!   source as its internal IR.  That IR resolves word references in
!   exactly three places:
!
!     1. Factor's built-in vocabs (kernel, math, sequences, ...) for
!        words that already exist with the right semantics — DUP, DROP,
!        SWAP, +, *, etc.  The Rust resolver renames ANS-name → Factor
!        name during emission; no aliases needed in this vocab.
!
!     2. forth.runtime (this file) for words ANS Forth needs that
!        Factor doesn't directly express, or expresses with different
!        semantics.  Memory model, return stack, ANS-style I/O, ANS
!        booleans, ANS-floored mod, do-loop machinery with i/j.
!
!     3. forth.wf64-gfx for host-callback words exposed by newfactor-ui
!        (gpane-*, ev-*).  Separate vocab; see factor/wf64-gfx/.
!
! BUDGET: this file targets 200-400 lines.  If it grows past 600,
!         re-evaluate per word whether the Rust resolver could handle
!         it via direct rename instead.
!
! STATUS: Phase 1 first draft.  Each word's body is a sketch; the real
!         implementations land as the compiler comes online and we
!         verify each against ANS test fixtures.

USING: accessors alien alien.accessors alien.c-types alien.data
       arrays byte-arrays continuations io kernel kernel.private
       math math.bitwise math.functions math.order math.parser
       namespaces prettyprint.config sequences sequences.private
       strings ;
IN: forth.runtime

! ── 1. CELL-ADDRESSED MEMORY MODEL ───────────────────────────────────────
!
! ANS Forth's memory model assumes a linear byte-addressable space with
! cells (8 bytes here, 64-bit cell).  Factor has no such space —
! everything is GC'd objects.  We bridge by representing an "address"
! as a (byte-array, offset) tuple.  All addresses Forth ever sees flow
! through this representation; arithmetic on addresses (CELL+, CHARS,
! etc.) operates on the offset while keeping the byte-array stable.
!
! What this gives us: every portable ANS program that only uses addresses
! Forth gave it (from VARIABLE, CREATE, HERE+ALLOT) just works.
! What this does NOT give us: programs that POKE/PEEK arbitrary integer
! addresses (e.g. HEX 8000 @ for hardware registers).  Those are out
! of scope; use the FFI instead.

TUPLE: nf-addr
    { ba   byte-array read-only }   ! the backing storage
    { off  integer    read-only } ; ! byte offset into ba

: <nf-addr> ( ba off -- addr ) nf-addr boa ; inline

! Address arithmetic.  An ANS address can have an integer added or
! subtracted to it (within the same allocation), staying inside the
! original byte-array.
: nf-addr+ ( addr n -- addr' )
    [ off>> + ] [ ba>> ] bi swap <nf-addr> ; inline

: cell+ ( addr -- addr' )  8 nf-addr+ ; inline
: char+ ( addr -- addr' )  1 nf-addr+ ; inline
: cells ( n -- bytes )     8 * ; inline
: chars ( n -- bytes )     ; inline  ! UTF-8: chars are byte-counted

! ── ANS fetch/store ──
!
! 64-bit (cell) and 8-bit (char) loads/stores against an nf-addr.
! Implemented via Factor's alien.c-types primitives on the byte-array's
! data pointer.  byte-arrays are pinned (Factor doesn't move them once
! data is taken from them via alien-address), but for safety we go
! through set-alien-unsigned-cell which Factor's compiler knows is
! safe under GC.
!
! NAMING: ANS Forth's `!` (store) clashes with Factor's line-comment
! syntax, so the Factor-side word is `nf-!`.  The Rust compiler emits
! the rename when it sees ANS `!` in source.  Same for `c!` → `nf-c!`
! and `+!` → `nf-+!` for symmetry, even though `c!` and `+!` would
! parse OK in isolation (`!` is only a comment as a standalone token).
! Keeping the prefix uniform makes the resolver table cleaner.
!
! ANS `@` and `c@` parse fine in Factor, so they keep their names.

! NB: alien-unsigned-cell takes ( c-ptr n -- value ) and
! set-alien-unsigned-cell takes ( value c-ptr n -- ).  The bi order
! below produces ( ba off ) on top, matching both signatures.

: @ ( addr -- n )
    [ ba>> ] [ off>> ] bi alien-unsigned-cell ; inline

: nf-! ( n addr -- )
    [ ba>> ] [ off>> ] bi set-alien-unsigned-cell ; inline

: c@ ( addr -- ch )
    [ ba>> ] [ off>> ] bi alien-unsigned-1 ; inline

: nf-c! ( ch addr -- )
    [ ba>> ] [ off>> ] bi set-alien-unsigned-1 ; inline

: nf-+! ( n addr -- )
    [ @ + ] [ nf-! ] bi ; inline

! ── Raw native pointer (for FFI) ──
!
! An nf-addr passed across the FFI boundary collapses to a single
! u64: the byte-array's pinned data pointer plus the offset.  Factor
! byte-arrays are pinned by the GC; the pointer is stable for the
! duration of the call.  CALLER MUST keep the original nf-addr live
! across the call (Factor's compiler does this automatically when the
! nf-addr is on the data stack).
!
! Used by forth.wf64-gfx (gpane-open's title pointer) and any future
! ANS Forth FFI helpers.

: nf-addr-raw ( addr -- u64 )
    [ ba>> >c-ptr alien-address ] [ off>> ] bi + ; inline

! ── Variable / constant cell allocation ──
!
! The Rust compiler emits a call to <variable> for each VARIABLE
! declaration in the source.  The result is a fresh 1-cell address
! whose contents start as zero.  Allocation happens at parse time
! (Rust ensures the byte-array lives for the program's lifetime);
! at runtime, the VARIABLE word just pushes the address.

: <variable> ( -- addr )
    8 <byte-array> 0 <nf-addr> ; inline

: <buffer> ( n-bytes -- addr )
    <byte-array> 0 <nf-addr> ; inline

! ── 2. RETURN STACK (>R / R> / R@ / RDROP) ───────────────────────────────
!
! Reuses the fstack trick from the archived forth.fstack vocab: a
! fixed-capacity tuple-backed stack pinned to a Factor special-object
! slot.  ~8 instructions per push/pop after the optimising compiler's
! intrinsic fires on the literal slot index.  See
! docs/upstream/legacy-forth-vocabs/fstack/fstack.factor for the
! analysis and the bypassed-SYMBOL: rationale.

TUPLE: fstack { storage array read-only } top ;

: <fstack> ( capacity -- fstack )
    f <array> 0 fstack boa ; inline

! Slot 82 was chosen as the first unused special-object index
! (special_object_count = 85; highest named = 81).  No Factor source
! references 82 special-object, so this is collision-free.
! Lazy init.  save-image-and-exit zeros special-object slots
! outside the OBJ_STARTUP_QUOT..OBJ_BIGNUM_NEG_ONE range (=81), so
! 82 comes back as `f` after image reload.  Same story for slot 83
! below.
: get-frs ( -- fstack )
    82 special-object [ 256 <fstack> dup 82 set-special-object ] unless* ; inline
: set-frs ( fs -- )       82 set-special-object ; inline

:: fstack-push ( x fs -- )
    fs top>> :> n
    x n fs storage>> set-nth-unsafe
    n 1 + fs top<< ;

:: fstack-pop ( fs -- x )
    fs top>> 1 - :> n
    n fs top<<
    n fs storage>> nth-unsafe ;

: fstack-peek ( fs -- x )
    [ top>> 1 - ] [ storage>> ] bi nth-unsafe ; inline

: fstack-drop ( fs -- )
    [ top>> 1 - ] keep top<< ; inline

! Public ANS-name return-stack words.
: >r    ( x -- )  get-frs fstack-push ; inline
: r>    ( -- x )  get-frs fstack-pop  ; inline
: r@    ( -- x )  get-frs fstack-peek ; inline
: rdrop ( -- )    get-frs fstack-drop ; inline
: 2>r   ( a b -- )  swap >r >r ; inline
: 2r>   ( -- a b )  r> r> swap ; inline

! ── 3. DO/LOOP MACHINERY ─────────────────────────────────────────────────
!
! ANS Forth's DO/LOOP exposes the index via I (innermost) and J (next
! outer).  Loop frames live on a separate stack so I/J can be word
! references that look up the current index at runtime regardless of
! call depth.  This is the ANS-faithful semantic: I works correctly
! inside any quotation invoked from the loop body, even via EXECUTE.
!
! Uses special-object slot 83 for the loop-frame stack.  Same idiom
! as the return stack above; same performance profile.

! `left?` is set by LEAVE; bump-loop checks it and returns done?=t,
! exiting the loop cleanly at the next iteration boundary.
TUPLE: loop-frame
    { limit integer }
    { index integer }
    { left? boolean } ;

! Lazy initialisation:
!   `save-image-and-exit` zeros every special-object slot outside a
!   hardcoded range that maxes out at OBJ_BIGNUM_NEG_ONE (=81), so
!   our slots 82 and 83 come back as `f` after image reload.  The
!   parse-time init we used to do at the bottom of the file only
!   fires during build-image, not after the saved image loads.
!   Solution: lazy init inside the getter via `unless*` — if the
!   slot is f, allocate a fresh fstack, install it, and return it.
: get-loop-frames ( -- fs )
    83 special-object [ 32 <fstack> dup 83 set-special-object ] unless* ; inline
: set-loop-frames ( fs -- )   83 set-special-object ; inline

! Internal: push/pop a loop frame.  `f` is the initial value of the
! `left?` field — no LEAVE has fired yet.
: push-loop-frame ( limit start -- )
    f loop-frame boa get-loop-frames fstack-push ; inline

: pop-loop-frame ( -- )
    get-loop-frames fstack-drop ; inline

! I returns the innermost loop's current index.  Constant-time slot read.
: i ( -- n )
    get-loop-frames fstack-peek index>> ; inline

: j ( -- n )
    get-loop-frames [ top>> 2 - ] [ storage>> ] bi nth-unsafe index>> ; inline

! Bumping the innermost index by +1 (LOOP) or +n (+LOOP).  Checks
! the `left?` flag first — if LEAVE has fired, report done? = true
! immediately without bumping further.
:: bump-loop ( delta -- done? )
    get-loop-frames fstack-peek :> frame
    frame left?>>
    [ t ]
    [
        frame index>> delta +  :> new-index
        new-index frame index<<
        new-index frame limit>> >=
    ] if ;

! LEAVE — mark the innermost loop frame as "ready to exit".
!
! Earlier drafts used throw+recover or with-return+return for a
! non-local exit.  Both of those mechanisms restore the data stack
! to a SAVED state on return — which discards the accumulator the
! body has been building up across iterations.  ANS-faithful LEAVE
! must preserve the data stack.
!
! Flag-based LEAVE is the clean alternative: set `left?` on the
! frame, body completes its current iteration to a natural end,
! bump-loop sees the flag and exits the loop.  Trade-off: any
! body code AFTER `LEAVE` in the same iteration still runs.  The
! common ANS idiom places LEAVE at the end of an IF at the end of
! the loop body, where this distinction doesn't matter:
!
!     do  body  some-cond if leave then  loop
!
! Mid-body LEAVE in the form `do  some-cond if leave then  body  loop`
! will execute `body` once more for the leave-firing iteration.
! Document this in the user guide if it becomes a real issue.
: leave ( -- )
    get-loop-frames fstack-peek t >>left? drop ;

! do-loop:  ( limit start quot -- )
! Runs quot repeatedly with i providing the current index.  Each
! call of `quot` MUST leave the step amount on the stack as its
! final action — the compiler injects `1` for plain LOOP and the
! user's step expression for +LOOP.  bump-loop consumes the step,
! bumps the index, returns done?.  We continue while NOT done.
! LEAVE escapes early via with-return.
!
! `inline` is required for two reasons:
!   1. Factor's compiler refuses `quot call` if the surrounding word
!      isn't inline — the quotation argument can't escape into a
!      non-inline frame because the compiler needs to know the
!      quotation's identity at compile time to inline-expand it.
!   2. Callers of do-loop want effect inference to see through it.
!      With `inline` the call-site sees the full body and can
!      derive the effect from the literal quotation passed in.
!
! LEAVE just sets a flag now, so the loop always exits via the
! normal "bump-loop returned done?" path.  pop-loop-frame runs
! after the loop on every exit.
::  do-loop  ( limit start quot -- )
    limit start push-loop-frame
    [ quot call bump-loop not ] kernel:loop
    pop-loop-frame ; inline

! ?do-loop:  ( limit start quot -- )
! ANS ?DO: skip the loop entirely if limit equals start.  Standard
! DO would run once even when bounds equal (an ANS-94 footgun
! `?DO` exists to dodge); we mirror that.
::  ?do-loop  ( limit start quot -- )
    limit start = [ ] [ limit start quot do-loop ] if ; inline

! ── 4. ANS-STYLE I/O ─────────────────────────────────────────────────────
!
! ANS . prints a signed integer in the current BASE followed by a
! single space.  Factor's prettyprint:. is the debugger pretty-printer
! (different output for collections, no trailing space).  We define
! our own.  number-base is the global from prettyprint.config that
! HEX, DECIMAL, OCTAL, BINARY mutate.

USE: prettyprint.config       ! for number-base

: . ( n -- )
    number-base get >base write " " write ; inline

: u. ( u -- )
    number-base get >base write " " write ; inline   ! unsigned uses same path on 64-bit

: cr ( -- )       "\n" write ; inline
: emit ( ch -- )  1string write ; inline
: space ( -- )    " " write ; inline
: spaces ( n -- ) [ space ] times ; inline
: type ( str -- ) write ; inline

! ── 5. ANS BOOLEANS (-1 / 0, not Factor's t / f) ─────────────────────────

CONSTANT: forth-true  -1
CONSTANT: forth-false 0

: bool>flag ( ? -- n )  forth-true forth-false ? ; inline
: flag>bool ( n -- ? )  0 = not ; inline

! The Rust compiler wraps every comparison the user writes (= < >, ...)
! in bool>flag so that on the data stack the user sees -1 / 0, matching
! ANS expectations.  IF / WHILE / UNTIL consume the flag and treat
! 0 as false, anything else as true.

! ── 6. ANS-FLOORED MOD (differs from Factor's truncated mod) ─────────────
!
! Factor's `mod` truncates toward zero.  ANS Forth's MOD is floored:
! the sign of the result matches the sign of the divisor.  Worked
! example: -7 MOD 3 is 2 in ANS, -1 in Factor's default mod.

: floored-mod ( a b -- r )
    [ mod ] 2keep
    over 0 < over 0 < xor
    [ drop + ] [ 2drop ] if ;

! Rust resolver maps source-level `MOD` to this word.

! ── 7. STACK WORDS NOT IN FACTOR KERNEL ──────────────────────────────────

: ?dup ( x -- 0 | x x )  dup [ dup ] when ; inline
: depth ( -- n )         get-datastack length ; inline

! Factor kernel has 2dup, 2drop, 2swap, 2over, 2nip, 3dup, 3drop, 4dup.
! Anything else the user writes resolves directly.

! ── 8. NUMBER CONVERSIONS ────────────────────────────────────────────────
!
! Factor's integers are arbitrary-precision and unified, so most
! ANS conversion words collapse to identity or thin wrappers.  We
! provide them for completeness so the Rust resolver has unambiguous
! targets to emit against.

: s>d ( n -- n )        ; inline  ! identity in unified-integer Factor
: d>s ( n -- n )        ; inline
: d>f ( n -- f )        >float ; inline
: f>d ( f -- n )        >integer ; inline

! ── 9. FP ALIASES ────────────────────────────────────────────────────────
!
! Factor's + - * / are polymorphic over int and float.  ANS's f+ f- f* f/
! are float-only.  We alias for naming clarity; the Rust resolver can
! also lower f+ → + directly when the types are known.

ALIAS: f+  +
ALIAS: f-  -
ALIAS: f*  *
ALIAS: f/  /
ALIAS: f<  <
ALIAS: f>  >
ALIAS: f=  =

! ── 10. EXECUTION TOKEN HELPERS ──────────────────────────────────────────
!
! ANS EXECUTE consumes an XT and calls it.  In Factor a "word" is the
! XT equivalent; `execute` already exists in kernel and does the right
! thing for fully-typed call sites.  For unknown-effect XTs from the
! data stack, the Rust compiler emits `call( -- )` (Factor's runtime-
! checked dynamic call).  The wrapper below is only for the
! contexts where the compiler can't make that decision Rust-side.

: ans-execute ( xt -- )  execute( -- ) ; inline


! ── End of forth.runtime ─────────────────────────────────────────────────
!
! Word count: roughly 35 public words.  Re-evaluate budget once the
! Rust resolver is wired and the Hayes test fixtures start exercising
! corner cases (e.g. ABORT, ABORT", CATCH, THROW likely need additions).
