! forth.preparser — text-level rewriter that runs before Factor's lexer.
!
! THE PROBLEM:
!   Factor's lexer treats `!` as a line-comment start unconditionally.
!   Factor has no Forth-style IF/ELSE/THEN or BEGIN/WHILE/REPEAT control syntax.
!   Both problems are the same kind: source text that Factor cannot parse as-is.
!
! THE SOLUTION:
!   Read the source file as raw text, walk tokens left-to-right with a small
!   amount of state (a control-structure stack), rewrite each token, and
!   hand the result to Factor's compiler.  Factor never sees the Forth tokens.
!
! TOKEN REWRITES:
!
!   Forth token   → Factor token(s)          Notes
!   ──────────────────────────────────────────────────────────────────
!   !             → var!                     store word
!
!   IF            → [                        open true-branch quotation
!   ELSE          → ] [                      close true, open false
!   THEN (no else)→ ] when                   close true, emit when
!   THEN (+else)  → ] if                     close false, emit if
!
!   BEGIN         → [                        open loop-body quotation
!   WHILE         → ] [                      close cond-quot, open body-quot
!   REPEAT        → ] while                  close body-quot, emit while
!   UNTIL         → ] until                  close body+cond-quot, emit until
!   AGAIN         → ] loop                   close body-quot, emit loop (infinite)
!
! Everything else passes through unchanged, including:
!   \ comments (Factor already handles these as backslash-to-eol)
!   ( ) comments (Factor already handles these via the ( word)
!   word definitions  :  ;  SYNTAX:  etc.
!   numbers, strings, vocabulary words
!
! FORTH SOURCE CONVENTIONS (files loaded via forth-load / FORTH-LOAD:):
!   Comments: \  (backslash to end of line) or  ( ... )
!   Control:  IF ELSE THEN  BEGIN WHILE REPEAT  BEGIN UNTIL  BEGIN AGAIN
!   Store:    !  (exactly as in ANS Forth)
!
! MULTILINE SAFETY:
!   Control structures span any number of source lines — the ctrl-stack
!   is preserved across line boundaries within one preprocess-forth call.
!   Newlines are preserved in output so Factor's line-number error messages
!   remain accurate.
!
! KNOWN LIMITATIONS:
!   • `!` inside string literals "hello ! world" will be wrongly rewritten.
!     (Rare in Forth code; a full tokeniser would be needed to handle this.)
!   • DO / LOOP / +LOOP / LEAVE / I / J not yet implemented.
!   • EXIT (early return from word) not yet implemented.
!   • CASE / OF / ENDOF / ENDCASE not yet implemented.

USING: kernel sequences strings splitting namespaces
       io.files io.encodings.utf8 eval ;
IN: forth.preparser

! ── Control-structure stack ───────────────────────────────────────────
!
! A fresh vector is allocated per preprocess-forth call.
! Stack entries are strings marking the open control structure:
!   "IF"       — IF seen, no ELSE yet
!   "IF-ELSE"  — IF + ELSE seen, awaiting THEN
!   "BEGIN"    — BEGIN seen, awaiting WHILE/UNTIL/AGAIN
!   "WHILE"    — BEGIN + WHILE seen, awaiting REPEAT

SYMBOL: forth-ctrl-stack

: ctrl-get  ( -- v )   forth-ctrl-stack get-global ; inline
: ctrl-push ( s -- )   ctrl-get push ;
: ctrl-pop  ( -- s )   ctrl-get pop ;

! ── Token rewriter ────────────────────────────────────────────────────
!
! Rewrites one whitespace-delimited token.
! Multi-token output ("] if", "] when", etc.) is returned as a single string
! containing an embedded space; after " " join the space produces correct
! Factor tokenisation, since Factor's lexer splits on whitespace.
!
! Cascade of `when` clauses — each checks the CURRENT token so that
! replacements from earlier clauses do not trigger later clauses.

: rewrite-token ( token -- token' )
    ! ── store ──────────────────────────────────────────────────
    dup "!"      = [ drop "var!"                              ] when
    ! ── IF / ELSE / THEN ────────────────────────────────────────
    dup "IF"     = [ drop "IF"      ctrl-push "["             ] when
    dup "ELSE"   = [ drop ctrl-pop drop "IF-ELSE" ctrl-push "] [" ] when
    dup "THEN"   = [ drop ctrl-pop "IF-ELSE" =
                     [ "] if" ] [ "] when" ] if               ] when
    ! ── BEGIN / WHILE / REPEAT / UNTIL / AGAIN ──────────────────
    dup "BEGIN"  = [ drop "BEGIN"   ctrl-push "["             ] when
    dup "WHILE"  = [ drop ctrl-pop drop "WHILE"  ctrl-push "] [" ] when
    dup "REPEAT" = [ drop ctrl-pop drop "] while"             ] when
    dup "UNTIL"  = [ drop ctrl-pop drop "] until"             ] when
    dup "AGAIN"  = [ drop ctrl-pop drop "] loop"              ] when ;

! ── Line and file processing ──────────────────────────────────────────

: rewrite-line ( line -- line' )
    "\t" " " replace           ! normalise tabs → spaces
    " "  split                 ! tokenise on spaces
    [ empty? ] reject          ! remove empty strings from consecutive spaces
    [ rewrite-token ] map      ! rewrite each token
    " " join ;                 ! rejoin (embedded spaces expand correctly)

: preprocess-forth ( str -- str' )
    ! Allocate a fresh ctrl-stack; it persists across all lines in this call.
    V{ } clone forth-ctrl-stack set-global
    "\n" split                 ! split on newlines (preserved in output)
    [ rewrite-line ] map       ! process each line
    "\n" join ;                ! rejoin with newlines

! ── Public entry points ───────────────────────────────────────────────

: forth-load ( path -- )
    ! Read a Forth source file, rewrite tokens, compile and run.
    ! Resets the control-structure stack for a fresh file context.
    utf8 file-contents preprocess-forth eval ;

SYNTAX: FORTH-LOAD:
    ! Convenience syntax: scan the next token as a file path and load it.
    ! Usage:   FORTH-LOAD: myapp.fth
    scan-token forth-load ;

! ── REPL support ──────────────────────────────────────────────────────────
!
! For interactive use the ctrl-stack must PERSIST across separate input
! lines so that multi-line word definitions typed line-by-line work.
!
! `repl-reset`    — start a fresh ctrl-stack (call once per session).
! `preprocess-str — preprocess multi-line input without resetting.
! `forth-eval`    — transpile + eval; ctrl-stack persists across calls.

: repl-reset ( -- )
    ! Reset the ctrl-stack to empty (call when starting a new session).
    V{ } clone forth-ctrl-stack set-global ;

: (ensure-ctrl-stack) ( -- )
    ! Initialise ctrl-stack lazily if not yet present.
    forth-ctrl-stack get-global [ repl-reset ] unless ;

: preprocess-str ( str -- str' )
    ! Preprocess Forth source WITHOUT resetting the ctrl-stack.
    ! Suitable for REPL use; ctrl-stack persists across calls.
    (ensure-ctrl-stack)
    "\n" split
    [ rewrite-line ] map
    "\n" join ;

: forth-eval ( str -- )
    ! Evaluate one or more lines of Forth syntax (REPL mode).
    ! The ctrl-stack persists across calls; call `repl-reset` to restart.
    preprocess-str eval ;
