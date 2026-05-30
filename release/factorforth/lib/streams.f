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

\ A string is a writable, growable collection too — `new-like` returns
\ a fresh empty string of the same shape, and `at!` delegates to the
\ inner darray.  This is what lets `map` over a string return a string
\ (not a darray of char codes), and what lets `reverse` and the other
\ shape-preserving algorithms work on strings as on any other
\ collection.  `new-like`'s body inlines `new-string`'s expansion
\ (`new-darray <string>`) because methods are emitted before colon
\ defs in the same translation unit, so a forward reference would
\ fail at Factor parse time.
METHOD: new-like ( s:string -- d )  {: _ :}
    new-darray <string> ;
METHOD: at! ( v i s:string -- )     string>chars at! ;

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

\ ── split / join ─────────────────────────────────────────────────
\
\ split breaks a string into a darray of strings on a delimiter
\ character; join glues a darray of strings back together with a
\ (possibly different) delimiter character.  They round-trip:
\   s d split  d join   ==  s
\
\ The delimiter is captured as a local so split / join are re-entrant —
\ a user-defined method that itself calls split is safe inside any
\ field-handler the outer split passes into the protocol.

\ split ( s delim -- coll ).  coll always carries the current field as
\ its last element; a delimiter starts a fresh empty field, any other
\ char extends the last one.  Reads via the stream protocol.
: split ( s delim -- coll ) {: s delim :}
    s string>reader >r                   ( -- ; R: reader )
    new-darray new-string over d-push    ( coll is [ "" ] )
    BEGIN
        r@ read-char                     ( coll ch )
        dup eof? IF
            drop -1
        ELSE dup delim = IF
            drop  new-string over d-push  0
        ELSE
            over dup size 1- swap at string-push  0
        THEN THEN
    UNTIL
    r> drop ;

\ join ( coll delim -- s ).  Append each field; insert the delimiter
\ before every field but the first.
: join ( coll delim -- s ) {: coll delim :}
    new-string                           ( s )
    coll size 0 ?DO
        I 0 > IF  delim over string-push  THEN
        coll I swap at over append-into
    LOOP ;

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

