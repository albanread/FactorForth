! forth.dictionary — Forth dictionary/vocabulary interface on Factor's word system.
!
! Factor's vocabulary system IS the dictionary.  This vocabulary exposes
! it with Forth-compatible word names.
!
! Words:
!   find        ( str -- xt 1 | xt -1 | str 0 )   ANS find on Factor words
!   '           ( "name" -- xt )                   tick: get XT of word
!   words       ( -- )                             list words in current vocab
!   words-in    ( vocab-name -- )                  list words in named vocab
!   order       ( -- )                             show search order
!   vocabulary  ( "name" -- )                      create a vocabulary
!   get-current ( -- wid )                         current definition vocab
!   set-current ( wid -- )                         switch current vocab
!   >name       ( xt -- str )                      word name from XT
!   name>       ( str -- xt )                      find word by name
!   immediate?  ( xt -- ? )                        is word immediate?
!   dump        ( byte-array -- )                  hex dump byte-array
!   see         ( "name" -- )                      decompile a word

USING: kernel sequences strings io prettyprint
       namespaces words vocabs vocabs.parser combinators arrays
       math math.parser accessors parser lexer ;
IN: forth.dictionary

! ── Word lookup ───────────────────────────────────────────────────────

: name>  ( str -- xt )
    current-vocab lookup-word ; inline

: >name  ( xt -- str )
    name>> ; inline

: find  ( str -- xt 1 | xt -1 | str 0 )
    dup current-vocab lookup-word [
        nip dup primitive? [ -1 ] [ 1 ] if
    ] [
        0
    ] if* ;

! ── Tick ─────────────────────────────────────────────────────────────

! ' compiles the named word as a literal (XT) — pushes word object at runtime.
! <wrapper> in the accum compiles as "push the wrapped word as a literal".
SYNTAX: '
    scan-token current-vocab lookup-word
    <wrapper> over push ;

! ── Listing ──────────────────────────────────────────────────────────

: words-in  ( vocab-str -- )
    lookup-vocab vocab-words
    [ name>> write bl ] each nl ;

: words  ( -- )
    current-vocab vocab-words
    [ name>> write bl ] each nl ;

: forth-words  ( -- )
    "forth.core" words-in ;

: order  ( -- )
    "Search order: " write
    manifest get search-vocabs>>
    [ name>> write bl ] each nl ;

! ── Vocabulary management ─────────────────────────────────────────────

SYNTAX: vocabulary  ( "name" -- )
    scan-token dup create-vocab
    swap create-word-in
    swap [ ] curry define ;   ! word pushes the vocab object

: get-current  ( -- vocab )
    current-vocab ;

: set-current  ( vocab -- )
    set-current-vocab ;

! ── Immediate flag ────────────────────────────────────────────────────

: immediate?  ( xt -- ? )
    "parsing" word-prop ; inline

! ── Decompiler ───────────────────────────────────────────────────────

SYNTAX: see  ( "name" -- )
    scan-token current-vocab lookup-word pprint nl ;

! ── Hex dump ─────────────────────────────────────────────────────────

: dump  ( byte-array -- )
    [   ! ( byte index )
        dup 16 mod 0 = [ nl ] when   ! newline every 16 bytes
        drop                          ! drop index, keep byte
        >hex 2 CHAR: 0 pad-head write bl
    ] each-index nl ;
