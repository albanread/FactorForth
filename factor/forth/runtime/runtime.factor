! forth.runtime — NewFactor's runtime support vocab.
!
! ARCHITECTURE
!
!   The Rust ANS Forth compiler (src/compiler/) emits canonical Factor
!   source as its internal IR.  That IR resolves word references in
!   exactly three places:
!
!     1. Factor's built-in vocabs (kernel, math, sequences, ...) for
!        words that already exist with the right semantics — DUP, DROP,
!        SWAP, +, *, etc.  The Rust resolver renames ANS-name → Factor
!        name during emission; no aliases needed in this vocab.
!
!     2. forth.runtime (this file) for words ANS Forth needs that
!        Factor doesn't directly express, or expresses with different
!        semantics.  Memory model, return stack, ANS-style I/O, ANS
!        booleans, ANS-floored mod, do-loop machinery with i/j.
!
!     3. forth.wf64-gfx for host-callback words exposed by newfactor-ui
!        (gpane-*, ev-*).  Separate vocab; see factor/wf64-gfx/.
!
! BUDGET: this file targets 200-400 lines.  If it grows past 600,
!         re-evaluate per word whether the Rust resolver could handle
!         it via direct rename instead.
!
! STATUS: Phase 1 first draft.  Each word's body is a sketch; the real
!         implementations land as the compiler comes online and we
!         verify each against ANS test fixtures.

USING: accessors alien alien.accessors alien.c-types alien.data
       arrays byte-arrays continuations io kernel kernel.private
       math math.bitwise math.functions math.order math.parser
       namespaces prettyprint.config sequences sequences.private
       strings ;
IN: forth.runtime

! ── 1. CELL-ADDRESSED MEMORY MODEL ───────────────────────────────────────
!
! ANS Forth's memory model assumes a linear byte-addressable space with
! cells (8 bytes here, 64-bit cell).  Factor has no such space —
! everything is GC'd objects.  We bridge by representing an "address"
! as a (byte-array, offset) tuple.  All addresses Forth ever sees flow
! through this representation; arithmetic on addresses (CELL+, CHARS,
! etc.) operates on the offset while keeping the byte-array stable.
!
! What this gives us: every portable ANS program that only uses addresses
! Forth gave it (from VARIABLE, CREATE, HERE+ALLOT) just works.
! What this does NOT give us: programs that POKE/PEEK arbitrary integer
! addresses (e.g. HEX 8000 @ for hardware registers).  Those are out
! of scope; use the FFI instead.

TUPLE: nf-addr
    { ba   byte-array read-only }   ! the backing storage
    { off  integer    read-only } ; ! byte offset into ba

: <nf-addr> ( ba off -- addr ) nf-addr boa ; inline

