! forth.locals — ANS Forth 2012 {: :} locals word-definition syntax.
!
! Usage:
!   {: wordname a b -- result :}   body... ;
!   {: wordname a b | c -- result :}   body... ;
!       (| c declares an uninitialized mutable local, initialized to 0)
!
! Equivalent Factor spellings:
!   :: wordname ( a b -- result ) body... ;
!   :: wordname ( a b -- result ) 0 :> c  body... ;
!
! DESIGN NOTE: {: is a word-definition form, not a mid-body form.
!   Factor's :> only works inside ::, [let, or [| — all of which
!   require the lambda scope set up by with-lambda-scope.  {: establishes
!   this scope internally; the body (up to ;) has full access to all
!   named locals and may use :> for additional bindings.
!
! SYNTAX:
!   {: name  in1 in2 ...  |  uninit1 uninit2 ...  --  out1 out2 ...  :}
!        body... ;
!
!   • Tokens before | (or -- if no |) → input locals, bound left=deepest right=TOS
!   • Tokens between | and --         → uninitialized locals, initialized to 0
!   • Tokens between -- and :}        → output names (documentation only)
!   • Body ends with ;
!
! HOW IT WORKS (locals.parser internals):
!   1. scan-new-word: creates the word object
!   2. parse-{:}-header: reads {: ... :} tokens, splits on | and --
!   3. inputs >array make-locals: builds (vars assoc) from input names
!   4. <effect>: constructs the stack-effect object
!   5. make-uninit-reader: builds a reader-quot that (when called inside
!      with-lambda-scope) calls parse-def for each uninit name — adding
!      them to the lambda scope — then parses the body, prepending
!      "0 def-c 0 def-d ..." initialization to the compiled quotation.
!   6. (parse-locals-definition): sets up with-lambda-scope, creates the
!      lambda, rewrites closures, returns (word quot effect).
!   7. define-declared: installs the word in the dictionary.

USING: kernel sequences arrays effects locals locals.parser
       effects.parser parser lexer words fry quotations ;
IN: forth.locals

<PRIVATE

! ── Header token parsing ──────────────────────────────────────────────

: split-at-token ( seq elt -- before after )
    ! Split seq at first occurrence of elt (elt not included in either part).
    ! Returns ( seq { } ) if elt not found.
    !
    ! index ( obj seq -- n/f ): obj=NOS, seq=TOS.
    ! We need elt as obj and seq as the sequence, so:
    !   2dup swap  →  seq elt seq elt  →  swap  →  seq elt elt seq
    !   index finds elt in seq → result: seq elt n/f
    !   if found:  nip cut rest  →  seq n cut rest  →  before after
    !   if not:    2drop { }     →  seq then drop elt → {}
    2dup swap index
    [ nip cut rest ] [ 2drop { } ] if ;

: parse-{:}-header ( -- inputs uninits outputs )
    ! Reads lexer tokens up to :} and partitions on | and --.
    ! Returns: inputs (from stack), uninits (start at 0), outputs (docs).
    ":}" parse-tokens
    "|" split-at-token         ! ( before-bar after-bar )
    dup empty? [
        ! No | found: before-bar contains inputs and possibly -- outputs
        drop "--" split-at-token  ! ( inputs outputs )
        { } swap                   ! ( inputs { } outputs )
    ] [
        ! | found: before-bar=inputs, split after-bar on --
        "--" split-at-token        ! ( inputs uninits outputs )
    ] if ;

! ── Uninitialized-local reader-quotation ─────────────────────────────
!
! When (parse-locals-definition) calls the reader-quot inside
! with-lambda-scope, that call runs at parse time with the lambda scope
! active.  For each uninit name "c" we call parse-def directly, which:
!   (a) creates a <local> word for "c"
!   (b) wraps it in a <def> object (the compile-time "store" token)
!   (c) calls update-locals — adding "c" to the current lambda scope
!       so the body can reference it
! We then build a combined quotation: { 0 def-c 0 def-d ... body... }
! The locals rewriter sees the <def> objects and emits load-local for each.

: make-uninit-reader ( uninit-names -- reader-quot )
    '[ _                                ! push uninit-names
       [ parse-def ] map                ! ( uninit-defs ) — also adds locals to scope
       parse-definition                 ! ( uninit-defs body )
       swap                             ! ( body uninit-defs )
       [ { 0 } swap suffix ] map        ! ( body { {0 def-c} {0 def-d} ... } )
       concat                           ! ( body prefix )
       swap append                      ! ( prefix ++ body as sequence )
       >quotation ] ;                   ! ( combined-quot )

! ── Main definition builder ───────────────────────────────────────────

:: parse-{:} ( -- word def effect )
    [
        scan-new-word                   ! ( word )
        parse-{:}-header               ! ( word inputs uninits outputs )
        :> outputs :> uninits :> inputs
        inputs >array make-locals      ! ( word words assoc )
        :> in-assoc :> in-vars
        inputs >array outputs >array <effect> :> eff
        uninits empty?
            [ [ parse-definition ] ]
            [ uninits make-uninit-reader ] if :> reader-quot
        eff in-vars in-assoc reader-quot
        (parse-locals-definition)      ! ( word quot effect )
    ] with-definition ;

PRIVATE>

! ── Public syntax word ────────────────────────────────────────────────

SYNTAX: {:
    parse-{:} define-declared ;
