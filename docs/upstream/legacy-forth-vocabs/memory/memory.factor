! forth.memory — Addressable cell store for ANS Forth variable compatibility.
!
! Provides a large Factor array addressed by byte offsets (multiples of 8).
! This lets `variable` expose an integer address so that `@`, `!`, `cell+`,
! `cells`, and address arithmetic all work exactly as in ANS Forth.
!
! Special-object slots used (see fstack.factor — slot 82 is fstack):
!   83 — var-store  : backing array (4096 object slots = 32 KB logical)
!   84 — var-here   : byte offset of next free cell (always a multiple of 8)
!
! Address model:
!   byte-addr = slot-index * 8
!   addr >> 3 = slot-index (or addr /i 8 for integer division)
!   var@ ( addr -- val ) :  addr/8 → index → nth-unsafe in store
!   var! ( val addr -- ) :  addr/8 → index → set-nth-unsafe in store
!
! KEY:  set-nth-unsafe ( elt i seq -- ) — elt=val (deepest), i=idx, seq=store (TOS)
!       With stack `val idx store` after `var-store`, this maps correctly. ✓
!
! COMPILER NOTE: Like fstack (slot 82), the literal slot numbers 83 / 84 trigger
!   compiler.cfg.intrinsics.misc emit-special-object — a single VM-field load,
!   much cheaper than a hashtable lookup. Always use the literal integer, not
!   a named constant, so the intrinsic fires.

USING: kernel kernel.private sequences.private arrays math ;
IN: forth.memory

! ── Backing store accessors ───────────────────────────────────────────

: var-store ( -- arr )  83 special-object ; inline
: var-here  ( -- n )    84 special-object ; inline
: var-here! ( n -- )    84 set-special-object ; inline

! ── Cell read / write ─────────────────────────────────────────────────
!
! `8 /i` converts a byte address to a slot index (integer divide by 8).
! We don't use `-3 shift` because `shift` lives in math.bitwise which
! may not be available in all load contexts; `/i` is in core `math`. ✓

: var@  ( addr -- val )   8 /i var-store nth-unsafe ; inline
: var!  ( val addr -- )   8 /i var-store set-nth-unsafe ; inline

! ── Memory model words ────────────────────────────────────────────────

: here    ( -- addr )   var-here ; inline
: allot   ( n -- )      var-here + var-here! ; inline
: unused  ( -- n )      var-store length 8 * var-here - ; inline

! ── Cell-size helpers ─────────────────────────────────────────────────

: cell    ( -- 8 )      8 ; inline
: cells   ( n -- n*8 )  8 * ; inline
: cell+   ( addr -- addr' ) 8 + ; inline
: cell-   ( addr -- addr' ) 8 - ; inline
: char+   ( addr -- addr' ) 1 + ; inline
: chars   ( n -- n )    ; inline          ! 1 char = 1 byte in this model

! ── Initialise backing store at load time ────────────────────────────
!
! Guard: only allocate once. If slot 83 is already set (non-f), skip.
! Called at parse time when the vocab is first loaded.

83 special-object [
    4096 0 <array>  83 set-special-object
    0               84 set-special-object
] unless
