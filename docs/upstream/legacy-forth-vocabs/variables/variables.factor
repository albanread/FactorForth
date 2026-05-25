! forth.variables — Forth-style defining words on Factor's namespace system.
!
! Factor parsing model (KEY):
!   In a script/compilation-unit, literals like `42` are compiled as push
!   instructions into the accumulator (vector), NOT onto the data stack.
!   SYNTAX: words receive the accumulator as their argument.
!
! VARIABLE vs VALUE (ANS Forth semantics):
!   variable counter         ! defines counter; counter pushes an INTEGER BYTE ADDRESS
!   counter @                ! reads the cell at that address  (@ = var@)
!   42 counter !             ! stores 42  (in .fth files — preparser rewrites ! → var!)
!   3 counter +!             ! adds 3 to the cell at counter's address
!   counter cell+ @          ! reads the cell one slot after counter
!
!   0 value score            ! defines score; generates runtime: 0 score-sym set-global
!   score                    ! → 0  (score returns its global value, NOT an address)
!   99 to score              ! generates runtime: 99 score-sym set-global
!   score                    ! → 99
!
!   42 constant answer       ! pops 42 from accum, defines answer to push 42
!
! NOTE: `!` is defined programmatically (Factor's lexer treats `!` as a comment).
!   In .fth files loaded via `forth-load` (forth.preparser), write `!` normally —
!   the text-level rewriter converts it to `var!` before Factor's lexer runs.
!   In raw Factor source, write `var!` directly.

USING: kernel sequences math namespaces words vocabs.parser parser
       combinators arrays byte-arrays io lexer prettyprint vocabs
       effects stack-checker forth.memory ;
IN: forth.variables

! ── Low-level memory ops ─────────────────────────────────────────────
!
! `@` / `!` / `+!` / `-!` work on INTEGER BYTE ADDRESSES (from `variable`).
! `value` words use Factor's namespace (get-global/set-global) directly.

: @   ( addr -- val )    var@ ; inline

! `!` cannot be typed in Factor source — the lexer always treats `!` as a line
! comment before any word definition can intercept it.
!
! Solution: load Forth source files through `forth-load` / `FORTH-LOAD:` from
! forth.preparser, which rewrites `!` → `var!` at the TEXT LEVEL before
! Factor's lexer ever sees the file.  Users write standard ANS Forth `!`
! and the preparser handles it transparently.
!
! `var!` is also available directly as the low-level primitive.
! The `!` word is created programmatically so other compiled words can
! reference it by the canonical ANS Forth name.
<<
    "!" current-vocab create-word
    [ var! ]
    { "val" "addr" } { } <effect>
    define-inline
>>

: +!  ( n addr -- )      tuck var@ + swap var! ; inline
: -!  ( n addr -- )      tuck var@ swap - swap var! ; inline
: ?   ( addr -- )        @ . ;

! ── Helpers ───────────────────────────────────────────────────────────

: (declare-effect) ( word def in out -- )
    ! ( word def in-seq out-seq -- )
    <effect> define-inline ;

: (define-sym-word) ( word -- )
    ! Define word to push itself (like SYMBOL:), declared ( -- val ).
    dup [ ] curry { } { "val" } <effect> define-inline ;

: (define-val-word) ( word -- )
    ! Define word to return its own global, declared ( -- val ).
    dup [ get-global ] curry { } { "val" } <effect> define-inline ;

: (define-2val-word) ( word -- )
    ! Define word to return its own global as two values, declared ( -- lo hi ).
    dup [ get-global first2 ] curry { } { "lo" "hi" } <effect> define-inline ;

! ── variable ─────────────────────────────────────────────────────────
! Creates a word that pushes an integer byte address (ANS Forth compatible).
! The cell at that address in `var-store` holds the variable's value.
!
! Usage:
!   variable counter          ! counter → byte-addr (e.g. 0, 8, 16, …)
!   counter @                 ! → 0  (initial value)
!   42 counter var!           ! store 42  (`var!` is Factor's spelling of `!`)
!   counter @                 ! → 42
!   3 counter +!              ! add 3
!   counter @                 ! → 45
!   counter cell+ @           ! read the next cell (address arithmetic works)

SYNTAX: variable
    scan-token create-word-in    ! ( accum word )
    here                         ! ( accum word addr )
    8 allot                      ! advance here by one cell; addr stays on stack
    0 over var!                  ! initialise the cell to 0
    [ ] curry                    ! ( accum word quot-that-pushes-addr )
    { } { "addr" } <effect>
    define-inline ;              ! ( accum )

! `lo hi 2variable name` — allocates two consecutive cells.
! name pushes the base address; lo lives at addr, hi at addr+8.
SYNTAX: 2variable
    scan-token create-word-in    ! ( accum word )
    here                         ! ( accum word addr )
    16 allot                     ! advance here by two cells
    0 over var!                  ! init lo cell
    0 over 8 + var!              ! init hi cell (0 over 8+ = addr+8)
    [ ] curry
    { } { "addr" } <effect>
    define-inline ;

! Read / write 2variable (lo at addr, hi at addr+8):
: 2@  ( addr -- lo hi )  dup var@ swap 8 + var@ ; inline
: 2!  ( lo hi addr -- )  tuck 8 + var! var! ; inline

! ── constant ─────────────────────────────────────────────────────────
! Forth-style: `42 constant answer` — pops 42 from accum, defines answer.
! In Factor's parse model, `42` is in the accumulator (not data stack).
! `over pop` removes it from the accum and creates a static word definition.

SYNTAX: constant
    scan-token create-word-in    ! ( accum word )
    over pop                     ! ( accum word n ) — pop compile-time value from accum
    [ ] curry { } { "val" } <effect> define-inline ;   ! define word to push n: ( accum )

! `lo hi 2constant name` — pops hi then lo from accum.
SYNTAX: 2constant
    scan-token create-word-in    ! ( accum word )
    over pop                     ! ( accum word hi ) — pop hi (last added to accum)
    pick pop                     ! ( accum word hi lo ) — pick accum (depth 2), pop lo
    swap 2array                  ! ( accum word {lo hi} ) — lo at [0], hi at [1]
    [ first2 ] curry { } { "lo" "hi" } <effect> define-inline ;   ! ( accum )

! ── value / to ───────────────────────────────────────────────────────
! `value` creates a word whose body reads from its own NAMESPACE global.
! Unlike `variable`, no address is exposed — the value is returned directly.
! `to` updates that namespace entry at runtime.
!
! Key: `0 value score` defines score AND generates runtime code:
!   0 <score-wrapper> set-global
! The `0` stays in the accum as a runtime push; `value` appends the
! wrapper literal and set-global. Effect: ( accum -- accum ).

SYNTAX: value
    scan-token create-word-in              ! ( accum word )
    [ (define-val-word) ] keep            ! define word (returns global), keep ref
    <wrapper> over push                   ! compile: literal push of word sym
    \ set-global over push ;              ! compile: set-global call

! `lo hi 2value name` — generates: lo hi 2array name-sym set-global
SYNTAX: 2value
    scan-token create-word-in
    [ (define-2val-word) ] keep           ! define word (returns lo hi from global), keep ref
    \ 2array over push                    ! compile: 2array (lo hi -> {lo hi})
    <wrapper> over push                   ! compile: literal push of word sym
    \ set-global over push ;              ! compile: set-global call

! `to` generates runtime code: <word-sym-literal> set-global
! Uses <wrapper> so the sym is pushed as a LITERAL (not a call to the word).
! Works only with `value` / `2value` (namespace-backed words).
SYNTAX: to
    scan-token current-vocab lookup-word
    <wrapper> over push          ! compile: literal push of sym
    \ set-global over push ;     ! compile: set-global call

! ── defer / is / action-of ───────────────────────────────────────────
! `defer` creates a word whose behaviour is set later with `is`.

: (defer-err) ( -- )
    "deferred word called before IS" throw ;

! `defer` creates a word with an error-thrower placeholder.
! `is` REDEFINES the word at parse-time (pops quotation from accum and calls define).
! This avoids the "call on runtime value" issue with global-based dispatch.

SYNTAX: defer
    scan-token create-word-in    ! ( accum word )
    [ (defer-err) ] define ;     ! define with error-thrower: ( accum )

! `is` — parse-time redefine: pops quotation from accum, defines the word.
! Usage:  [ 2 * ] is myop   — makes myop behave as [ 2 * ]
! Uses `infer` to get the quotation's effect so `define-declared` can mark
! myop as compilable (plain `define` leaves effect>> unset → not-compiled).
SYNTAX: is
    scan-token current-vocab lookup-word    ! ( accum word )
    over pop                               ! ( accum word quot ) — pop [quot] from accum
    dup infer                              ! ( accum word quot effect )
    define-declared ;                      ! redefine with declared effect: ( accum )

! `action-of` — push word object itself as a literal (word IS the action after `is`)
SYNTAX: action-of
    scan-token current-vocab lookup-word
    <wrapper> over push ;        ! compile: literal push of word (the action)

: defer@ ( xt -- xt' )   get-global ;
: defer! ( xt' xt -- )   set-global ;

! ── create ────────────────────────────────────────────────────────────
! ANS Forth CREATE: defines a word that pushes the current `here` address.
! Subsequent `,` (from forth.memory) / allot calls lay down data.
! Usage:
!   create my-table  8 allot      ! reserve 1 cell after my-table's address
!   42 my-table var!              ! store 42 there
!   my-table @                    ! → 42

SYNTAX: create
    scan-token create-word-in    ! ( accum word )
    here                         ! ( accum word addr ) — snapshot current here
    [ ] curry                    ! ( accum word quot-that-pushes-addr )
    { } { "addr" } <effect>
    define-inline ;              ! ( accum )

! ── buffer: ───────────────────────────────────────────────────────────
! Creates a word that pushes a byte-array buffer of n bytes.
! Usage:  256 buffer: scratch-buf

SYNTAX: buffer:
    scan-token create-word-in    ! ( accum word )
    over pop                     ! ( accum word n ) — pop size from accum
    <byte-array>                 ! ( accum word byte-array )
    [ ] curry { } { "buf" } <effect> define-inline ;   ! define word to push buffer: ( accum )

! ── marker ────────────────────────────────────────────────────────────

SYNTAX: marker
    current-vocab vocab-words clone
    scan-token create-word-in
    swap [ ] curry { } { "words" } <effect> define-inline ;
! TODO: executing the marker word should restore vocab to snapshot.
