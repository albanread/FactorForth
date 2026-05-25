\ gfx-shapes.f - the simplest possible graphics demo
\
\ Opens a 400x300 pane, fills it with a few coloured rectangles
\ and circles, presents the frame.  No fractal-iter dependency.
\
\ Verifies the gpane-* FFI end-to-end:
\   Forth code  ->  rt_gpane_*  ->  batch::push  ->  PostMessageW
\                                                ->  GUI thread renders.

: backdrop ( id -- id )
    dup gpane-begin
    0x101830 gpane-clear              \ deep navy
;

\ shapes draws into whichever pane is currently begin-d.
\ It doesn't take or return an id - gpane-* commands target
\ the in-progress batch, which gpane-begin set up.
: shapes ( -- )
    \ Three filled rectangles, evenly spaced.
    50  60 100 80 0xE83800 gpane-fill-rect    \ orange
   170  60 100 80 0xFFC857 gpane-fill-rect    \ amber
   290  60 100 80 0x3A8CF5 gpane-fill-rect    \ blue

    \ A stroked rectangle around the trio.
   40 50 360 100 2 0xFFFFFF gpane-stroke-rect

    \ Two circles.
   140 220 40 0x7DC8FF gpane-fill-circle      \ ice
   260 220 40 0xFFA000 gpane-fill-circle      \ amber

    \ One diagonal line.
   40 290 360 290 3 0xB8EEFF gpane-line
;

: gfx-shapes ( -- )
    cr ." opening pane and rendering shapes..." cr
    440 320  S" Shapes demo"  gpane-open
    dup 0= if
        drop ." pane open failed" cr
    else
        backdrop shapes
        gpane-present
        drop
        ." done — close the pane to dismiss" cr
    then
;

." Loaded: gfx-shapes.  Try 'gfx-shapes'" cr
