! forth.structures — Forth 2012 structure words.
!
! BEGIN-STRUCTURE / END-STRUCTURE / FIELD: / CFIELD: / 2FIELD: / +FIELD
!
! Usage:
!   begin-structure point
!     field:  .x
!     field:  .y
!   end-structure
!
!   point       ! → 16  (total byte size)
!
! Stack discipline during struct definition:
!   begin-structure leaves ( size-box 0 )
!   Each field word leaves ( size-box new-offset )
!   end-structure consumes both, fills size-box

USING: arrays boxes byte-arrays combinators kernel lexer math
       namespaces parser sequences words ;
IN: forth.structures

! ── Structure definition ─────────────────────────────────────────────

SYNTAX: begin-structure  ! ( -- size-box 0 )
    ! Create the named word and push the size-box + initial offset.
    scan-token create-word-in   ! ( -- word )
    <box>                       ! ( word size-box )
    dup [ box> ] curry          ! ( word size-box [ size-box box> ] )
    rot define                  ! ( size-box ) — word defined to return size
    0 ;                         ! ( size-box 0 )

: end-structure  ( size-box offset -- )
    swap >box ;                 ! fill the box with the final struct size

! ── +field — low-level field definer ─────────────────────────────────
! Regular word (not SYNTAX:) so field:, cfield:, 2field: can call it.
! Reads the field name from the input stream via scan-token.
!
! Stack: ( size-box offset field-size -- size-box new-offset )

: +field  ( size-box offset field-size "<spaces>name" -- size-box offset' )
    over                         ! ( size-box offset field-size offset )
    scan-token create-word-in    ! ( size-box offset field-size offset word )
    swap [ + ] curry define      ! ( size-box offset field-size ) — word: ( base -- base+offset )
    + ;                          ! ( size-box offset+field-size )

! ── Field defining words ─────────────────────────────────────────────
! These are SYNTAX: so they run at parse time (consuming names from stream).
! They align the offset appropriately then call +field.

SYNTAX: field:   ! ( size-box offset -- size-box offset' ) cell-aligned, 8 bytes
    7 + -8 bitand   ! align offset to 8
    8 +field ;

SYNTAX: cfield:  ! ( size-box offset -- size-box offset' ) byte, no alignment
    1 +field ;

SYNTAX: 2field:  ! ( size-box offset -- size-box offset' ) double-cell, 16 bytes
    7 + -8 bitand   ! align to 8
    16 +field ;

! ── Field access helpers ──────────────────────────────────────────────
! Fields return integer offsets. For byte-array structs:

: field@  ( buf offset -- val )   swap nth ;        ! read byte at offset
: field!  ( val buf offset -- )   swap set-nth ;    ! write byte at offset

! ── Allocation ───────────────────────────────────────────────────────

: struct-new  ( size -- buf )   <byte-array> ;   ! zeroed byte-array
