! forth.fstack — Fixed-capacity tuple-backed stack, pinned to a VM special-object slot.
!
! WHY THIS EXISTS (vs the original `SYMBOL: r-stk` + `V{ }` approach):
!
!  The old `>r` was:  r-stk get push
!    1. r-stk get  = OBJ-GLOBAL hashtable lookup                (~10 insns)
!    2. push       = growable dispatch → bounds/resize check → write (~13 insns)
!    Total: ~23 instructions
!
!  The new `>r` is:   get-frs fstack-push
!    1. get-frs    = `82 special-object`
!                    The compiler intrinsic `emit-special-object` fires because
!                    82 is a compile-time literal, emitting a single ^^vm-field
!                    load (one indexed read from the VM struct).              (~1 insn)
!    2. fstack-push = tuple slot reads (##slot-imm) + set-nth-unsafe (##set-slot)
!                    + increment (##add-imm) + slot write (##set-slot-imm)   (~7 insns)
!    Total: ~8 instructions, ~3x faster
!
! KEY COMPILER FACTS exploited here:
!   • `kernel.private:special-object` has a compiler intrinsic in
!     compiler.cfg.intrinsics.misc that emits ^^vm-field when its argument
!     is a compile-time literal — one memory access, no hashtable.
!   • Tuple accessors (top>>, top<<, storage>>) are auto-generated `inline`
!     words that compile to ##slot-imm / ##set-slot-imm instructions.
!   • `nth-unsafe` and `set-nth-unsafe` (sequences.private) are `inline` and
!     call slots.private:slot / set-slot, which have ##slot / ##set-slot
!     intrinsics — direct array element access, no bounds check.
!   • Fixed-capacity array (no growable overhead, no fill-count tracking).
!
! SLOT CHOICE: 82 is the first unused special-object slot.
!   kernel.factor: special-object-count = 85, highest named = 81 (OBJ-BIGNUM-NEG-ONE).
!   Slots 82–84 are reserved/unused. No Factor source uses `82 special-object`.

USING: kernel kernel.private combinators sequences.private arrays locals
       accessors math ;
IN: forth.fstack

! ── The fstack type ───────────────────────────────────────────────────────────

TUPLE: fstack { storage array read-only } top ;

: <fstack> ( capacity -- fstack )
    ! f <array> allocates a Factor-managed array (GC-tracked).
    ! top=0 means the stack is empty; push stores at index `top` then increments.
    f <array> 0 fstack boa ; inline

! ── Global access via special-object slot 82 ─────────────────────────────────
!
! Use the literal 82 (not a CONSTANT:) so that `emit-special-object` in
! compiler.cfg.intrinsics.misc sees a compile-time literal and emits a single
! ^^vm-field instruction instead of falling back to an interpreted call.

: get-frs ( -- fstack )   82 special-object ; inline
: set-frs ( fs -- )       82 set-special-object ; inline

! Initialise at vocab load time.
! This is TOP-LEVEL code (not a << block), so it runs AFTER the compilation
! unit finishes — meaning <fstack> and fstack are compiled and callable.
! Guard: only allocate if slot 82 is still f (allows safe vocab reload).
82 special-object [ 256 <fstack> 82 set-special-object ] unless

! ── Push / pop / peek / drop ──────────────────────────────────────────────────
!
! Written with `::` locals for readability; Factor's locals backend compiles
! these to SSA virtual registers — equivalent to hand-crafted stack code.
! Locals compile path:
!   :> n  →  virtual register (no heap allocation for fixnum-range values)
!   top>> →  ##slot-imm (direct tuple slot read)
!   top<< →  ##set-slot-imm (direct tuple slot write)
!   storage>> → ##slot-imm
!   set-nth-unsafe → ##set-slot (direct array element write, no bounds check)
!   nth-unsafe     → ##slot   (direct array element read, no bounds check)

:: fstack-push ( x fs -- )
    fs top>> :> n
    x n fs storage>> set-nth-unsafe    ! storage[n] ← x
    n 1 + fs top<< ;                   ! fs.top ← n + 1

:: fstack-pop ( fs -- x )
    fs top>> 1 - :> n
    n fs top<<                         ! fs.top ← n
    n fs storage>> nth-unsafe ;        ! return storage[n]

! fstack-peek and fstack-drop don't need locals — the combinators keep things clean.

: fstack-peek ( fs -- x )
    ! Returns top of stack without removing it.
    [ top>> 1 - ] [ storage>> ] bi nth-unsafe ; inline

: fstack-drop ( fs -- )
    ! Discards top of stack.
    [ top>> 1 - ] keep top<< ; inline

: fstack-second ( fs -- x )
    ! Returns second-from-top item without popping (for 2r@).
    [ top>> 2 - ] [ storage>> ] bi nth-unsafe ; inline
