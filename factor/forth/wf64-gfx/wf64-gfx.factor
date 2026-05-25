! forth.wf64-gfx — Host-callback FFI vocab.
!
! Binds the small set of C extern functions the NewFactor host binary
! (newfactor-ui.exe) exports for the iGui Direct2D front-end.  These
! symbols are the SAME symbols WF64 exports from its runtime.rs; we
! reuse the iGui module via the `wf64` path dependency, so on Windows
! the symbols are statically linked into newfactor-ui and reachable
! through the executable's own export table.
!
! LIBRARY REGISTRATION
!
!   Before any of these words run, the Rust host MUST register itself
!   with Factor's FFI under the library name "nf-host".  Concretely,
!   newfactor-ui's startup quotation calls
!
!     "nf-host" "<path-to-exe>" cdecl add-library
!
!   exactly once via `nf_host_register_library` (see src/session.rs).
!   Headless smoke tests that don't use gpane-* don't need to
!   register the library.
!
! NAME CONVENTION
!
!   Rust extern symbol           Factor binding name      ANS-Forth name
!   ───────────────────────────  ───────────────────────  ────────────────
!   rt_gpane_open                rt_gpane_open            gpane-open
!   rt_gpane_begin               rt_gpane_begin           gpane-begin
!   rt_gpane_present             rt_gpane_present         gpane-present
!   rt_gpane_clear               rt_gpane_clear           gpane-clear
!   rt_gpane_fill_rect           rt_gpane_fill_rect       gpane-fill-rect
!   rt_gpane_stroke_rect         rt_gpane_stroke_rect     gpane-stroke-rect
!   rt_gpane_line                rt_gpane_line            gpane-line
!   rt_gpane_fill_circle         rt_gpane_fill_circle     gpane-fill-circle
!   rt_gpane_next_event_for      rt_gpane_next_event_for  (wrapped below)
!
!   The Rust ANS Forth compiler emits the ANS-Forth name as-is into
!   the Factor IR; this vocab provides the ANS-Forth-name word
!   definitions, which thin-wrap the raw FFI calls.
!
! TYPE WIDTH
!
!   All numeric parameters are declared `longlong` (signed 64-bit) on
!   the Factor side even though the Rust signatures use `u64`.
!   Rationale: the Rust functions immediately reinterpret as `i64`
!   (`x as i64 as f32`), so bit-pattern preservation is what matters.
!   Signed-64 on the Factor side matches ANS Forth's signed-cell
!   semantics directly (negative coordinates flow through correctly).
!
! UNITS
!
!   Colours are 24-bit packed 0xRRGGBB in one cell.  Coordinates and
!   sizes are signed integer pixels.  `gpane-open` returns a child_id
!   (>0 success, 0 failure).

USING: alien.c-types alien.data alien.syntax forth.runtime
       kernel locals ;
IN: forth.wf64-gfx

! ─── 1. Raw FFI bindings ─────────────────────────────────────────────

LIBRARY: nf-host

! title_addr is declared `void*` so Factor's FFI marshaller
! auto-pins the byte-array we pass and forwards its data
! pointer.  Declaring it `longlong` would force us through
! `alien-address`, which strictly refuses byte-array-backed
! aliens (vm/alien.cpp:14 `type_error(ALIEN_TYPE, obj)`).
FUNCTION: longlong rt_gpane_open ( longlong width,
                                   longlong height,
                                   void*    title_addr,
                                   longlong title_len )

FUNCTION: longlong rt_gpane_begin ( longlong child_id )

FUNCTION: longlong rt_gpane_present ( )

FUNCTION: longlong rt_gpane_clear ( longlong rgb )

FUNCTION: longlong rt_gpane_fill_rect ( longlong x,
                                        longlong y,
                                        longlong w,
                                        longlong h,
                                        longlong rgb )

FUNCTION: longlong rt_gpane_stroke_rect ( longlong x,
                                          longlong y,
                                          longlong w,
                                          longlong h,
                                          longlong thick,
                                          longlong rgb )

FUNCTION: longlong rt_gpane_line ( longlong x0,
                                   longlong y0,
                                   longlong x1,
                                   longlong y1,
                                   longlong thick,
                                   longlong rgb )

