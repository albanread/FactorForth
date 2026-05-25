\ letmandelbrot.f - the Mandelbrot set rendered to a pane (using LET DSL)
\
\ A rewritten version of gfx-mandelbrot.f using the M2.x LET construct
\ for infix arithmetic.
\
\ Try it:  letmandelbrot
\
\ Close the window to return to the prompt.


\ ── 1. Parameters ─────────────────────────────────────────
\
\ View window in the complex plane: real x maps from -2.5 to
\ +1.0 across `width` columns, imaginary y from -1.25 to +1.25
\ across `height` rows.  Each `mb-blk x mb-blk` screen block
\ corresponds to one (col, row) iteration cell.

64 constant mb-maxiter
2  constant mb-blk

\ Computed FCONSTANT — supported now that `fconstant` accepts
\ multi-token expressions.  These are constant-folded by
\ Factor at compile time, so each use costs nothing extra.
3.5e 240e f/   fconstant mb-dx       \ real-axis step per column
2.5e 180e f/   fconstant mb-dy       \ imag-axis step per row
-2.5e          fconstant mb-x0       \ left edge of real axis
-1.25e         fconstant mb-y0       \ top edge of imag axis


\ ── 2. Palette ────────────────────────────────────────────
\
\ A 16-step escape-time gradient: deep navy → ice-white →
\ amber → black.  Pixels that never escape (n == maxiter)
\ are painted black for the classic "interior is solid".

: mb-colour-palette ( n -- rgb )
    15 and
    dup 0 = if drop 0x0D1540 else
    dup 1 = if drop 0x102B80 else
    dup 2 = if drop 0x1558C8 else
    dup 3 = if drop 0x3A8CF5 else
    dup 4 = if drop 0x7DC8FF else
    dup 5 = if drop 0xB8EEFF else
    dup 6 = if drop 0xFFFFFF else
    dup 7 = if drop 0xFFF4A8 else
    dup 8 = if drop 0xFFCC57 else
    dup 9 = if drop 0xFFA000 else
    dup 10 = if drop 0xFF6800 else
    dup 11 = if drop 0xE83800 else
    dup 12 = if drop 0xAA1200 else
    dup 13 = if drop 0x650000 else
    dup 14 = if drop 0x280000 else
        drop 0x080010
    then then then then then then then then
    then then then then then then then
;

: mb-colour ( n -- rgb )
    dup mb-maxiter = if
        drop 0x000000
    else
        mb-colour-palette
    then
;


\ ── 3. Escape-time iteration (the heart of the renderer) ──
\
\ Given a starting point z0 = (z0x, z0y) and parameter c
\ (= (cx, cy)) in the complex plane, iterate z = z² + c up to
\ `maxiter` times and return the iteration count at which
\ |z|² > 4 (the orbit has escaped to infinity), or `maxiter`
\ if the orbit stays bounded.
\
\ Stack picture:  z0x z0y cx cy maxiter -- n
\
\ FactorForth's unified typed stack means floats and ints
\ coexist; no need to bounce through an FP stack.  We park
\ x, y, cx, cy in variables to keep the loop body readable —
\ doing it all on the stack would be possible but the
\ shuffling would dominate the code.

variable mb-x    variable mb-y       \ current iterate z = (x, y)
variable mb-cx   variable mb-cy      \ Mandelbrot parameter c
variable mb-iters                    \ saved maxiter (loop budget)
variable mb-count                    \ iterations actually run


\ Helper words.  Splitting predicate from body keeps each one
\ small and self-contained — every word has a clean static
\ stack effect that Factor's compiler can infer without
\ having to reason about EXIT or non-local exits inside an
\ IF branch of a DO loop.  An earlier attempt using
\ ?DO/LOOP + EXIT couldn't satisfy Factor's effect checker:
\ both branches of the inner IF have to merge to the same
\ ( -- step ) shape that DO/LOOP wants, and EXIT's
\ "doesn't return" marker doesn't reconcile with the
\ ( -- 1 ) of the fall-through path when the merge is
\ inspected post-hoc by the not-compiled audit.

