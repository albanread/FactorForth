\ othello.f — text Othello (Reversi) on CoreProtocols.
\
\ The Phase 1 capstone: a real program built entirely on the object
\ system + the standard library (grid from collections.f).  No GUI —
\ the board renders as text.  Load after core.f and collections.f.
\
\ Board is an 8x8 grid; each cell is empty / black / white.  We hold
\ the live game in a VALUE so the move words read cleanly
\ (`x y black play` rather than threading the board through the stack).

0 CONSTANT empty
1 CONSTANT black
2 CONSTANT white

0 VALUE board

\ Standard opening: the central four squares, 0-based (x,y).
\   (3,3)=white (4,4)=white   (3,4)=black (4,3)=black
: othello-new ( -- )
    8 8 new-grid TO board
    white  3 3 board at-xy!
    white  4 4 board at-xy!
    black  3 4 board at-xy!
    black  4 3 board at-xy! ;

\ The opponent of a colour.
: other ( color -- color' )
    black = if white else black then ;

\ ── Rendering ─────────────────────────────────────────────────────
\
\ A cell's glyph: '.' empty, 'X' black, 'O' white.
: cell>char ( v -- ch )
    dup empty = if drop 46 else        \ '.'
    dup black = if drop 88 else        \ 'X'
    drop 79 then then ;                \ 'O'

\ Print the board, one row per line.  Reads the backing cells
\ linearly (no at-xy), so the loop index never collides with at-xy's
\ internal return-stack use.
: show-board ( -- )
    board grid-h board grid-w *  0 do
        board grid>cells i cells@ cell>char emit
        i 1+ board grid-w mod 0= if cr then
    loop ;

\ ── Move engine ───────────────────────────────────────────────────
\
\ The move under consideration lives in m* variables; the directional
\ scan uses s* variables.  Keeping them apart lets count-flips and
\ do-flips share scan state without trampling the move.  Single board,
\ single-threaded — globals are the simple, correct choice here.
VARIABLE mx   VARIABLE my   VARIABLE mcol     \ the move: (x,y) colour
VARIABLE mdx  VARIABLE mdy                    \ current direction
VARIABLE sx   VARIABLE sy                     \ scan cursor
VARIABLE sdx  VARIABLE sdy  VARIABLE scol     \ scan direction + colour
VARIABLE flips     VARIABLE scanning

\ Seed the scan cursor at the move cell, in the current direction.
: scan-init ( -- )
    mcol @ scol !  mdy @ sdy !  mdx @ sdx !  my @ sy !  mx @ sx ! ;

\ Step the scan cursor one cell along the direction.
: scan-step ( -- )
    sx @ sdx @ + sx !   sy @ sdy @ + sy ! ;

\ How many opponent pieces does the move bracket in the current
\ direction?  Walk from the cell after the move: count a run of
\ opponent pieces; if it's closed by our own colour (on the board),
\ return the run length, else 0.
: count-flips ( -- n )
    scan-init  0 flips !  -1 scanning !
    begin scanning @ while
        scan-step
        sx @ sy @ board in-bounds? 0= if
            0 flips !  0 scanning !              \ ran off the edge
        else
            sx @ sy @ board at-xy                \ the cell
            dup empty = if   drop  0 flips !  0 scanning !
            else scol @ = if         0 scanning !   \ closed: keep the tally
            else  flips @ 1+ flips !  then then     \ opponent: tally on
        then
    repeat
    flips @ ;

\ Flip n cells from the move cell along the current direction.
: do-flips ( n -- )
    scan-init
    begin dup 0 > while
        scan-step
        scol @  sx @ sy @  board at-xy!
        1-
    repeat drop ;

\ Resolve the current direction: count, and flip if it brackets.
: flip-dir ( -- )
    count-flips dup 0 > if do-flips else drop then ;

\ Place `color` at (x,y) and flip in all eight directions.  Assumes
\ the move is legal (at least one direction brackets); illegal moves
\ simply place a piece with nothing to flip.
: play ( x y color -- )
    mcol !  my !  mx !
    mcol @  mx @ my @  board at-xy!
    -1 mdx ! -1 mdy ! flip-dir
     0 mdx ! -1 mdy ! flip-dir
     1 mdx ! -1 mdy ! flip-dir
    -1 mdx !  0 mdy ! flip-dir
     1 mdx !  0 mdy ! flip-dir
    -1 mdx !  1 mdy ! flip-dir
     0 mdx !  1 mdy ! flip-dir
     1 mdx !  1 mdy ! flip-dir ;
