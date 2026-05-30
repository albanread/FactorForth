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
METHOD: new-like ( d:darray -- e )  {: _ :}
    <rawvec> <darray> ;

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
\ prints every element.  Locals capture the collection and token
\ per-call, so each is fully re-entrant — an xt body can call `each`
\ on another collection without corrupting the outer activation.
: each ( c xt -- ) {: c xt :}
    c size 0 do
        i c at  xt call1
    loop ;

\ `map ( c xt -- d )` applies xt ( x -- y ) to every element and
\ collects the results into a fresh collection of the SAME type as the
\ input — a grid maps to a grid, a darray to a darray.  The result is
\ built by `new-like` and filled by linear index, so the shape (a
\ grid's w*h, a darray's length) is preserved.  Mid-body `{: dst :}`
\ binds the new collection right after `new-like` produces it.
: map ( c xt -- d ) {: c xt :}
    c new-like {: dst :}
    c size 0 ?do
        i c at  xt call1>          \ y
        i dst at!                  \ write at the same linear index
    loop
    dst ;

\ `filter ( c xt -- d )` keeps the elements for which the predicate
\ xt ( x -- ? ) is true, into a fresh darray.
: filter ( c xt -- d ) {: c xt :}
    new-darray {: dst :}
    c size 0 do
        i c at                     \ element
        dup xt call1>              \ element flag
        if dst d-push else drop then
    loop
    dst ;

\ `fold ( c init xt -- acc )` threads an accumulator through every
\ element, left to right: acc starts at init, and for each element
\ xt ( acc x -- acc ) folds it in.  This is the general reducer the
\ other algorithms specialise — sum is `0 ' + fold`, and so on.
\ The accumulator lives on the data stack between iterations; locals
\ just bind c and xt.  call2> is the two-in/one-out effect-annotated
\ call that keeps the DO loop inferable.
: fold ( c init xt -- acc ) {: c init xt :}
    init
    c size 0 do
        i c at  xt call2>
    loop ;

\ ── Search & predicate combinators ────────────────────────────────
\
\ The predicate family, all over the protocol.  xt is a predicate
\ ( x -- ? ).  (These scan every element — no early exit — favouring a
\ simple, obviously-correct loop over short-circuiting; the result is
\ the same either way.)