! Address arithmetic.  An ANS address can have an integer added or
! subtracted to it (within the same allocation), staying inside the
! original byte-array.
:: nf-addr+ ( addr n -- addr' )
    addr ba>> addr off>> n + <nf-addr> ; inline

: cell+ ( addr -- addr' )  8 nf-addr+ ; inline
: char+ ( addr -- addr' )  1 nf-addr+ ; inline
: cells ( n -- bytes )     8 * ; inline
: chars ( n -- bytes )     ; inline  ! UTF-8: chars are byte-counted

! ── ANS fetch/store ──
!
! 64-bit (cell) and 8-bit (char) loads/stores against an nf-addr.
! Implemented via Factor's alien.c-types primitives on the byte-array's
! data pointer.  byte-arrays are pinned (Factor doesn't move them once
! data is taken from them via alien-address), but for safety we go
! through set-alien-unsigned-cell which Factor's compiler knows is
! safe under GC.
!
! NAMING: ANS Forth's `!` (store) clashes with Factor's line-comment
! syntax, so the Factor-side word is `nf-!`.  The Rust compiler emits
! the rename when it sees ANS `!` in source.  Same for `c!` → `nf-c!`
! and `+!` → `nf-+!` for symmetry, even though `c!` and `+!` would
! parse OK in isolation (`!` is only a comment as a standalone token).
! Keeping the prefix uniform makes the resolver table cleaner.
!
! ANS `@` and `c@` parse fine in Factor, so they keep their names.

! NB: alien-unsigned-cell takes ( c-ptr n -- value ) and
! set-alien-unsigned-cell takes ( value c-ptr n -- ).  The bi order
! below produces ( ba off ) on top, matching both signatures.

: @ ( addr -- n )
    [ ba>> ] [ off>> ] bi alien-unsigned-cell ; inline

: nf-! ( n addr -- )
    [ ba>> ] [ off>> ] bi set-alien-unsigned-cell ; inline

: c@ ( addr -- ch )
    [ ba>> ] [ off>> ] bi alien-unsigned-1 ; inline

: nf-c! ( ch addr -- )
    [ ba>> ] [ off>> ] bi set-alien-unsigned-1 ; inline

: nf-+! ( n addr -- )
    [ @ + ] [ nf-! ] bi ; inline

! ── ANS float fetch/store ──
!
! `f@` reads an IEEE-754 double from an nf-addr; `nf-f!` stores
! one.  Both go through Factor's `alien-double` accessor.  Used
! by the `farray` defining-word: a cell-wide buffer with float
! semantics on access.

: f@ ( addr -- f )
    [ ba>> ] [ off>> ] bi alien-double ; inline

: nf-f! ( f addr -- )
    [ ba>> ] [ off>> ] bi set-alien-double ; inline

! ── Raw native pointer (for FFI) ──
!
! An nf-addr passed across the FFI boundary collapses to a single
! u64: the byte-array's pinned data pointer plus the offset.  Factor
! byte-arrays are pinned by the GC; the pointer is stable for the
! duration of the call.  CALLER MUST keep the original nf-addr live
! across the call (Factor's compiler does this automatically when the
! nf-addr is on the data stack).
!
! Used by forth.wf64-gfx (gpane-open's title pointer) and any future
! ANS Forth FFI helpers.

! Convert nf-addr's (byte-array + offset) to a raw u64 address.
!
! WARNING: only valid for nf-addrs whose `ba` slot is an alien
! (not a byte-array).  `alien-address` strictly refuses
! byte-array-backed pointers (vm/alien.cpp:14:
! `type_error(ALIEN_TYPE, obj)`) because the data can move
! under GC.  For byte-array-backed nf-addrs, declare the FFI
! parameter as `void*` instead of `longlong` and pass the
! byte-array directly (Factor pins it for the call duration);
! see `gpane-open` in forth.wf64-gfx for the canonical shape.
: nf-addr-raw ( addr -- u64 )
    [ ba>> >c-ptr alien-address ] [ off>> ] bi + ; inline

! ── Variable / constant cell allocation ──
!
! The Rust compiler emits a call to <variable> for each VARIABLE
! declaration in the source.  The result is a fresh 1-cell address
! whose contents start as zero.  Allocation happens at parse time
! (Rust ensures the byte-array lives for the program's lifetime);
! at runtime, the VARIABLE word just pushes the address.

: <variable> ( -- addr )
    8 <byte-array> 0 <nf-addr> ; inline

: <buffer> ( n-bytes -- addr )
    <byte-array> 0 <nf-addr> ; inline

! ── 2. RETURN STACK (>R / R> / R@ / RDROP) ───────────────────────────────
!
! Reuses the fstack trick from the archived forth.fstack vocab: a
! fixed-capacity tuple-backed stack pinned to a Factor special-object
! slot.  ~8 instructions per push/pop after the optimising compiler's
! intrinsic fires on the literal slot index.  See
! docs/upstream/legacy-forth-vocabs/fstack/fstack.factor for the
! analysis and the bypassed-SYMBOL: rationale.

TUPLE: fstack { storage array read-only } top ;

: <fstack> ( capacity -- fstack )
    f <array> 0 fstack boa ; inline

! Slot 82 was chosen as the first unused special-object index
! (special_object_count = 85; highest named = 81).  No Factor source
! references 82 special-object, so this is collision-free.
! Lazy init.  save-image-and-exit zeros special-object slots
! outside the OBJ_STARTUP_QUOT..OBJ_BIGNUM_NEG_ONE range (=81), so
! 82 comes back as `f` after image reload.  Same story for slot 83
! below.
: get-frs ( -- fstack )
    82 special-object [ 256 <fstack> dup 82 set-special-object ] unless* ; inline
: set-frs ( fs -- )       82 set-special-object ; inline

:: fstack-push ( x fs -- )
    fs top>> :> n
    x n fs storage>> set-nth-unsafe
    n 1 + fs top<< ;

:: fstack-pop ( fs -- x )
    fs top>> 1 - :> n
    n fs top<<
    n fs storage>> nth-unsafe ;

: fstack-peek ( fs -- x )
    [ top>> 1 - ] [ storage>> ] bi nth-unsafe ; inline

: fstack-drop ( fs -- )
    [ top>> 1 - ] keep top<< ; inline

! Public ANS-name return-stack words.
!
! NOT marked `inline` deliberately.  Factor's `>r` / `r>` in `kernel`
! manipulate the retainstack and are RESTRICTED — Factor's compiler
! refuses to inline anything that touches the retainstack outside
! tightly-bracketed patterns.  Our words don't touch the retainstack
! (they push onto an fstack tuple held in a special-object slot),
! but `inline` would expose `get-frs fstack-push` at every call
! site, which makes the inference search for retainstack patterns
! and fail.  As regular non-inlined calls, the body is opaque to
! the inliner and the call-site sees a clean `( x -- )` signature.
: >r    ( x -- )    get-frs fstack-push ;
: r>    ( -- x )    get-frs fstack-pop  ;
: r@    ( -- x )    get-frs fstack-peek ;
: rdrop ( -- )      get-frs fstack-drop ;
: 2>r   ( a b -- )  swap >r >r ;
: 2r>   ( -- a b )  r> r> swap ;

! ── 3. DO/LOOP MACHINERY ─────────────────────────────────────────────────
!
! ANS Forth's DO/LOOP exposes the index via I (innermost) and J (next
! outer).  Loop frames live on a separate stack so I/J can be word
! references that look up the current index at runtime regardless of
! call depth.  This is the ANS-faithful semantic: I works correctly
! inside any quotation invoked from the loop body, even via EXECUTE.
!
! Uses special-object slot 83 for the loop-frame stack.  Same idiom
! as the return stack above; same performance profile.

! `left?` is set by LEAVE; bump-loop checks it and returns done?=t,
! exiting the loop cleanly at the next iteration boundary.
TUPLE: loop-frame
    { limit integer }
    { index integer }
    { left? boolean } ;

! Lazy initialisation:
!   `save-image-and-exit` zeros every special-object slot outside a
!   hardcoded range that maxes out at OBJ_BIGNUM_NEG_ONE (=81), so
!   our slots 82 and 83 come back as `f` after image reload.  The
!   parse-time init we used to do at the bottom of the file only
!   fires during build-image, not after the saved image loads.
!   Solution: lazy init inside the getter via `unless*` — if the
!   slot is f, allocate a fresh fstack, install it, and return it.
: get-loop-frames ( -- fs )
    83 special-object [ 32 <fstack> dup 83 set-special-object ] unless* ; inline
: set-loop-frames ( fs -- )   83 set-special-object ; inline

! Internal: push/pop a loop frame.  `f` is the initial value of the
! `left?` field — no LEAVE has fired yet.
: push-loop-frame ( limit start -- )
    f loop-frame boa get-loop-frames fstack-push ; inline

: pop-loop-frame ( -- )
    get-loop-frames fstack-drop ; inline

! I returns the innermost loop's current index.  Constant-time slot read.
: i ( -- n )
    get-loop-frames fstack-peek index>> ; inline

: j ( -- n )
    get-loop-frames [ top>> 2 - ] [ storage>> ] bi nth-unsafe index>> ; inline

! Bumping the innermost index by +1 (LOOP) or +n (+LOOP).  Checks
! the `left?` flag first — if LEAVE has fired, report done? = true
! immediately without bumping further.
:: bump-loop ( delta -- done? )
    get-loop-frames fstack-peek :> frame
    frame left?>>
    [ t ]
    [
        frame index>> delta +  :> new-index
        new-index frame index<<
        new-index frame limit>> >=
    ] if ;

! LEAVE — mark the innermost loop frame as "ready to exit".
!
! Earlier drafts used throw+recover or with-return+return for a
! non-local exit.  Both of those mechanisms restore the data stack
! to a SAVED state on return — which discards the accumulator the
! body has been building up across iterations.  ANS-faithful LEAVE
! must preserve the data stack.
!
! Flag-based LEAVE is the clean alternative: set `left?` on the
! frame, body completes its current iteration to a natural end,
! bump-loop sees the flag and exits the loop.  Trade-off: any
! body code AFTER `LEAVE` in the same iteration still runs.  The
! common ANS idiom places LEAVE at the end of an IF at the end of
! the loop body, where this distinction doesn't matter:
!
!     do  body  some-cond if leave then  loop
!
! Mid-body LEAVE in the form `do  some-cond if leave then  body  loop`
! will execute `body` once more for the leave-firing iteration.
! Document this in the user guide if it becomes a real issue.
: leave ( -- )
    get-loop-frames fstack-peek t >>left? drop ;

! do-loop:  ( limit start quot -- )
! Runs quot repeatedly with i providing the current index.  Each
! call of `quot` MUST leave the step amount on the stack as its
! final action — the compiler injects `1` for plain LOOP and the
! user's step expression for +LOOP.  bump-loop consumes the step,
! bumps the index, returns done?.  We continue while NOT done.
! LEAVE escapes early via with-return.
!
! `inline` is required for two reasons:
!   1. Factor's compiler refuses `quot call` if the surrounding word
!      isn't inline — the quotation argument can't escape into a
!      non-inline frame because the compiler needs to know the
!      quotation's identity at compile time to inline-expand it.
!   2. Callers of do-loop want effect inference to see through it.
!      With `inline` the call-site sees the full body and can
!      derive the effect from the literal quotation passed in.
!
! LEAVE just sets a flag now, so the loop always exits via the
! normal "bump-loop returned done?" path.  pop-loop-frame runs
! after the loop on every exit.
::  do-loop  ( limit start quot -- )
    limit start push-loop-frame
    [ quot call bump-loop not ] kernel:loop
    pop-loop-frame ; inline

! ?do-loop:  ( limit start quot -- )
! ANS ?DO: skip the loop entirely if limit equals start.  Standard
! DO would run once even when bounds equal (an ANS-94 footgun
! `?DO` exists to dodge); we mirror that.
::  ?do-loop  ( limit start quot -- )
    limit start = [ ] [ limit start quot do-loop ] if ; inline

! ── 4. ANS-STYLE I/O ─────────────────────────────────────────────────────
!
! ANS . prints a signed integer in the current BASE followed by a
! single space.  Factor's prettyprint:. is the debugger pretty-printer
! (different output for collections, no trailing space).  We define
! our own.  number-base is the global from prettyprint.config that
! HEX, DECIMAL, OCTAL, BINARY mutate.

USE: prettyprint.config       ! for number-base

: . ( n -- )
    number-base get >base write " " write ; inline

: u. ( u -- )
    number-base get >base write " " write ; inline   ! unsigned uses same path on 64-bit

: cr ( -- )       "\n" write ; inline
: emit ( ch -- )  1string write ; inline
: space ( -- )    " " write ; inline
: spaces ( n -- ) [ space ] times ; inline

! ── ANS string vocabulary ─────────────────────────────────────────────────
!
! ANS Forth's traditional string model uses PAD-as-temporary, dual
! c-addr / counted-string representations, and CMOVE direction
! conventions that have made these the most crash-prone words in
! the spec for forty years.  We sidestep all of that:
!
!   - Every string lives as an nf-addr wrapping a byte-array.
!     Byte-arrays are GC'd; nothing ever clobbers anything.
!   - (c-addr u) is always a *pair* explicitly on the stack.
!   - PAD doesn't exist.  Words that traditionally returned a
!     pointer into PAD instead allocate a fresh byte-array.
!
! `S" text"` in user source emits as
!   "text" forth.runtime:s-quote-runtime
! which produces (nf-addr u) per ANS.  `." text"` emits via the
! dedicated `print-string` (a thin wrapper over Factor's `write`)
! to avoid round-tripping through nf-addr for output-only text.

USING: io.encodings.utf8 byte-arrays sequences ;

! s-quote-runtime: take a Factor string literal (compile-time
! constant) and return the ANS (nf-addr u) pair.  Allocates a
! fresh byte-array per call.
::  s-quote-runtime  ( factor-str -- nf-addr u )
    factor-str utf8 encode  :> bytes
    bytes 0 <nf-addr>
    bytes length ; inline

! print-string: emit a Factor string directly.  Used by the `."`
! emit shape; not user-callable through ANS.
: print-string ( factor-str -- )
    write ; inline

! ANS TYPE: write u bytes from c-addr to standard output.
! Decodes via UTF-8 — string literals from S" are encoded UTF-8
! at compile-time, so the decode round-trips losslessly.  For
! cbuffer contents the user filled themselves, garbage in =
! garbage out (no validation).
::  type  ( c-addr u -- )
    c-addr ba>>   :> ba
    c-addr off>>  :> start
    start u +     :> end
    start end ba <slice> utf8 decode write ;

! CMOVE: copy u bytes from src to dst.  ANS doesn't define
! behaviour for overlapping ranges in CMOVE (CMOVE> is the
! backward-direction word for that case); we don't either,
! conservatively forward-copy.
!
! NB: nf-addr+ has signature ( addr n -- addr' ) — addr below,
! offset on top.  So `src i nf-addr+` is "src offset by i."
::  cmove  ( src dst u -- )
    u 0 <= [ ] [
        u <iota> [| i |
            src i nf-addr+ c@         ! fetch byte at src[i]
            dst i nf-addr+ nf-c!      ! store at dst[i]
        ] each
    ] if ;

! FILL: write `char` (low byte) to u consecutive bytes starting
! at c-addr.
::  fill  ( c-addr u char -- )
    u 0 <= [ ] [
        char :> ch
        u <iota> [| i |
            ch  c-addr i nf-addr+  nf-c!     ! ch at c-addr+i
        ] each
    ] if ;

! Convenience: `BL` constant for the ASCII space, used by FILL
! to clear buffers (`buf 80 BL FILL`).
CONSTANT: bl 32

! ALLOT: in traditional ANS this extends the data-space pointer;
! we don't have a data-space pointer, so this is a no-op that
! simply drops its argument.  Real allocation happens at template
! instantiation time (see sema::expand_templates).  Programs that
! call ALLOT outside a CREATE/DOES> template context get a quiet
! drop rather than an error; that's a documented deviation from
! ANS but matches our "no raw memory" stance.
: allot ( n -- ) drop ; inline

! FLOATS: cells/chars/floats are interchangeable address-arithmetic
! multipliers in ANS.  We have cells (×8) and chars (×1) already;
! floats is an alias for cells since our cells are 64-bit.
: floats ( n -- bytes ) 8 * ; inline

! HERE: in traditional ANS this returns the data-space pointer.
! We don't have one; HERE returns 0 as a stub so programs that
! reference it don't crash.  Real ANS code rarely uses HERE for
! anything beyond "the address where the next ALLOT will land",
! which doesn't translate to our model.
: here ( -- addr ) 0 ; inline

! ── Host I/O callbacks (Phase 3.1b) ───────────────────────────────────────
!
! The Rust host exposes three C extern functions that Factor calls
! when user code does I/O.  This is the same FFI machinery
! forth.wf64-gfx uses for `rt_gpane_*`, just covering character-
! level read/write/line-read.
!
! At session startup the Rust worker thread evaluates a one-liner
! to register the host binary as the `nf-host` FFI library:
!
!     "nf-host" "<path-to-current-exe>" cdecl add-library
!
! Then `install-host-streams` (defined below) binds Factor's
! output-stream (and optionally input-stream) to host-routed
! tuples — so EMIT, CR, TYPE, `.`, anything that does `write`,
! flows through `nf_rt_write_char` and lands wherever the session's
! IoMode decided (a Test-mode buffer, stdout, a GUI pane, …).

USING: alien.libraries alien.syntax io.styles ;

LIBRARY: nf-host

FUNCTION: longlong nf_rt_read_char  ( )
FUNCTION: void     nf_rt_write_char ( longlong ch )
FUNCTION: longlong nf_rt_read_line  ( void* buf, longlong max )

! Float-FFI proof of life.  rt_check_double exercises double-in
! / double-out through XMM0 (Win64 ABI for the first FP arg and
! the FP return); rt_emit_double pushes the IEEE-754 byte pattern
! through the captured output stream so the Rust side can verify
! bit-exact transmission.
FUNCTION: double rt_check_double ( double x )
FUNCTION: void   rt_emit_double  ( double x )

! M2.x #32 — INCLUDED needs to compile an ANS source file through
! NewFactor's own compiler (the file contents are ANS, not Factor).
! rt_compile_ans takes a NUL-terminated c-string path and returns
! a malloc'd C string containing the resulting Factor IR.  Factor's
! FFI marshaling allocates a fresh NUL-terminated copy of the
! Factor string for the call and frees it on return.
FUNCTION: c-string rt_compile_ans ( c-string path )

! Output stream backed by nf_rt_write_char.  Implements just enough
! of the Factor stream protocol that `write` and friends work.
TUPLE: nf-host-output-stream ;
C: <nf-host-output-stream> nf-host-output-stream

M: nf-host-output-stream stream-write1 ( ch stream -- )
    drop nf_rt_write_char ;

M: nf-host-output-stream stream-write ( str stream -- )
    drop [ nf_rt_write_char ] each ;

M: nf-host-output-stream stream-nl ( stream -- )
    drop 10 nf_rt_write_char ;

M: nf-host-output-stream stream-flush ( stream -- )
    drop ;

M: nf-host-output-stream stream-format ( str style stream -- )
    nip [ nf_rt_write_char ] each ;

M: nf-host-output-stream dispose ( stream -- ) drop ;

! Input stream backed by nf_rt_read_char.  Just enough for KEY-like
! single-char reads; line reading goes through nf_rt_read_line on
! the ACCEPT path.
TUPLE: nf-host-input-stream ;
C: <nf-host-input-stream> nf-host-input-stream

M: nf-host-input-stream stream-read1 ( stream -- ch/f )
    drop nf_rt_read_char dup -1 = [ drop f ] when ;

M: nf-host-input-stream dispose ( stream -- ) drop ;

! Bind Factor's global I/O streams to host-routed tuples.  Run
! once at session startup.  Also installs our custom eval callback
! (defined further down in §13) so user code errors surface as
! readable ANS-style messages rather than Factor's stack-trace
! dump.  We can't call install-nf-eval-callback here because the
! word is defined later in the file; instead, the Rust session
! worker calls `install-nf-eval-callback` as a separate eval
! after install-host-streams runs.
: install-host-streams ( -- )
    <nf-host-input-stream>  input-stream  set-global
    <nf-host-output-stream> output-stream set-global
    <nf-host-output-stream> error-stream  set-global ;

! Same idea, but using `set` instead of `set-global` so the binding
! takes effect in the CURRENT dynamic scope.  Factor's eval-callback
! wraps every nf_eval_string call in `with-string-writer`, which
! rebinds output-stream dynamically — overriding our global binding
! for the duration of the eval.  Prepending a call to this word at
! the start of each eval source pushes the host-routed binding back
! on top of that dynamic frame, so emit / type / `.` / cr again
! flow through nf_rt_write_char.
: rebind-host-streams ( -- )
    <nf-host-input-stream>  input-stream  set
    <nf-host-output-stream> output-stream set ;

! ANS-named user-facing words for host I/O.  KEY blocks waiting
! for a byte; ACCEPT reads up to u bytes into c-addr, returns
! actual count.  Both bypass Factor's stream layer and call the
! host directly — they're cheap and the indirection through
! input-stream isn't needed for the common case.
: key ( -- ch )
    nf_rt_read_char ;

: accept ( c-addr u -- u' )
    [ nf-addr-raw ] [ ] bi*
    nf_rt_read_line ;

! ── Pictured numeric output (the ANS `<# # #S sign hold #>` DSL) ──
!
! In traditional ANS this DSL uses PAD as the accumulator and
! builds the string backward.  We replace PAD with a per-call
! Factor string-buffer that we build *forward* and reverse at
! `#>`.  Observable behaviour matches ANS exactly:
!
!   <#  opens a session (clears the accumulator)
!   #   peels one digit off TOS using current BASE, pushes its
!       character onto the accumulator
!   #S  peels remaining digits until n = 0; always at least one
!       digit so `0 <# #S sign #>` yields "0"
!   sign  if TOS is negative, prepend '-' (in pre-reverse order,
!         push '-' onto the accumulator)
!   hold  push an arbitrary character (useful for "0x" prefixes,
!         comma group separators, etc.)
!   #>  close session, drop the residual n, return (c-addr u) of
!       the finished string
!
! Plus the convenience: `n>$` does the standard signed-decimal
! flow in one word — `dup abs <# #S swap sign #>` is the ANS
! incantation.

USING: namespaces math.parser ;

SYMBOL: current-num-buf
CONSTANT: digit-chars "0123456789abcdefghijklmnopqrstuvwxyz"

: <#  ( -- )
    SBUF" " clone current-num-buf set ;

: #  ( n -- n' )
    number-base get /mod
    digit-chars nth
    current-num-buf get push ;

: #S  ( n -- 0 )
    ! Always extract at least one digit, so 0 #S → "0".
    # [ dup 0 = ] [ # ] until ;

: sign  ( n -- )
    0 < [ CHAR: - current-num-buf get push ] when ;

: hold  ( ch -- )
    current-num-buf get push ;

: #>  ( n -- c-addr u )
    drop
    current-num-buf get >string reverse
    utf8 encode
    dup length
    [ 0 <nf-addr> ] dip ;

! ANS-idiomatic single-word: signed decimal formatting.
: n>$  ( n -- c-addr u )
    dup abs <# #S swap sign #> ;

! Base-switching shortcuts.  ANS users say `hex` to switch to
! base 16 for a block, then `decimal` to switch back.
: hex     ( -- ) 16 number-base set ;
: decimal ( -- ) 10 number-base set ;
: binary  ( -- )  2 number-base set ;
: octal   ( -- )  8 number-base set ;

! ── 5. ANS BOOLEANS (-1 / 0, not Factor's t / f) ─────────────────────────

CONSTANT: forth-true  -1
CONSTANT: forth-false 0

: bool>flag ( ? -- n )  forth-true forth-false ? ; inline
: flag>bool ( n -- ? )  0 = not ; inline

! ── ANS comparator wrappers ──────────────────────────────────────────────
!
! Every ANS comparator returns -1 / 0 on the data stack.  Factor's
! native `=` / `<` etc. return Factor's `t` / `f`.  These wrappers
! sit between user code and Factor; the resolver maps source-level
! `=`, `<`, `<>`, `<=`, `>=`, `>`, `0=`, `0<`, `0>` to the ans- form.
!
! Tail call to `bool>flag` is the conversion.  Marked `inline` so
! Factor's optimizer folds `expr bool>flag` into a single conditional
! move (cmov) — no branch in the emitted machine code.

: ans=  ( a b -- n )   =      bool>flag ; inline
: ans<> ( a b -- n )   =  not bool>flag ; inline
: ans<  ( a b -- n )   <      bool>flag ; inline
: ans>  ( a b -- n )   >      bool>flag ; inline
: ans<= ( a b -- n )   <=     bool>flag ; inline
: ans>= ( a b -- n )   >=     bool>flag ; inline
: ans0= ( n -- m )     zero?  bool>flag ; inline
: ans0< ( n -- m )     0 <    bool>flag ; inline
: ans0> ( n -- m )     0 >    bool>flag ; inline

! Float comparators land on the same paths — Factor's `<` and friends
! are polymorphic over int and float operands.
: ansf< ( a b -- n )   <      bool>flag ; inline
: ansf> ( a b -- n )   >      bool>flag ; inline
: ansf= ( a b -- n )   =      bool>flag ; inline

! IF / WHILE / UNTIL consume an ANS flag and treat 0 as false,
! anything else as true.  At emit time, NewFactor's compiler
! prepends `flag>bool` (= `zero? not` ≡ `0 = not`) before any
! Factor `if` / `when` so 0 becomes Factor's `f` and non-zero
! becomes `t`.  See src/compiler/emit.rs `Expr::If` handling.
! `BEGIN ... UNTIL` and `BEGIN ... WHILE ... REPEAT` use `zero?`
! directly with the appropriate branch structure (no extra wrap
! needed — already encoded in the loop emit).

! ── 6. ANS-FLOORED MOD (differs from Factor's truncated mod) ─────────────
!
! Factor's `mod` truncates toward zero.  ANS Forth's MOD is floored:
! the sign of the result matches the sign of the divisor.  Worked
! example: -7 MOD 3 is 2 in ANS, -1 in Factor's default mod.

: floored-mod ( a b -- r )
    ! Algorithm: r = a mod b (truncated, Factor's default).  If r is
    ! nonzero AND the signs of r and b differ, add b — that's the
    ! floored-adjustment.  Worked traces:
    !   -7  3 → tuck mod gives ( 3 -1 ); -1<0 differs from 3<0 → -1+3 = 2  ✓
    !    7 -3 → tuck mod gives ( -3 1 ); 1<0 differs from -3<0 → 1+-3 = -2 ✓
    !    7  3 → tuck mod gives ( 3 1 );  signs agree → nip → 1        ✓
    !   -7 -3 → tuck mod gives ( -3 -1 ); signs agree → nip → -1     ✓
    !    0  5 → tuck mod gives ( 5 0 );  r=0 short-circuit → nip → 0  ✓
    tuck mod                              ! ( a b -- b r )
    dup 0 = [
        nip                               ! r=0: keep r, drop b
    ] [
        2dup 0 < swap 0 < xor             ! ( b r -- b r signs-differ? )
        [ + ] [ nip ] if                  ! differ: r+b ; same: drop b
    ] if ;

! Rust resolver maps source-level `MOD` to this word.

! ── 7. STACK WORDS NOT IN FACTOR KERNEL ──────────────────────────────────

: ?dup ( x -- 0 | x x )  dup [ dup ] when ; inline
: depth ( -- n )         get-datastack length ; inline

! Factor kernel has 2dup, 2drop, 2swap, 2over, 2nip, 3dup, 3drop, 4dup.
! Anything else the user writes resolves directly.

! ── 8. NUMBER CONVERSIONS ────────────────────────────────────────────────
!
! Factor's integers are arbitrary-precision and unified, so most
! ANS conversion words collapse to identity or thin wrappers.  We
! provide them for completeness so the Rust resolver has unambiguous
! targets to emit against.

: s>d ( n -- n )        ; inline  ! identity in unified-integer Factor
: d>s ( n -- n )        ; inline
: d>f ( n -- f )        >float ; inline
: f>d ( f -- n )        >integer ; inline

! ── 9. FP ALIASES ────────────────────────────────────────────────────────
!
! Factor's + - * / are polymorphic over int and float.  ANS's f+ f- f* f/
! are float-only.  We alias for naming clarity; the Rust resolver can
! also lower f+ → + directly when the types are known.

ALIAS: f+  +
ALIAS: f-  -
ALIAS: f*  *
ALIAS: f/  /
ALIAS: f<  <
ALIAS: f>  >
ALIAS: f=  =

! ── 10. EXECUTION TOKEN HELPERS ──────────────────────────────────────────
!
! ANS EXECUTE consumes an XT and calls it.  In Factor a "word" is the
! XT equivalent; `execute` already exists in kernel and does the right
! thing for fully-typed call sites.  For unknown-effect XTs from the
! data stack, the Rust compiler emits `call( -- )` (Factor's runtime-
! checked dynamic call).  The wrapper below is only for the
! contexts where the compiler can't make that decision Rust-side.

! ans-execute accepts EITHER a Factor word (what `\ name` produces
! and what `'` in our compiler emits) OR a Factor quotation
! (anything produced by `[ ... ]`).  Factor's `call( -- )` handles
! both — invokes the word's body or runs the quotation.  The
! runtime-checked `( -- )` effect annotation means we accept
! anything that produces no net data-stack change; user code that
! wants more flexibility can fall back to Factor's `call` directly.
: ans-execute ( xt -- )  call( -- ) ; inline

! ── 11. ANS CORE COMPLETENESS (M2.x #39) ─────────────────────────────────
!
! Words filling out the ANS Core word set so user programs don't have
! to define their own.  Most are tiny — `1+` is `1 +` literally — but
! grouping them here keeps the resolver one-line entries clean.

! Arithmetic shortcuts.  Factor has `2/` (as `-1 shift`) but not
! `1+` / `1-` / `2*` — all small wrappers.
: 1+ ( n -- n+1 ) 1 + ; inline
: 1- ( n -- n-1 ) 1 - ; inline
: ans2* ( n -- 2n ) 1 shift ; inline

! Floored-division /MOD.  Factor's /mod returns ( quotient remainder ),
! ANS expects ( remainder quotient ) — swap to match.  Floored remainder
! is what `floored-mod` produces; floored quotient = (a - r) / b which is
! exactly divisible.  We do it in one pass:
:: ans/mod ( a b -- r q )
    a b floored-mod :> r
    a r - b /i      :> q
    r q ;

! Intermediate-precision multiply-divide.  ANS allows the implementation
! to use a wider intermediate type to avoid overflow during the multiply.
! On Factor that means bignum promotion — automatic and free.
: ans*/    ( a b c -- d )    [ * ] dip /i ; inline
:: ans*/mod ( a b c -- r q )
    a b * :> ab
    ab c floored-mod :> r
    ab r - c /i      :> q
    r q ;

! Bit-shifts.  Factor's `shift` is bidirectional: positive shifts left,
! negative shifts right.  ANS LSHIFT / RSHIFT take an unsigned count
! and direction is encoded in the word name.
: ans-lshift ( n u -- n' )  shift ; inline
: ans-rshift ( n u -- n' )  neg shift ; inline

! 2@ / 2! — fetch and store a CELL PAIR.  ANS: 2@ ( addr -- x1 x2 )
! where x2 is at addr+0 (data-stack TOS after) and x1 is at addr+CELL
! (data-stack NOS after).  2! is the inverse.
: ans2@ ( addr -- x1 x2 )
    [ cell+ @ ] [ @ ] bi ;
: ans2! ( x1 x2 addr -- )
    swap over nf-! cell+ nf-! ;

! ERASE = FILL with 0.  Already have `fill ( c-addr u char -- )`.
: ans-erase ( c-addr u -- )  0 fill ;

! ANS 0<> uses the same wrapper pattern as the other comparators.
: ans0<> ( n -- m )  zero? not bool>flag ; inline

! ANS double-stack ops.  Factor's kernel:2over is DIFFERENT
! — it's `( x y z -- x y z x y )` (3-cell version, equivalent to
! `over over`).  ANS 2OVER takes a 4-deep stack and copies items
! 3-4 to the top.  Same for 2SWAP — Factor's core doesn't ship it.
! Use locals for clarity.
:: ans2swap ( a b c d -- c d a b )  c d a b ;
:: ans2over ( a b c d -- a b c d a b )  a b c d a b ;

! ── 12. MANAGED STRINGS (M2.x #43 — `$` vocab) ───────────────────────────
!
! WF64's managed-string library, ported to use Factor's native
! `string` type as the backing storage.  Strings are immutable,
! GC-tracked, Unicode-aware — every traditional Forth string
! footgun (PAD clobbering, lifetime of S", counted-string length
! byte) goes away because the user never holds a raw address.
!
! Surface:
!   S$" lit"     compile-time literal (parse-side handled by Rust)
!   $len $clen   byte / codepoint length
!   $+           concatenation
!   $upper $lower case conversion (Unicode-aware via Factor)
!   $find        substring search; -1 on miss
!   $contains?   substring presence; ANS -1/0
!   $starts? $ends?  prefix/suffix check; ANS -1/0
!   $slice       substring extraction
!   $cmp         lexicographic compare; -1/0/1 in ANS style
!   $hash        hash code (Factor's hashcode — stable per VM run)
!   $.  $.cr     print (with / without trailing newline)
!   >$           legacy (c-addr u) → $ converter
!   $>addr       $ → legacy (c-addr u) converter
!   int>$  $>int  number ↔ string
!
! All `?`-suffixed words return ANS -1/0 via `bool>flag` so
! they slot into the IF / WHILE / UNTIL convention.

USING: sequences strings unicode math.parser splitting byte-arrays
       io.encodings.string io.encodings.utf8 ;

: $len     ( s -- n )         length ;
: $clen    ( s -- n )         length ;          ! Factor strings are Unicode codepoints already
: $+       ( a b -- c )       append ;
: $upper   ( s -- s' )        >upper ;
: $lower   ( s -- s' )        >lower ;

! $find: ( haystack needle -- index | -1 )
! Factor's `subseq-index ( seq subseq -- i/f )` matches our arg
! order — haystack=seq, needle=subseq, no swap needed.  Convert
! the `f` return (not-found) to -1 to match ANS conventions.
: $find ( haystack needle -- index )
    subseq-index dup [ ] [ drop -1 ] if ;

! $contains? / $starts? / $ends? : ANS predicate convention.
: $contains? ( s sub -- flag )    subseq-index >boolean bool>flag ;
: $starts?   ( s prefix -- flag ) head? bool>flag ;
: $ends?     ( s suffix -- flag ) tail? bool>flag ;

! $slice: ( s from len -- s' )   ANS-style slice with offset+length.
! Factor's `subseq` takes ( from to seq ) where to is exclusive.
:: $slice ( s from len -- s' )
    from from len + s subseq ;

! $cmp: ( a b -- n )   lexicographic compare.  Returns -1, 0, or 1.
! Factor's `<=>` returns the symbols +lt+ / +eq+ / +gt+, not
! integers — translate to ANS -1/0/1.  Use cascaded comparisons
! rather than symbol-dispatch to avoid pulling in math.order's
! symbol resolution into our reduced USING:.
: $cmp ( a b -- n )
    ! `<` is math-only in Factor; use `before?` which dispatches
    ! via `<=>` and works on strings.
    2dup = [ 2drop 0 ] [ before? -1 1 ? ] if ;

! $hash: Factor's hashcode is fast and stable for the lifetime
! of the VM run.  Not bit-stable across runs — file a follow-up
! if WF64-compat FNV-64 is needed.
: $hash ( s -- n )  hashcode ;

! Output helpers.
: $.    ( s -- )  write ;
: $.cr  ( s -- )  write 10 emit ;

! Number ↔ string.
: int>$ ( n -- s )  number>string ;
: $>int ( s -- n/f )  string>number ;

! Legacy interop — bridge to the (c-addr u) nf-addr+length pair
! that S", TYPE, CMOVE use.
!
!   >$ copies u bytes out of c-addr's backing byte-array (UTF-8
!   decoded) into a fresh Factor string.  Safe: source can be
!   mutated or GC'd freely after.
:: >$ ( c-addr u -- s )
    c-addr ba>>      :> ba
    c-addr off>>     :> base
    base base u + ba subseq      ! byte-array slice
    utf8 decode ;                ! → Factor string

!   $>addr encodes the Factor string back to UTF-8 bytes, wraps
!   in a fresh nf-addr, and returns (c-addr, u).  Copy-out — the
!   caller may pass the result to TYPE/CMOVE without disturbing
!   the original.
: $>addr ( s -- c-addr u )
    utf8 encode                  ! → byte-array
    [ 0 <nf-addr> ] [ length ] bi ;


! ── 13. ANS ERROR TRANSLATION (M2.11 / #35) ──────────────────────────────
!
! When user code errors (undefined word, stack underflow, divide-
! by-zero, …) Factor throws a condition tuple or kernel-error array.
! Factor's stock print-error renders it as a multi-line stack trace
! that's mostly useless to a Forth programmer and trails with "Error
! in print-error!" when error-stream is in an awkward state.
!
! Translate to ANS THROW codes + a single-line readable message,
! emitted via the host's normal output stream.
!
! Code table (subset — ANS-1994 Table 9.1):
!   -1   ABORT
!   -4   stack underflow
!   -5   stack overflow
!   -6   return stack underflow
!   -9   invalid memory address
!  -10   division by zero
!  -11   result out of range
!  -13   undefined word
!  -24   invalid numeric argument
!  -256  implementation-defined / unknown

USING: classes classes.builtin accessors prettyprint combinators present
       io.streams.string sequences ;

! Print a single value short enough to fit on one line.  Wraps
! Factor's `short.` (which truncates pprint output) in a safe-
! present so an exotic value doesn't tank the formatter.
: nf-short. ( obj -- )
    [ short. ]
    [ 2drop "(unprintable)" print ] recover ;

! Try to convert a type-id (a small fixnum from KERNEL_ERROR
! arrays) to its class object.  Factor exposes this via
! `type>class` in classes.builtin.  If lookup fails just print
! the raw number.
: nf-type-id>name ( id -- str )
    [ type>class name>> ]
    [ 2drop "(unknown type-id)" ] recover ;

:: nf-format-kernel-error ( arr -- )
    arr length 1 > [ arr second ] [ 0 ] if
    {
        ! Numeric codes match vm/errors.hpp.  Where the error
        ! array carries useful payload (type-check has expected
        ! class + actual obj at indices 2,3; fixnum-range has the
        ! offending value at index 2; etc.) we surface it; the
        ! more detail the user gets, the less guessing they do.
        { 0  [ "ANS error -256: VM expired" print ] }
        { 1  [ "ANS error -256: I/O error " write
               arr length 2 > [ arr third . ] [ cr ] if ] }
        { 3  [ "ANS error -256: type-check failed" print
               arr length 4 >= [
                   "  expected type: " write
                   arr third nf-type-id>name print
                   "  actual object: " write
                   arr fourth nf-short.
                   "  actual class:  " write
                   arr fourth class-of name>> print
               ] when ] }
        { 4  [ "ANS error -10: Division by zero" print ] }
        { 6  [ "ANS error -256: invalid array size" print
               arr length 3 > [
                   "  requested: " write arr third .
               ] when ] }
        { 7  [ "ANS error -256: value out of fixnum range" print
               arr length 3 > [
                   "  value: " write arr third nf-short.
               ] when ] }
        { 8  [ "ANS error -256: FFI error" print
               arr length 3 > [
                   "  detail: " write arr third nf-short.
               ] when ] }
        { 9  [ "ANS error -13: undefined C symbol" print
               arr length 3 > [
                   "  symbol: " write arr third nf-short.
               ] when ] }
        { 10 [ "ANS error -4: Stack underflow" print ] }
        { 11 [ "ANS error -5: Stack overflow" print ] }
        { 12 [ "ANS error -6: Return stack underflow" print ] }
        { 13 [ "ANS error -256: Return stack overflow" print ] }
        { 14 [ "ANS error -256: Call stack underflow" print ] }
        { 15 [ "ANS error -256: Call stack overflow" print ] }
        { 16 [ "ANS error -256: out of memory" print ] }
        { 17 [ "ANS error -42: Floating-point trap" print ] }
        { 18 [ "ANS error -28: Interrupt" print ] }
        { 19 [ "ANS error -256: Callback space overflow" print ] }
        [
            "ANS error -256: kernel error code " write
            number>string print
            "  full payload: " write arr nf-short.
        ]
    } case ;

! Safe present: never lets the error escape.  Some error tuples
! (parser internals, continuations) don't have present methods,
! so a naked `err present print` can throw no-method which
! escapes the recover at the alien-callback boundary → die.
! Catch it; fall back to a fixed string.
!
! recover's recovery branch receives the original try-quot inputs
! PLUS the error on top.  Our try-quot has net ( err -- ), so
! recovery sees ( err error -- ) and must consume both.
: nf-safe-present ( err -- )
    [ present print ]
    [ 2drop "ANS error -256: (unprintable error)" print ] recover ;

! Dig through wrapping layers (lexer-error, condition) until we
! reach the underlying error tuple.  Empirically these wrap
! parse errors (no-word-error etc).
: nf-unwrap-error ( err -- err' )
    {
        { [ dup class-of name>> "lexer-error"   = ] [ error>> nf-unwrap-error ] }
        { [ dup class-of name>> "condition"     = ] [ error>> nf-unwrap-error ] }
        { [ dup class-of name>> "not-compiled"  = ] [ error>> nf-unwrap-error ] }
        [ ]
    } cond ;

:: nf-format-tuple-error ( err0 -- )
    err0 nf-unwrap-error :> err
    err class-of name>> {
        { "division-by-zero" [ "ANS error -10: Division by zero" print ] }
        { "no-method"        [ "ANS error -13: Generic dispatch failure (no method)" print ] }
        { "bounds-error"     [ "ANS error -9: Index out of bounds" print ] }
        { "undefined-word"   [
            "ANS error -13: Undefined word: " write
            err word>> dup string? [ ] [ name>> ] if print
        ] }
        { "no-word-error"    [
            "ANS error -13: Undefined word: " write
            err name>> print
        ] }
        [
          "ANS error -256: [" write write "] " write
          err nf-safe-present ]
    } case ;

: nf-format-error ( err -- )
    [
        dup array? [ nf-format-kernel-error ] [ nf-format-tuple-error ] if
    ] [
        2drop "ANS error -256: (error during error formatting)" print
    ] recover ;

! Run a piece of user source.  Catches errors and turns them into
! ANS messages instead of letting Factor's debugger run.  Mirrors
! the structure of `eval>string` (basis/eval/eval.factor) but with
! our nf-format-error in place of print-error, and no
! with-string-writer wrapping (we want output to flow through the
! host streams, not be captured into a returned C-string).
!
! IMPORTANT: don't wrap in `with-file-vocabs` here.  The interaction
! between with-file-vocabs's namespace cleanup and `recover`'s
! stack-restore was triggering Factor's alien-callback stack-effect
! checker (combinators:wrong-values) on error paths involving
! certain error classes — process got killed via `kernel:die`.
! Naked recover around the eval is cleaner and the user source
! we send already references everything via fully-qualified Factor
! names (`forth.runtime:foo`) so vocab search isn't needed.
! Callback body: runs the eval, returns a result (always null).
!
! Avoid `::` locals here — empirically, the local-capture machinery
! interacts badly with `recover` at the alien-callback boundary on
! some error paths (combinators:wrong-values → kernel:die).  Use
! a fried quotation `'[ _ ... ]` to capture the str instead — this
! is the same shape Factor's stock eval-callback uses (basis/eval/
! eval.factor's (eval>string)).
!
! Both try and recovery branches end with `f` so the net effect is
! always `( str -- f )` regardless of which path runs.
! Permissive eval that allows the user's code to leave residue
! on the data stack — the REPL contract.  The stock Factor
! eval-callback uses `eval>string` which calls `(eval)` with
! `( -- )` effect, enforcing zero net stack change per call.
! That's catastrophic for an interactive Forth: any program
! that pushes a value without immediately consuming it (which
! is normal — variables, partial computations, intermediate
! values left for the next eval) crashes with an underflow-
! shaped error.
!
! Fix: parse the source ourselves, then `call( ..a -- ..b )` —
! Factor's runtime-checked dynamic call with row-var effect.
! Row vars accept any actual stack change, so residue is fine.
!
! Structure note: the `recover` MUST be nested inside a word
! that's CALLED BY the alien-callback's outermost quotation,
! not in the outermost quotation itself.  Empirically a recover
! at the alien-callback boundary trips combinators:wrong-values
! → kernel:die on some error paths.  Mirroring the stock
! `eval>string` shape (recover is inside `(eval>string)` which
! is called by `eval>string` which is called by the callback's
! body quotation) keeps us safely two levels in.

USING: fry eval ;

! The data stack is fresh on every alien-callback invocation
! (Factor's `with-callback-frame` saves+restores around the
! callback body to enforce its declared `( c-string -- void* )`
! effect).  For an interactive REPL we need stack contents to
! persist across calls: type `5`, then `dup .` in a separate
! eval, both `5`s should print.
!
! Solution: hold the inter-eval stack in a global symbol.  On
! eval entry, restore it; run user code; capture the resulting
! stack; save back.  `with-datastack` does the heavy lifting —
! it takes (stack quot --) and returns the resulting stack as
! an array, all in one operation.
SYMBOL: nf-saved-datastack
{ } nf-saved-datastack set-global

! Parse the source into a quot, then run it under
! `with-datastack` — restoring the previously-saved data stack
! before, capturing the resulting data stack after.  This is
! how interactive stack residue survives the alien-callback
! frame: Factor's `with-callback-frame` zeroes the data stack
! at the C boundary, but our saved-stack symbol lives in the
! global namespace which is unaffected.
!
! The combinator runs as `[ ... ] call( quot -- )` so the alien-
! callback's static stack-effect checker sees a clean
! ( quot -- ) net effect for the body.  with-datastack itself
! has a static effect of ( array quot -- new-array ), which the
! body consumes/produces in a balanced way.
USING: eval ;

! with-datastack would be the natural mechanism but its internal
! save-restore interacts badly with Factor's catch when a kernel
! error (stack underflow code 10, etc.) fires inside the user
! quotation — the throw appears to skip catch and crash the
! process.  See #47.  We do the equivalent dance by hand using
! set-datastack + get-datastack with no try-quot of our own —
! the catch in nf-do-eval owns the unwind.
!
! On success: data stack is loaded with saved contents, quot
! runs, resulting ds is captured into the global, ds is then
! cleared back to ( ) so the alien-callback frame sees a clean
! net effect.
!
! On throw: the catch in nf-do-eval rewinds ds to ( str ), the
! recover branch runs, the saved-stack global is left at its
! previous value (we never reached set-global).  Stack residue
! from before the bad eval is preserved.
: nf-eval-with-saved-stack ( str -- )
    parse-string                                ! ( quot )
    nf-saved-datastack get-global swap          ! ( saved quot )
    [ set-datastack ] dip                       ! ( ...saved... quot )
    call( ..a -- ..b )                          ! ( ...result... )
    get-datastack                               ! ( ...result... vec )
    nf-saved-datastack set-global               ! ( ...result... )
    { } set-datastack ;                         ! ( )

! Recover shape: when try fails, recover restores the stack to
! the state BEFORE the try-quot was called, then pushes the error
! on top.  So recovery here starts with `[ str err ]` on the
! stack, not just `[ err ]`.  Net for recovery must be ( str err -- )
! to match try's net of ( str -- ).
: nf-do-eval ( str -- )
    [ nf-eval-with-saved-stack ]
    [ nip nf-format-error flush ] recover ;

: nf-do-eval-with-vocabs ( str -- )
    [ nf-do-eval ] with-file-vocabs ;

! Public name kept for compatibility — some callers reference
! `nf-eval-source` by name (e.g. install-nf-eval-callback's
! pre-existing diagnostics).  Same shape: consume str, leave
! nothing on stack.
: nf-eval-source ( str -- )  nf-do-eval-with-vocabs ;

! The alien callback Factor invokes from `nf_eval_string`.  The
! function signature is `void* fn(c-string)` — we return null
! because output already went through the host streams during
! eval; the caller's "interpreter_output" field stays empty.
USING: alien.syntax ;
: nf-eval-callback ( -- callback )
    void* { c-string } cdecl
    [ nf-do-eval-with-vocabs f ] alien-callback ;

! Install our callback in OBJ-EVAL-CALLBACK (special-object slot
! 6 in the Factor VM).  Subsequent calls to nf_eval_string from
! Rust will route through nf-eval-source instead of Factor's
! stock eval>string.
USING: alien.remote-control alien.libraries ;
: install-nf-eval-callback ( -- )
    \ nf-eval-callback ?callback OBJ-EVAL-CALLBACK set-special-object ;

! ── 13b. THE LISTENER (#54) ──────────────────────────────────────────────
!
! Factor's stock listener is the model: one long-running function
! that loops, threading a `datastack` value through each iteration
! via with-datastack.  The data stack persists naturally because
! there's no alien-callback boundary inside the loop — it's all
! one Factor execution context, just like the stock listener.
!
! Architecture:
!   Rust worker thread calls `nf_eval_string("nf-listener-start")`
!   exactly once.  That call drives nf-listener-loop, which:
!     1. Polls nf_rt_next_command (blocks on host-side channel).
!     2. If c-string is null, exits the loop.
!     3. Otherwise, runs the source with `with-datastack`
!        threading the persistent stack.  Errors are caught by
!        a tight `recover` right around with-datastack — the
!        same shape Factor's listener-step uses (basis/listener/
!        listener.factor:122).
!     4. Calls nf_rt_command_done to signal the host.
!     5. Recurses (tail call).
!
! This avoids the alien-callback-frame interaction that broke
! the previous OBJ_EVAL_CALLBACK-based approach.  Errors —
! including kernel errors like stack underflow — stay inside
! the listener's recover frame and never cross a callback
! boundary.

LIBRARY: nf-host
FUNCTION: c-string nf_rt_next_command ( )
FUNCTION: void     nf_rt_command_done ( )
! Stack-view publication.  Called after every eval to ship the
! current data-stack contents to the host's stack pane.  Items
! are sent one at a time, bracketed by begin/end so the host
! can rebuild the snapshot atomically.  Only fixnums are sent —
! other Factor object types are skipped (they'd need a more
! involved marshalling to render usefully).
FUNCTION: void nf_rt_stack_begin ( )
FUNCTION: void nf_rt_stack_item  ( longlong v )
FUNCTION: void nf_rt_stack_end   ( )

! Parse the source into a quot, then run it under with-datastack
! threading the listener's persistent stack through.
! On success: returns the new datastack.
! On error: returns the OLD datastack unchanged (the failed
! quot's residue is discarded), prints the formatted error.
:: nf-listener-eval ( datastack source -- datastack' )
    [ source parse-string :> quot
      datastack quot with-datastack ]
    [ nf-format-error flush datastack ]
    recover ;

! Ship the current data-stack contents to the host stack pane.
! Fixnums go through as-is; other values are skipped — the
! pane currently displays integers only.  The bracket pair
! lets the host rebuild its snapshot atomically.
USING: classes math ;
: nf-publish-datastack ( datastack -- )
    nf_rt_stack_begin
    [ dup fixnum? [ nf_rt_stack_item ] [ drop ] if ] each
    nf_rt_stack_end ;

! The listener loop.  Threads `datastack` as a value through
! each iteration.  Exits when nf_rt_next_command returns f
! (null c-string, signaling shutdown).
USING: alien.libraries alien.syntax ;
: nf-listener-loop ( datastack -- )
    nf_rt_next_command dup [
        ! got a source string
        nf-listener-eval
        dup nf-publish-datastack    ! ship snapshot to stack pane
        nf_rt_command_done
        nf-listener-loop
    ] [
        ! null => exit
        2drop
    ] if ;

! Entry point Rust calls via nf_eval_string.  Starts the loop
! with an empty data stack.  Earlier we pre-pushed sentinel
! zeros as a defence against bare `.` underflow (#47), but
! they polluted `depth` and the Data Stack pane.  With the
! listener-level recover catching kernel-error 10 cleanly and
! the improved formatter giving "ANS error -4: Stack underflow"
! instead of process abort, the sentinels stopped earning
! their keep.
: nf-listener-start ( -- )
    { } nf-listener-loop ;

! ── 14. ANS FILE ACCESS (M2.x #32) ───────────────────────────────────────
!
! Minimal viable surface — INCLUDED is the gate for the Forth 2012
! test runner.  Other File Access Word Set primitives (OPEN-FILE,
! READ-FILE, WRITE-FILE, etc.) are deferred until a user program
! actually wants them.
!
! INCLUDED ( c-addr u -- )
!     Reads the ANS source file at path c-addr/u, compiles it
!     through NewFactor's Rust pipeline (via the rt_compile_ans
!     FFI extern), and evaluates the resulting Factor IR.
!     UTF-8 encoding throughout.
!
!     The compilation step is what makes INCLUDED non-trivial here:
!     the file contents are ANS Forth, not Factor source, so we
!     can't just hand the raw bytes to `(eval)`.  Instead Rust does
!     the translation and we eval the IR — same path our normal
!     session eval takes, just initiated from inside Forth.

: nf-included ( c-addr u -- )
    >$                       ! (c-addr u) → Factor string path
    rt_compile_ans           ! Factor string → Factor IR string
    ( -- ) (eval) ;          ! parse + execute

! ── End of forth.runtime ─────────────────────────────────────────────────
!
! Word count: roughly 50 public words.  Re-evaluate budget once the
! Hayes test fixtures start exercising corner cases.
