\ collections.f — CoreProtocols, Layer 1: collections.
\
\ Load after core.f.  Pure ANS Forth on the object system + the
\ <cells> / cells@ / cells! primitives (a fixed mutable store).
\
\ grid — a 2-D mutable cell store.
\   * 0-based: the first cell is (0, 0).
\   * addressed (x, y): column first, then row — matching canvas
\     coordinates, so the GUI layer and the grid agree.
\   * row-major: the linear index is  y * width + x.

CLASS: grid SLOT: w SLOT: h SLOT: cells ;

\ `new-grid ( w h -- g )` is the constructor you call: it allocates
\ the backing store (w*h zeroed cells) and builds the grid.  The
\ raw boa `<grid> ( w h cells -- g )` is the low-level form.
: new-grid ( w h -- g )
    2dup * <cells>  <grid> ;

\ Dimension readers (friendlier names over the auto getters).
: grid-w ( g -- w )  grid>w ;
: grid-h ( g -- h )  grid>h ;

\ Linear index for (x, y), row-major.
: (grid-index) ( x y g -- i )  grid>w * + ;

\ Read / write a cell by (x, y).  No bounds check — pair with
\ in-bounds? when the coordinates aren't already known good.
: at-xy  ( x y g -- v )
    dup grid>cells >r (grid-index) r> swap cells@ ;

: at-xy! ( v x y g -- )
    dup grid>cells >r (grid-index) r> swap cells! ;

\ True iff 0 <= n < limit.
: (0..<?) ( n limit -- ? )
    over 0 >= -rot < and ;

\ True iff (x, y) is inside the grid.
: in-bounds? ( x y g -- ? )
    dup grid>h rot swap (0..<?)     \ x g  (y in [0,h))
    -rot grid>w (0..<?)             \ (x in [0,w))  with the y-flag below
    and ;

\ ── darray — a growable 1-D sequence ─────────────────────────────
\
\ (Named darray — "dynamic array" — to avoid colliding with Factor's
\ own `vector` class in dispatch.  It is the standard library's
\ growable vector.)  Backed by the <rawvec> store, which grows on
\ push.  Holds any value per element, like a slot.

CLASS: darray SLOT: data ;

: new-darray ( -- d )  <rawvec> <darray> ;
: d-push ( x d -- )    darray>data rawvec-push ;

\ ── The collection protocol ───────────────────────────────────────
\
\ A small set of generics every collection implements, so algorithms
\ written against the protocol work on any backing.  grid joins it
\ with a linear (row-major) view alongside its 2-D at-xy.

GENERIC: size ( c -- n )           \ element count
GENERIC: at   ( i c -- x )         \ read element at linear index i
GENERIC: at!  ( x i c -- )         \ write element at linear index i

\ grid — linear view: w*h cells, row-major.  (Uses the class
\ accessors grid>w / grid>h directly: METHOD: bodies are emitted
\ before plain `:` definitions in the same compile, so a method must
\ not forward-reference a `:` word like grid-w defined later — the
\ auto-generated accessors are available, the wrappers are not.)
METHOD: size ( g:grid -- n )    dup grid>w swap grid>h * ;
METHOD: at   ( i g:grid -- x )  grid>cells swap cells@ ;
METHOD: at!  ( x i g:grid -- )  grid>cells swap cells! ;

\ darray — the growable sequence.
METHOD: size ( d:darray -- n )    darray>data rawvec-len ;
METHOD: at   ( i d:darray -- x )  darray>data rawvec-at ;
METHOD: at!  ( x i d:darray -- )  darray>data rawvec-set ;

\ ── Algorithms over the protocol ─────────────────────────────────
\
\ Written ONCE against size/at — they work on any collection that
\ implements them (grid, darray, and anything you add later).  This
\ is the payoff of the protocol: no per-class iteration code.
\
\ `each ( c xt -- )` runs xt once per element (the element on the
\ stack).  xt is an execution token — get one with `'`:  xs ' . each
\ prints every element.  (Held in VALUEs across the loop so the
\ collection and token read cleanly; single-threaded, like the rest.)
0 VALUE each-c
0 VALUE each-xt
: each ( c xt -- )
    TO each-xt  TO each-c
    each-c size 0 do
        i each-c at  each-xt call1
    loop ;

\ `map ( c xt -- d )` applies xt ( x -- y ) to every element and
\ collects the results into a fresh darray (any input collection,
\ darray result — a type-preserving `map` waits on a `like` protocol).
0 VALUE map-c
0 VALUE map-xt
0 VALUE map-dst
: map ( c xt -- d )
    TO map-xt  TO map-c
    new-darray TO map-dst
    map-c size 0 do
        i map-c at  map-xt call1>  map-dst d-push
    loop
    map-dst ;

\ `filter ( c xt -- d )` keeps the elements for which the predicate
\ xt ( x -- ? ) is true, into a fresh darray.
0 VALUE filt-c
0 VALUE filt-xt
0 VALUE filt-dst
: filter ( c xt -- d )
    TO filt-xt  TO filt-c
    new-darray TO filt-dst
    filt-c size 0 do
        i filt-c at                 \ element
        dup filt-xt call1>          \ element flag
        if filt-dst d-push else drop then
    loop
    filt-dst ;

\ `fold ( c init xt -- acc )` threads an accumulator through every
\ element, left to right: acc starts at init, and for each element
\ xt ( acc x -- acc ) folds it in.  This is the general reducer the
\ other algorithms specialise — sum is `0 ' + fold`, and so on.
\ (Held in VALUEs across the loop, like each/map/filter.  call2> is
\ the two-in/one-out effect-annotated call that keeps the DO loop
\ inferable.)
0 VALUE fold-c
0 VALUE fold-xt
0 VALUE fold-acc
: fold ( c init xt -- acc )
    TO fold-xt  TO fold-acc  TO fold-c
    fold-c size 0 do
        fold-acc  i fold-c at  fold-xt call2>  TO fold-acc
    loop
    fold-acc ;