\ ── Text utilities ────────────────────────────────────────────────
\
\ A small kit of string-manipulation words that every Forth user
\ reaches for.  Everything here builds on the collection protocol
\ (size / at / new-like / at!) plus the ASCII character predicates
\ from core.f.  Each word leaves the input untouched and returns a
\ FRESH string — there is no in-place mutation API here.
\
\ Names that read like Forth (rather than Factor): `upcase-string` /
\ `downcase-string` (paralleling Layer 0's char versions), `trim-left`
\ / `trim-right` / `trim`, `starts-with?` / `ends-with?` /
\ `contains?`, `pad-left` / `pad-right`, `repeat-char` /
\ `repeat-string`.

\ subseq ( s start end -- t ) — chars from start (inclusive) to end
\ (exclusive) as a fresh string.  Caller clamps; out-of-range slices
\ would error in `at`.  Used internally by trim and friends.
: subseq ( s start end -- t ) {: s start end :}
    new-string {: dst :}
    end start ?do
        i s at  dst string-push
    loop
    dst ;

\ upcase-string / downcase-string — whole-string ASCII case-flip.
\ `map` returns a string here because `new-like` on a string is a
\ string (the methods we added above) — so the result class matches.
: upcase-string   ( s -- t )  ' upcase-char   map ;
: downcase-string ( s -- t )  ' downcase-char map ;

\ skip-ws-left / skip-ws-right — cursor helpers used by the trim
\ variants.  Each walks past contiguous whitespace and returns the
\ resting index.  Kept separate so the cursor logic is named and
\ obvious, and so the trims read top-down.
: skip-ws-left ( s i -- i' ) {: s i :}
    \ from index i, advance while inside the string AND on whitespace.
    \ Both args bind so the cursor logic reads with names; i is pushed
    \ back onto the data stack and walked there.
    i
    begin
        dup s size < if  dup s at whitespace-char?  else 0 then
    while
        1+
    repeat ;

: skip-ws-right ( s i -- i' ) {: s i :}
    \ from index i, retreat while i > 0 AND s[i-1] is whitespace.
    \ Returns the exclusive end of the non-WS prefix.
    i
    begin
        dup 0 > if  dup 1- s at whitespace-char?  else 0 then
    while
        1-
    repeat ;

: trim-left  ( s -- t ) {: s :}
    s 0 skip-ws-left  {: start :}
    s start s size subseq ;

: trim-right ( s -- t ) {: s :}
    s s size skip-ws-right  {: end :}
    s 0 end subseq ;

: trim ( s -- t ) {: s :}
    s 0 skip-ws-left  {: start :}
    s s size skip-ws-right  {: end :}
    end start < if
        new-string                                  \ all whitespace
    else
        s start end subseq
    then ;

\ substring-at? ( s pos needle -- ? ) — does needle appear in s at
\ offset pos?  The bounds check up front lets the loop assume every
\ index it touches is in range.  Used by contains? and ends-with?.
: substring-at? ( s pos needle -- ? ) {: s pos needle :}
    pos needle size + s size > if
        0
    else
        -1
        needle size 0 ?do
            i needle at  i pos + s at  =  and
        loop
    then ;

: starts-with? ( s prefix -- ? ) {: s prefix :}
    s 0 prefix substring-at? ;

: ends-with? ( s suffix -- ? ) {: s suffix :}
    s size suffix size - {: off :}
    off 0 < if  0  else  s off suffix substring-at?  then ;

\ contains? ( s needle -- ? ) — does needle appear anywhere in s?
\ Empty needle is vacuously contained (matches at every offset, but
\ the loop still terminates because we OR every result together).
: contains? ( s needle -- ? ) {: s needle :}
    needle size s size > if
        0
    else
        0                                           \ found accumulator
        s size needle size - 1+  0 ?do
            s i needle substring-at?  or
        loop
    then ;

\ pad-left / pad-right ( s n ch -- t ) — return a string of width
\ max(n, s size) with ch padding the short side.  Clamps so n smaller
\ than s.size is a no-op (the input flows through unchanged).
: pad-left ( s n ch -- t ) {: s n ch :}
    new-string {: dst :}
    n s size - 0 max 0 ?do  ch dst string-push  loop
    s size 0 ?do  i s at  dst string-push  loop
    dst ;

: pad-right ( s n ch -- t ) {: s n ch :}
    new-string {: dst :}
    s size 0 ?do  i s at  dst string-push  loop
    n s size - 0 max 0 ?do  ch dst string-push  loop
    dst ;

\ repeat-char ( ch n -- s ) — a string of n copies of ch.
\ repeat-string ( s n -- t ) — a string of n copies of s concatenated.
\ Both clamp non-positive n to the empty string.
: repeat-char ( ch n -- s ) {: ch n :}
    new-string {: dst :}
    n 0 max 0 ?do  ch dst string-push  loop
    dst ;

: repeat-string ( s n -- t ) {: s n :}
    new-string {: dst :}
    n 0 max 0 ?do
        s size 0 ?do  i s at  dst string-push  loop
    loop
    dst ;

\ ── Number ↔ string conversion ────────────────────────────────────
\
\ A pair of bridges over Factor's `number>string` / `string>number`,
\ wrapped so they speak our `string` class rather than Factor's
\ native string type.
\
\ Integer base follows the current `BASE` (so the same `n>string` in
\ `HEX:` mode renders hex), and floats / scientific notation /
\ Factor-style numeric literals are all accepted on the parsing
\ side.  Failure on `s>n` returns the standard ANS two-value
\ shape `( 0 0 )` so a parsed zero is unambiguous.

\ n>string ( n -- s ) — render a number as a fresh string.  Works on
\ ints (in the current `BASE`) and floats (via Factor's default
\ float-to-string formatting).  Wraps the runtime rawvec → darray →
\ string so the result is a first-class `string` (the same class
\ `>string` returns) — collection algorithms work on it directly.
: n>string ( n -- s )   num>chars <darray> <string> ;

\ s>n ( s -- n ? ) — parse a string as a number.  Returns `( n -1 )`
\ on success or `( 0 0 )` on any parse failure (empty, non-numeric,
\ trailing garbage).  Use the flag to branch; don't treat 0 as a
\ sentinel because a string of "0" parses successfully to 0.
\ Unwraps string → darray → rawvec so the runtime sees a Factor
\ sequence it can feed to `string>number`.
: s>n ( s -- n ? )      string>chars darray>data chars>num ;

\ ── show-into-string capture ──────────────────────────────────────
\
\ Capture the output of any rendering word as a string.  The xt
\ should have effect ( x -- ): typically `' show`, but any word
\ that writes to the output stream works (`' .`, `' dump`, a custom
\ rendering, etc.).  The output stream is redirected only during
\ the call; the user's normal stdout is untouched.

\ to-string ( x -- s ) — render x as a string.  Integers and floats
\ take the direct `n>string` path (no need for a string-writer round
\ trip just to call back into Factor's `number>string`); everything
\ else routes through `show` via `capture1`.  The result is a fresh
\ first-class `string`, so it composes with every text utility:
\
\   42 to-string 6 '0' pad-left show       \  → 000042
\   3 4 <point> to-string size .            \ length of "(3,4)"
\
\ Combines with everything: the captured string is a first-class
\ collection, so `to-string` then `pad-left`, `trim`, `upcase-string`
\ etc. all compose naturally.
: to-string ( x -- s )
    dup typeof int-type   = if  n>string  exit then
    dup typeof float-type = if  n>string  exit then
    ' show capture1 <darray> <string> ;

\ capture-with ( x xt -- s ) — render via xt and capture.  The
\ underlying primitive, exposed for users who want a different
\ renderer than `show` (e.g. `' dump` for the debug representation
\ or `' .` for raw number printing).
: capture-with ( x xt -- s )  capture1 <darray> <string> ;

\ ── Formatted strings: `{}` placeholders ─────────────────────────
\
\ `format ( s d -- t )` walks template string `s` and substitutes
\ each `{}` marker with the next value from darray `d`, rendered
\ via `to-string`.  Number of `{}`s should equal `d size` — extras
\ on either side are silently dropped (extra `{}` consumes past
\ the end and is an error; extra values are ignored).
\
\ For the common cases, `format1` / `format2` / `format3` take
\ the values directly on the stack and build the darray for you.
\
\ No escape syntax: a literal `{` (without a `}` after it) passes
\ through as itself, but there is no way to escape a literal `{}`
\ pair in the template.  If you need that, build the string with
\ `string-append`.

\ Helper: is template[i..i+1] the `{}` marker?  Out-of-range → no.
\ Lifted out so the main loop reads top-down.
: marker-at? ( s i -- ? ) {: s i :}
    i 1 + s size < if
        i s at '{' =  i 1 + s at '}' =  and
    else 0 then ;

\ Helper: push every char of src onto dst.  Same shape as
\ `append-into` (which appends across two strings via the chars
\ slot) but written over the collection protocol, so any
\ char-collection works as src.
: push-chars ( src dst -- ) {: src dst :}
    src size 0 ?do
        i src at  dst string-push
    loop ;

\ format ( s d -- t ) — the workhorse.  Two-cursor walk: `vi` over
\ the value darray, `i` over the template chars.  When `i` hits a
\ `{}` marker, pop the next value, render it, append, advance both.
: format ( s d -- t ) {: s d :}
    new-string {: result :}
    0 0                                  \ vi i  on the data stack
    begin
        dup s size <
    while
        s over marker-at? if
            \ Substitute: stack is ( vi i ); pop the value at d[vi].
            swap                          \ ( i vi )
            dup d at to-string  result push-chars
            1 +                           \ vi+1
            swap 2 +                      \ skip past `{}`  → ( vi+1 i+2 )
        else
            \ Literal char: push s[i], advance i.  `at` is ( i c -- x )
            \ with c on top, so `dup s at` is what we want — no swap.
            dup s at  result string-push
            1 +
        then
    repeat
    2drop
    result ;

\ The convenience arity wrappers.  Build a darray inline from
\ stack values and delegate.  These cover ~all real format calls;
\ for >3 values use `format` with a hand-built darray.
: format1 ( s v -- t ) {: s v :}
    new-darray {: d :}  v d d-push
    s d format ;

: format2 ( s a b -- t ) {: s a b :}
    new-darray {: d :}  a d d-push  b d d-push
    s d format ;

: format3 ( s a b c -- t ) {: s a b c :}
    new-darray {: d :}  a d d-push  b d d-push  c d d-push
    s d format ;
