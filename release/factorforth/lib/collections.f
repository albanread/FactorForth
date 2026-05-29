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

\ `new-like ( c -- d )` — a fresh, empty collection of c's OWN type,
\ shaped to hold c's elements: a result you fill by linear index with
\ at!.  This is what lets `map` preserve type — a grid maps to a grid,
\ a darray to a darray.  Extend it for any class you add.
\
\ Like the size/at methods, these bodies use only the auto-generated
\ boa constructors (<grid> / <darray>) and accessors, never a `:` word
\ defined later in this file.
\   * grid  — same w*h, freshly zeroed (every index already writable).
\   * darray — empty; its at! (set-nth) grows the backing as you write,
\     so writing indices 0..size-1 in order fills it to the right length.
GENERIC: new-like ( c -- d )
METHOD: new-like ( g:grid -- d )
    dup grid>w swap grid>h 2dup * <cells> <grid> ;
METHOD: new-like ( d:darray -- e )
    drop <rawvec> <darray> ;

\ `clone` (Layer 0's copy protocol) — a collection owns a mutable
\ backing store, so the default shallow clone would share it.  These
\ methods rebuild the tuple around a COPIED store, so the copy is fully
\ independent: mutating one never touches the other.  `(clone)` deep-
\ copies the Factor backing (array / vector elements and all).
METHOD: clone ( g:grid -- copy )
    dup grid>w over grid>h          \ g w h
    rot grid>cells (clone)          \ w h cells'
    <grid> ;
METHOD: clone ( d:darray -- copy )
    darray>data (clone) <darray> ;

\ ── dict — a key→value map ───────────────────────────────────────
\
\ Backed by a Factor hashtable.  Keys and values are any value; lookup,
\ insert, and membership are O(1).  A dict is a KEYED collection, not a
\ positional one, so it implements `size` but not the linear `at`/`at!`
\ — iterate it through `dict-keys` / `dict-values`, which hand back a
\ darray that DOES support each/map/fold.
\
\ `dict-at` returns two values — the value and a found flag — so a
\ stored 0 or f is never mistaken for "missing" (the same idiom as
\ find / index-of).
CLASS: dict SLOT: data ;

: new-dict    ( -- d )             <hash> <dict> ;
: dict-at     ( key d -- value ? ) dict>data hash-at ;
: dict-set    ( value key d -- )   dict>data hash! ;
: dict-has?   ( key d -- ? )       dict>data hash-key? ;
: dict-del    ( key d -- )         dict>data hash-del ;
: dict-keys   ( d -- keys )        dict>data hash-keys <darray> ;
: dict-values ( d -- vals )        dict>data hash-vals <darray> ;

METHOD: size  ( d:dict -- n )      dict>data hash-len ;
METHOD: clone ( d:dict -- copy )   dict>data (clone) <dict> ;

\ ── set — a collection of unique values ──────────────────────────
\
\ Backed by a Factor hash-set.  Membership (`set-has?`) is O(1) via the
\ hash — distinct from the sequence `member?`, which scans linearly
\ through `equals?`.  Like dict, it's unordered: `size` yes, linear
\ `at` no; iterate via `set-members`.
CLASS: set SLOT: data ;

: new-set     ( -- s )         <hashset> <set> ;
: set-add     ( elt s -- )     set>data hs-add ;
: set-has?    ( elt s -- ? )   set>data hs-in? ;
: set-del     ( elt s -- )     set>data hs-del ;
: set-members ( s -- members ) set>data hs-members <darray> ;

METHOD: size  ( s:set -- n )    set>data hs-len ;
METHOD: clone ( s:set -- copy ) set>data (clone) <set> ;

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
\ collects the results into a fresh collection of the SAME type as the
\ input — a grid maps to a grid, a darray to a darray.  The result is
\ built by `new-like` and filled by linear index, so the shape (a
\ grid's w*h, a darray's length) is preserved.
0 VALUE map-c
0 VALUE map-xt
0 VALUE map-dst
: map ( c xt -- d )
    TO map-xt  TO map-c
    map-c new-like TO map-dst
    map-c size 0 ?do
        i map-c at  map-xt call1>    \ y
        i map-dst at!                \ write at the same linear index
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

\ ── Search & predicate combinators ────────────────────────────────
\
\ The predicate family, all over the protocol.  xt is a predicate
\ ( x -- ? ).  (These scan every element — no early exit — favouring a
\ simple, obviously-correct loop over short-circuiting; the result is
\ the same either way.)

\ `tally ( c xt -- n )` counts the elements that satisfy the predicate.
\ (Named tally, not count, to leave ANS COUNT's name free.)
0 VALUE tally-c
0 VALUE tally-xt
0 VALUE tally-n
: tally ( c xt -- n )
    TO tally-xt  TO tally-c
    0 TO tally-n
    tally-c size 0 ?do
        i tally-c at  tally-xt call1>
        if  tally-n 1 +  TO tally-n  then
    loop
    tally-n ;

\ `any? ( c xt -- ? )` — true iff at least one element satisfies xt.
\ Expressed over tally: any match means the count is non-zero.
: any? ( c xt -- ? )  tally 0 > ;

\ `all? ( c xt -- ? )` — true iff every element satisfies xt.  Starts
\ true and is cleared by the first element that fails (vacuously true
\ for an empty collection, the standard convention).
0 VALUE all-c
0 VALUE all-xt
0 VALUE all-flag
: all? ( c xt -- ? )
    TO all-xt  TO all-c
    -1 TO all-flag
    all-c size 0 ?do
        i all-c at  all-xt call1>
        0= if  0 TO all-flag  then
    loop
    all-flag ;

\ `find ( c xt -- x ? )` — the FIRST element satisfying xt and a found
\ flag.  When nothing matches, x is 0 and the flag is false.  Two
\ returns rather than a sentinel, so any value (including 0) is a valid
\ element without ambiguity.
0 VALUE find-c
0 VALUE find-xt
0 VALUE find-val
0 VALUE find-found
: find ( c xt -- x ? )
    TO find-xt  TO find-c
    0 TO find-val  0 TO find-found
    find-c size 0 ?do
        i find-c at                  \ x
        dup find-xt call1>           \ x flag
        if                           \ x   (matched)
            find-found 0= if         \ keep only the first match
                TO find-val  -1 TO find-found
            else drop then
        else drop then               \ x   (no match) -> drop
    loop
    find-val find-found ;

\ ── Numeric reductions (conveniences over fold) ───────────────────
\
\ Common folds with their identity element baked in.  Number
\ collections only — they lean on +/* directly.
: sum     ( c -- n )  0 ' + fold ;
: product ( c -- n )  1 ' * fold ;

\ ── Equality-based search ─────────────────────────────────────────
\
\ Where find/any?/all? take a predicate, these take a value and compare
\ it against each element with Layer 0's `equals?` — so they honour a
\ class's own notion of equality automatically.  (Requires core.f.)

\ `member? ( x c -- ? )` — true iff some element of c equals x.
0 VALUE mem-x
0 VALUE mem-c
0 VALUE mem-flag
: member? ( x c -- ? )
    TO mem-c  TO mem-x
    0 TO mem-flag
    mem-c size 0 ?do
        mem-x  i mem-c at  equals?
        if  -1 TO mem-flag  then
    loop
    mem-flag ;

\ `index-of ( x c -- i ? )` — the linear index of the first element
\ equal to x, plus a found flag.  Like `find`, two returns so index 0
\ is unambiguous from "not present".
0 VALUE idx-x
0 VALUE idx-c
0 VALUE idx-i
0 VALUE idx-found
: index-of ( x c -- i ? )
    TO idx-c  TO idx-x
    0 TO idx-i  0 TO idx-found
    idx-c size 0 ?do
        idx-x  i idx-c at  equals?
        if
            idx-found 0= if  i TO idx-i  -1 TO idx-found  then
        then
    loop
    idx-i idx-found ;
