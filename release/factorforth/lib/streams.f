\ streams.f — CoreProtocols, Layer 3: text & streams.
\
\ Load after core.f and collections.f.
\
\ The signature idea of this layer: end-of-file is an OBJECT, not a
\ flag.  `read-char` returns either a character code or the <eof>
\ marker, and the read loop dispatches on that instead of testing a
\ separate "did we hit the end?" boolean.  The loop becomes a method
\ table — see copy-stream below, written ONCE over the protocol so it
\ works for any input/output stream you define later.
\
\ Builds on Layer 0 (equals?) and Layer 1 (darray as the buffer).
\ See docs/coreprotocols.md (Layer 3) for the design.

\ ── <eof> — the end-of-stream marker ─────────────────────────────
\
\ An empty class.  We never need a privileged singleton: Layer 0's
\ equals? is structural, so any two eof-marker instances compare
\ equal, and a char (a fixnum) never equals one.  `eof` mints a
\ marker; `eof?` asks "is this the end?".
CLASS: eof-marker ;
: eof  ( -- m )    <eof-marker> ;
: eof? ( x -- ? )  <eof-marker> equals? ;

\ ── the STREAM protocol ──────────────────────────────────────────
\
\ read-char pulls one character code off an input stream, or returns
\ <eof> when drained.  write-char pushes one onto an output stream.
GENERIC: read-char  ( s -- ch|eof )
GENERIC: write-char ( ch s -- )

\ ── string-reader — reads codes out of a buffer, then <eof> ───────
CLASS: string-reader SLOT: buf SLOT: pos ;

\ A reader over a darray of character codes, starting at index 0.
: <reader> ( buf -- s )  0 <string-reader> ;

\ Note: our compiler emits METHOD bodies BEFORE the file's `:` colon
\ words, so a method may call words from earlier-loaded files (core,
\ collections) but NOT a `:` helper defined later in THIS file — that
\ would be an unresolved forward reference at load.  So read-char
\ inlines its end-of-buffer test and its <eof> construction rather
\ than calling reader-done? / eof.
METHOD: read-char ( s:string-reader -- ch|eof )
    \ done?  pos >= size
    dup string-reader>pos over string-reader>buf size < 0= IF
        drop <eof-marker>
    ELSE
        \ fetch buf[pos]
        dup string-reader>pos over string-reader>buf at   ( s ch )
        \ advance pos := pos + 1
        swap dup string-reader>pos 1+ over string-reader.pos!  ( ch s )
        drop
    THEN ;

\ Public convenience (same predicate, callable from user `:` words).
: reader-done? ( s -- ? )
    dup string-reader>pos
    swap string-reader>buf size
    < 0= ;

\ ── string-writer — accumulates codes into a darray ──────────────
CLASS: string-writer SLOT: buf ;

: <writer> ( -- w )  new-darray <string-writer> ;

METHOD: write-char ( ch w:string-writer -- )
    string-writer>buf d-push ;

\ Print a writer's accumulated characters (the toy's "show").
: writer-emit ( w -- )
    string-writer>buf dup size 0 DO
        dup I swap at emit
    LOOP drop ;

\ ── building / draining ──────────────────────────────────────────

\ Turn an ANS string into a string-reader (copies the bytes into a
\ darray buffer).
\ NB: an ANS c-addr here is an nf-addr object, so plain `+` (integer
\ math) can't offset it — walk it one char at a time with `char+`.
: str>reader ( c-addr u -- s )
    swap new-darray rot             ( c-addr d u )
    0 DO                            ( c-addr d )
        over c@ over d-push         ( push c-addr[i] )
        swap char+ swap             ( advance the pointer )
    LOOP
    nip <reader> ;

\ ── derived protocol words (write ONCE, work for any streams) ─────

\ Pump every character from `in` to `out` until <eof>.  The loop is
\ the method table: read-char tells us "char or end" by what it
\ returns, no flag.
\ Each iteration consumes the char on BOTH paths, so the loop body is
\ stack-balanced (Factor's compiler checks branch parity strictly).
: copy-stream ( in out -- )
    BEGIN
        over read-char              ( in out ch )
        dup eof? IF
            drop -1                 ( done: in out -1 )
        ELSE
            over write-char  0      ( wrote it: in out 0 )
        THEN
    UNTIL
    2drop ;

\ Drain an input stream into a fresh writer and return it.
: read-all ( in -- w )
    <writer> dup >r copy-stream r> ;
