\ streams.f — CoreProtocols, Layer 3: text & streams.
\
\ Load after core.f and collections.f.
\
\ Two things live here:
\   * `string` — a text value type (a darray of character codes) with
\     proper methods: show / size / at / equals?, plus string-append
\     and friends.  Our `CLASS: string` lives in the scratchpad vocab
\     and coexists with Factor's native builtin `string` (we never
\     name that builtin class, so shadowing it in scratchpad is inert).
\   * the STREAM protocol, whose signature idea is that end-of-file is
\     an OBJECT (<eof>), not a flag — read-char returns a char code or
\     the marker, and the read loop dispatches on that.  copy-stream is
\     written ONCE over the protocol and works for any stream.
\
\ Builds on Layer 0 (show / equals?) and Layer 1 (darray, size, at).
\ See docs/coreprotocols.md (Layer 3) for the design.
\
\ NB for lib authors: our compiler emits METHOD bodies BEFORE the
\ file's `:` colon words, so a method may call words from earlier-
\ loaded files (core, collections) and builtins, but NOT a `:` helper
\ defined later in THIS file — that is an unresolved forward reference
\ at load.  Hence read-char inlines its end test rather than calling a
\ helper.

\ ── classes ──────────────────────────────────────────────────────
CLASS: eof-marker ;                       \ the end-of-stream marker
CLASS: string SLOT: chars ;               \ a string: darray of char codes
CLASS: string-reader SLOT: buf SLOT: pos ;
CLASS: string-writer SLOT: buf ;

\ ── the STREAM protocol (new generics) ───────────────────────────
GENERIC: read-char  ( s -- ch|eof )
GENERIC: write-char ( ch s -- )

\ ── methods ──────────────────────────────────────────────────────
\
\ string joins the collection protocol (size / at) and the core
\ protocol (show).  Equality falls through to Layer 0's structural
\ default, which compares the char buffers element-wise.
METHOD: size ( s:string -- n )      string>chars size ;
METHOD: at   ( i s:string -- ch )   string>chars at ;
METHOD: show ( s:string -- )
    string>chars dup size 0 ?DO
        dup I swap at emit
    LOOP drop ;

METHOD: read-char ( s:string-reader -- ch|eof )
    \ done?  pos >= size
    dup string-reader>pos over string-reader>buf size < 0= IF
        drop <eof-marker>
    ELSE
        \ ch := buf[pos]
        dup string-reader>pos over string-reader>buf at   ( s ch )
        \ pos := pos + 1
        swap dup string-reader>pos 1+ over string-reader.pos!  ( ch s )
        drop
    THEN ;

METHOD: write-char ( ch w:string-writer -- )
    string-writer>buf d-push ;

\ ── <eof> helpers ────────────────────────────────────────────────
: eof  ( -- m )    <eof-marker> ;
: eof? ( x -- ? )  <eof-marker> equals? ;

\ ── string construction & ops ────────────────────────────────────
: new-string ( -- s )   new-darray <string> ;

\ Build a string from an ANS string literal.  An ANS c-addr is an
\ nf-addr object, so we can't offset it with integer `+` — walk it one
\ character at a time with char+.
: >string ( c-addr u -- s )
    swap new-darray rot 0 DO
        over c@ over d-push
        swap char+ swap
    LOOP nip <string> ;

: string-push ( ch s -- )   string>chars d-push ;

\ Append every character of src onto dst (in place).
: append-into ( src dst -- )
    over size 0 ?DO
        over I swap at
        over string-push
    LOOP 2drop ;

\ Concatenate two strings into a fresh one.
: string-append ( a b -- c )
    new-string >r
    swap r@ append-into
    r@ append-into
    r> ;

\ ── streams ──────────────────────────────────────────────────────
: <reader> ( buf -- s )  0 <string-reader> ;
: <writer> ( -- w )      new-darray <string-writer> ;

\ Public convenience (the same end test read-char inlines).
: reader-done? ( s -- ? )
    dup string-reader>pos swap string-reader>buf size < 0= ;

\ interop: string <-> streams
: string>reader ( s -- r )   string>chars <reader> ;
: writer>string ( w -- s )   string-writer>buf <string> ;

\ A reader straight from an ANS literal, and printing a writer.
: str>reader ( c-addr u -- r )  >string string>reader ;
: writer-emit ( w -- )          writer>string show ;

\ Read one line (up to a newline, code 10) from an input stream into a
\ fresh string.  The newline is consumed but not included; at <eof> you
\ get whatever was read (empty if the stream was already drained).
: read-line ( in -- s )
    new-string                 ( in s )
    BEGIN
        over read-char         ( in s ch )
        dup eof? IF
            drop -1            ( done at eof )
        ELSE dup 10 = IF
            drop -1            ( done at newline )
        ELSE
            over string-push 0 ( appended; keep going )
        THEN THEN
    UNTIL
    nip ;

\ ── derived protocol words (write ONCE, work for any stream) ──────
\
\ Pump every character from `in` to `out` until <eof>.  Each iteration
\ consumes the char on BOTH branches so the loop body is stack-balanced
\ (the compiler checks branch parity strictly).
: copy-stream ( in out -- )
    BEGIN
        over read-char
        dup eof? IF
            drop -1
        ELSE
            over write-char 0
        THEN
    UNTIL
    2drop ;

\ Drain an input stream into a fresh writer and return it.
: read-all ( in -- w )  <writer> dup >r copy-stream r> ;