: mb-bounded-step? ( -- ? )
    \ One Mandelbrot iteration: z ← z² + c.
    \ Takes x, y, cx, cy from variables, computes next step.
    \ Returns true if |z|² < 4, false if escaped.
    mb-x f@ mb-y f@ mb-cx f@ mb-cy f@
    LET (z_re, z_im, x, y) -> (z_next_re, z_next_im, mag) =
        re, im, rmag
        WHERE re   = z_re * z_re - z_im * z_im + x
        WHERE im   = 2 * z_re * z_im + y
        WHERE rmag = re * re + im * im
    END
    4e f< if
        mb-y f! mb-x f!
        1 mb-count +!
        -1
    else
        drop drop
        0
    then ;

: fractal-iter ( z0x z0y cx cy maxiter -- n )
    \ Stash maxiter first — it's on top of the stack, and the
    \ subsequent f! stores all want floats on top.
    mb-iters !
    mb-cy f!   mb-cx f!
    mb-y  f!   mb-x  f!
    0 mb-count !

    \ BEGIN <pred> WHILE <body> REPEAT — runs body while
    \ pred is true.  Pred is true iff we haven't burnt the
    \ iteration budget AND we're still inside the |z|<2 disk.
    begin
        mb-count @ mb-iters @ <
        if mb-bounded-step? else 0 then
    while
    repeat

    mb-count @ ;


\ ── 4. Renderer ───────────────────────────────────────────
\
\ Walk a 240×180 grid of iteration cells; for each one, map
\ to its complex coordinate, iterate, colour, paint a
\ `mb-blk × mb-blk` rectangle.  Whole frame goes into one
\ batch (gpane-begin … gpane-present) so the GUI thread
\ paints it as a single Direct2D submit.

variable mb-row              \ saved row index (i/j stack juggling)
variable mb-rgb              \ scratch colour register

: mb-draw ( id -- id )
    dup gpane-begin
    0x000000 gpane-clear

    180 0 do                  \ rows: 0..179
        i mb-row !

        240 0 do              \ cols: 0..239
            \ Push z₀ = (0, 0) — Mandelbrot starts at origin.
            \ Julia sets would push a constant z₀ here instead.
            0e  0e

            \ Compute c = (cx, cy) from the (col, row) cell.
            \ `i` here is the inner loop index (column).
            \ `mb-row @` is the outer loop index (row).
            i        s>d d>f  mb-dx f*  mb-x0 f+
            mb-row @ s>d d>f  mb-dy f*  mb-y0 f+

            \ Iterate and colour.
            mb-maxiter fractal-iter      \ ( id -- id n )
            mb-colour                    \ ( id n -- id rgb )
            mb-rgb !

            \ Paint the block.  (col*blk, row*blk, blk, blk, rgb).
            i mb-blk *   mb-row @ mb-blk *   mb-blk mb-blk
            mb-rgb @
            gpane-fill-rect              \ ( id -- id )
        loop
    loop

    gpane-present
;


\ ── 5. Event loop ─────────────────────────────────────────
\
\ Block until the pane (or the whole IDE frame) is closed.
\ gpane-next-event returns five values; we just look at the
\ last (the event kind) and drop the rest.

: mb-wait ( id -- )
    begin
        dup -1 gpane-next-event           \ ( id -- id p4 p3 p2 p1 kind )
        dup ev-close = swap ev-frame-close = or
        >r   drop drop drop drop   r>     \ keep flag, drop p1..p4
    until
    drop                                  \ drop id
;


\ ── 6. Entry point ────────────────────────────────────────

: letmandelbrot ( -- )
    cr ." rendering Mandelbrot set (LET DSL) ..." cr
    480 360  S" ∴ Mandelbrot (LET DSL)"  gpane-open
    dup 0= if
        drop ." (no UI substrate — demo skipped)" cr
    else
        mb-draw
        ." done — close the window to exit" cr
        mb-wait
    then
;

." Loaded: letmandelbrot.  Try 'letmandelbrot'" cr