\ `tally ( c xt -- n )` counts the elements that satisfy the predicate.
\ (Named tally, not count, to leave ANS COUNT's name free.)  Counter
\ lives on the data stack, so the algorithm is re-entrant by
\ construction — calling tally from inside an xt is safe.
: tally ( c xt -- n ) {: c xt :}
    0
    c size 0 ?do
        i c at  xt call1>
        if 1+ then
    loop ;

\ `any? ( c xt -- ? )` — true iff at least one element satisfies xt.
\ Expressed over tally: any match means the count is non-zero.
: any? ( c xt -- ? )  tally 0 > ;

\ `all? ( c xt -- ? )` — true iff every element satisfies xt.  Starts
\ true and is cleared by the first element that fails (vacuously true
\ for an empty collection, the standard convention).  Combines per-
\ element results with bitwise `and` — for ANS booleans (-1 / 0) that
\ behaves as logical AND.
: all? ( c xt -- ? ) {: c xt :}
    -1
    c size 0 ?do
        i c at  xt call1>  and
    loop ;

\ `find ( c xt -- x ? )` — the FIRST element satisfying xt and a found
\ flag.  When nothing matches, x is 0 and the flag is false.  Two
\ returns rather than a sentinel, so any value (including 0) is a valid
\ element without ambiguity.  (val, flag) state lives on the data
\ stack throughout the loop.
: find ( c xt -- x ? ) {: c xt :}
    0 0                                  \ val flag
    c size 0 ?do
        i c at                           \ val flag x
        dup xt call1>                    \ val flag x matched
        if
            over 0= if
                \ first match: replace (val flag x) with (x -1)
                nip nip -1
            else
                drop
            then
        else
            drop
        then
    loop ;

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
\ Flag accumulator lives on the data stack; once set, it stays set
\ via bitwise `or`.
: member? ( x c -- ? ) {: x c :}
    0
    c size 0 ?do
        x  i c at  equals?  or
    loop ;

\ `index-of ( x c -- i ? )` — the linear index of the first element
\ equal to x, plus a found flag.  Like `find`, two returns so index 0
\ is unambiguous from "not present".  (idx, flag) state lives on the
\ data stack.
: index-of ( x c -- i ? ) {: x c :}
    0 0                                  \ idx flag
    c size 0 ?do
        x  i c at  equals?
        if
            \ if not yet found, replace (idx flag) with (i -1)
            dup 0= if
                2drop i -1
            then
        then
    loop ;

\ ── Ordered algorithms (over the collection + ordering protocols) ──
\
\ Written ONCE against size/at/at! (Layer 1) and `cmp` (Layer 0's
\ ordering protocol, core.f), so they work on any collection whose
\ elements implement `cmp` — numbers out of the box, your own classes
\ the moment they answer `cmp`.  Requires core.f.

\ `min-of ( c -- x )` / `max-of ( c -- x )` — the least / greatest
\ element by `cmp`.  Expressed as a fold seeded with the first element,
\ so they cost one pass and need a NON-EMPTY collection.
: min-of ( c -- x )  dup 0 swap at  ' lesser  fold ;
: max-of ( c -- x )  dup 0 swap at  ' greater fold ;

\ `sorted? ( c -- ? )` — is the collection in non-decreasing `cmp`
\ order?  Walks adjacent pairs; starts true and is cleared by the first
\ inversion (vacuously true for size 0 or 1).  Accumulator lives on
\ the data stack; per-pair "no inversion" is folded in with `and`.
: sorted? ( c -- ? ) {: c :}
    -1
    c size 1 ?do
        i 1- c at  i c at  cmp 0> 0=               \ not inverted ?
        and
    loop ;

\ `sort ( c -- )` — sort the collection IN PLACE by `cmp`.  Insertion
\ sort: simple and obviously correct, O(n^2), fine for the small
\ in-memory collections these protocols target.  Mutates via at!, so
\ the collection must be writable at every index (grid and darray are).
\
\ Inner shift loop is its own word `insert-at-i`: each call gets a
\ fresh set of locals (c, i, key), so sort is fully re-entrant — even
\ a `cmp` method that recursively sorts another collection is safe.
\ The cursor j stays on the data stack, never in a shared variable.
: insert-at-i ( c i -- ) {: c i :}
    i c at {: key :}                                \ key := c[i]
    i                                               \ j := i  (stack)
    begin
        dup 0> if
            dup 1- c at   key cmp 0>                \ j > 0  AND  c[j-1] > key
        else 0 then
    while
        dup 1- c at   over c at!                    \ c[j] := c[j-1]
        1-                                          \ j--
    repeat
    key swap c at! ;                                \ c[j] := key

: sort ( c -- ) {: c :}
    c size 1 ?do  c i insert-at-i  loop ;

\ ── Convenience accessors over the collection protocol ───────────
\
\ Tiny shortcuts.  Read better than `0 c at` / `c size 1- swap at`,
\ and only depend on `size` + `at`, so they work on every collection.

: empty? ( c -- ? )  size 0= ;

\ `first` requires a non-empty collection (no in-bounds fallback —
\ same convention as `min-of` / `max-of`).  Returns the element at
\ index 0.
: first  ( c -- x )  0 swap at ;

\ `last` returns the element at the last index.  Non-empty.
: last   ( c -- x )  dup size 1- swap at ;

\ ── reverse — a fresh collection in reverse order ────────────────
\
\ Uses `new-like` so the result has the SAME backing type as the
\ input (a grid reverses to a grid, a darray to a darray), and fills
\ via `at!`.  The original is untouched.
\
\ Subtle point: a fresh `darray` is empty, and its `at!` grows the
\ backing only on monotonically ascending indices.  So we write the
\ destination IN ORDER (`d[0]` first, then `d[1]`, ...) while pulling
\ from the source in descending order.  For a grid (`new-like` gives
\ a fully-allocated zeroed backing) write order is unconstrained;
\ this same loop just happens to be in ascending dest order.
: reverse ( c -- d ) {: c :}
    c new-like {: dst :}
    c size 0 ?do
        c size 1- i -  c at                        \ x := c[size-1-i]   ( x )
        i dst at!                                  \ d[i] := x          ( )
    loop
    dst ;

\ ── Positional iteration: each-index / map-index ────────────────────
\
\ Like `each` / `map`, but the xt gets the INDEX too — handy when the
\ position matters (printing with numbers, building lookup tables,
\ pairing two collections by index).
\
\ each-index xt: ( i x -- )           map-index xt: ( i x -- y )

: each-index ( c xt -- ) {: c xt :}
    c size 0 ?do
        i  i c at  xt call2
    loop ;

: map-index ( c xt -- d ) {: c xt :}
    c new-like {: dst :}
    c size 0 ?do
        i  i c at  xt call2>                   \ y := xt(i, c[i])
        i dst at!                              \ d[i] := y
    loop
    dst ;

\ ── reduce — fold without an explicit init ─────────────────────────
\
\ Seeds the accumulator with the FIRST element and folds the rest in,
\ so the caller doesn't need a meaningful zero.  Non-empty (same
\ convention as `min-of` / `max-of`).
\
\ reduce xt: ( acc x -- acc )