FUNCTION: longlong rt_gpane_fill_circle ( longlong cx,
                                          longlong cy,
                                          longlong r,
                                          longlong rgb )

! rt_gpane_next_event_for ( child_id timeout_ms
!                           *kind *p1 *p2 *p3 *p4 -- got? )
!
! The five `out_*` arguments are raw C pointers to i64 cells.  The
! ANS-named wrapper below allocates a 5-cell scratch buffer.
FUNCTION: longlong rt_gpane_next_event_for ( longlong child_id,
                                             longlong timeout_ms,
                                             longlong* out_kind,
                                             longlong* out_p1,
                                             longlong* out_p2,
                                             longlong* out_p3,
                                             longlong* out_p4 )

! ─── 2. ANS-Forth-named surface ──────────────────────────────────────
!
! These are the names the Rust compiler emits into the Factor IR.
! Bodies are thin wrappers around the raw FFI: pointer-flatten
! nf-addr arguments, drop the always-zero return code from
! "command-style" calls, swap stack order if needed.

! gpane-open ( w h c-addr u -- child_id )
!
! c-addr is an nf-addr tuple wrapping a byte-array.  Pass its
! `ba` slot directly — Factor's FFI sees `void*` and auto-pins
! the byte-array for the call duration, forwarding the data
! pointer.  Note: this ignores the nf-addr's `off` slot; for
! string literals from `S"` the offset is always 0, so this
! works.  If you ever need to pass a slice with non-zero
! offset, use `<displaced-alien>` and accept the resulting
! type-check ceremony.
:: gpane-open ( w h c-addr u -- child_id )
    w h c-addr ba>> u
    rt_gpane_open ;

: gpane-begin ( child_id -- )
    rt_gpane_begin drop ;

: gpane-present ( -- )
    rt_gpane_present drop ;

: gpane-clear ( rgb -- )
    rt_gpane_clear drop ;

: gpane-fill-rect ( x y w h rgb -- )
    rt_gpane_fill_rect drop ;

: gpane-stroke-rect ( x y w h thick rgb -- )
    rt_gpane_stroke_rect drop ;

: gpane-line ( x0 y0 x1 y1 thick rgb -- )
    rt_gpane_line drop ;

: gpane-fill-circle ( cx cy r rgb -- )
    rt_gpane_fill_circle drop ;

! ─── 3. Event API wrapper ────────────────────────────────────────────
!
! Rust signature uses 5 out-pointers; the Forth-side shape (per WF64
! convention in kernel/igui_gfx.masm) is:
!
!     gpane-next-event ( child_id timeout-ms -- p4 p3 p2 p1 kind )
!
! On timeout / no event, all five values are 0 (kind = EV_NONE),
! keeping the stack effect predictable.
!
! Each call allocates a fresh 5-cell scratch byte-array; Factor's GC
! handles short-lived allocations efficiently and Rust pins the
! pointer for the duration of the call.

:: gpane-next-event ( child_id timeout-ms -- p4 p3 p2 p1 kind )
    0 longlong <ref> :> out-kind
    0 longlong <ref> :> out-p1
    0 longlong <ref> :> out-p2
    0 longlong <ref> :> out-p3
    0 longlong <ref> :> out-p4
    child_id timeout-ms
    out-kind out-p1 out-p2 out-p3 out-p4
    rt_gpane_next_event_for drop
    out-p4 longlong deref
    out-p3 longlong deref
    out-p2 longlong deref
    out-p1 longlong deref
    out-kind longlong deref ;

! ─── 4. Event-kind constants (mirror runtime.rs EV_*) ────────────────
!
! These match the i64 tags in `wf64::runtime::EV_*`.  Forth user code
! does `EV_KEY = if ... then`; the Rust compiler emits CONSTANT
! references which become Factor word references resolved here.

CONSTANT: EV_NONE        0
CONSTANT: EV_KEY         1
CONSTANT: EV_CHAR        2
CONSTANT: EV_MOUSE       3
CONSTANT: EV_FOCUS       4
CONSTANT: EV_RESIZE      5
CONSTANT: EV_CLOSE       6
CONSTANT: EV_FRAME_CLOSE 7
CONSTANT: EV_TICK       13