: reduce ( c xt -- x ) {: c xt :}
    0 c at                                     \ seed := c[0]  (acc on stack)
    c size 1 ?do
        i c at  xt call2>                      \ acc := xt(acc, c[i])
    loop ;

\ ── partition — split into matching / non-matching ─────────────────
\
\ Like `filter`, but you get BOTH the kept and the discarded elements
\ as a pair of darrays — saves a second pass for `filter` + an
\ inverted-predicate filter.  Result is two darrays in matching order.

: partition ( c xt -- yes no ) {: c xt :}
    new-darray {: yes :}
    new-darray {: no :}
    c size 0 ?do
        i c at                                  \ x
        dup xt call1>                           \ x ?
        if  yes d-push  else  no d-push  then
    loop
    yes no ;

\ ── take / skip — prefix / suffix slicing ──────────────────────────
\
\ `take ( c n -- d )` — the first `n` elements as a fresh darray.
\ `skip ( c n -- d )` — everything from index `n` onward.
\
\ Like `filter`, the result is always a darray (the new shape doesn't
\ match the input's; a 2-D grid sliced to a flat sequence is the
\ honest representation).  Both clamp to `size`: `take` of more than
\ exists returns the whole sequence, `skip` of more returns empty.

: take ( c n -- d ) {: c n :}
    new-darray {: dst :}
    n c size min 0 ?do
        i c at  dst d-push
    loop
    dst ;

: skip ( c n -- d ) {: c n :}
    new-darray {: dst :}
    \ Clamp n to size so `?do` doesn't get start > limit (which on
    \ ANS Forth would run the body anyway with the supplied index,
    \ leading to out-of-bounds reads on `at`).
    c size   n c size min   ?do
        i c at  dst d-push
    loop
    dst ;

\ ── concat — append two collections into a fresh darray ────────────
\
\ Same convention: the result is a darray regardless of input shapes,
\ because two grids of different sizes don't add up to a grid.

: concat ( a b -- c ) {: a b :}
    new-darray {: dst :}
    a size 0 ?do  i a at  dst d-push  loop
    b size 0 ?do  i b at  dst d-push  loop
    dst ;

\ ── Set algebra ────────────────────────────────────────────────────
\
\ All four return a FRESH set; the inputs are untouched.  Membership
\ tests use `set-has?` (hash-backed, O(1)) so each algorithm is
\ linear in its scanning input.

\ Set algorithms walk the members darray directly with a `?do` loop.
\ Without a `[ ... ]` quotation literal in our Forth we can't close
\ over locals through `each`, so the loop is explicit — and the per-
\ call state (dst, b) lives in locals, not in module-scope VALUEs.

: set-union ( a b -- c ) {: a b :}
    new-set {: dst :}
    b set-members {: bm :} bm size 0 ?do  i bm at  dst set-add  loop
    a set-members {: am :} am size 0 ?do  i am at  dst set-add  loop
    dst ;

: set-intersect ( a b -- c ) {: a b :}
    new-set {: dst :}
    a set-members {: am :}
    am size 0 ?do
        i am at  dup b set-has?
        if  dst set-add  else  drop  then
    loop
    dst ;

: set-difference ( a b -- c ) {: a b :}
    new-set {: dst :}
    a set-members {: am :}
    am size 0 ?do
        i am at  dup b set-has?
        if  drop  else  dst set-add  then
    loop
    dst ;

: subset? ( a b -- ? ) {: a b :}
    -1
    a set-members {: am :}
    am size 0 ?do
        i am at  b set-has?  and
    loop ;

\ Iteration shorthand for sets (sets aren't positionally indexable
\ themselves, but their `set-members` darray is).
: set-each ( s xt -- )  swap set-members swap each ;

\ ── dict-each — iterate (key, value) pairs ─────────────────────────
\
\ The xt sees both halves of every entry.  Uses `dict-keys` (an O(n)
\ snapshot) so it's safe against concurrent mutation of `d` from
\ inside the xt body.  Walking the keys darray with `?do` lets us
\ close over d and xt as locals — no module-scope VALUE scratch.

: dict-each ( d xt -- ) {: d xt :}
    d dict-keys {: ks :}
    ks size 0 ?do
        i ks at                                    \ key
        dup d dict-at drop                         \ key value
        xt call2
    loop ;
