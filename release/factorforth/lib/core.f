\ core.f — CoreProtocols, Layer 0: the core protocol.
\
\ CoreProtocols is Factor4th's standard class library.  It is written
\ in ordinary ANS Forth on top of the object system (CLASS:, GENERIC:,
\ METHOD:) — nothing here is special-cased in the compiler.  Load it
\ with  S" lib/core.f" INCLUDED  (or it is auto-loaded by the IDE).
\
\ Layer 0 is the root protocol every later class can opt into.  It is
\ deliberately tiny: a handful of generic functions with sensible
\ catch-all defaults, so a class "just works" before you specialise
\ anything, and reads better once you do.
\
\ See docs/coreprotocols.md for the design and diagrams.

\ ── show ──────────────────────────────────────────────────────────
\
\ `show ( x -- )` prints a human-readable rendering of x.  It is the
\ pretty, class-defined view — distinct from DUMP, which is the raw
\ type+bytes debugging view.  Add a METHOD: show for your own classes;
\ the object catch-all keeps `show` total for anything you haven't
\ taught it yet.

GENERIC: show ( x -- )

METHOD: show ( x:object -- )  {: _ :}
    ." <object>" ;

\ `show-ln ( x -- )` is `show` followed by a newline — the common
\ interactive case.  Defined once over the generic, so it works for
\ every class that implements `show`.
: show-ln ( x -- )  show cr ;

\ ── equals? ───────────────────────────────────────────────────────
\
\ `equals? ( a b -- ? )` is value equality — the protocol hook the
\ collection searches (member?, index-of) dispatch through, so they
\ honour whatever equality a class defines.
\
\ The catch-all default is ANS `=`, which is already structural: it
\ compares numbers and characters by value, and (because the substrate
\ does the same) like-shaped objects by their contents.  Override it
\ for a class that wants its own notion of equality — say, comparing
\ only an id slot:
\
\   METHOD: equals? ( a b:account -- ? )  acct>id swap acct>id = ;
\
\ It is distinct from ANS `=` only in being open: your method joins the
\ protocol without touching the library.
GENERIC: equals? ( a b -- ? )

METHOD: equals? ( a b:object -- ? )  = ;

\ ── clone ─────────────────────────────────────────────────────────
\
\ `clone ( x -- copy )` returns an independent copy of x.  The default
\ is a SHALLOW structural copy: it duplicates x's immediate slots, but
\ a slot that holds another object still points at the same object.
\ That's the right default for value-like classes (a point, a colour).
\
\ A class that OWNS a mutable backing store must override clone to copy
\ that store too, or the "copy" will share state with the original —
\ which is exactly why grid and darray (Layer 1) provide their own
\ clone methods.  Numbers and strings clone to an equal value.
GENERIC: clone ( x -- copy )

METHOD: clone ( x:object -- copy )  (clone) ;

\ ── cmp — the ordering protocol ───────────────────────────────────
\
\ `cmp ( a b -- n )` is three-way comparison: it returns a NEGATIVE
\ number if a sorts before b, ZERO if they're equal in order, and a
\ POSITIVE number if a sorts after b.  It's the hook the ordered
\ algorithms (min-of / max-of / sorted? / sort in Layer 1) dispatch
\ through, so they honour whatever order a class defines.
\
\ The catch-all default orders by numeric value, using ANS `<` / `>`.
\ That's right for numbers and characters; a class with its own notion
\ of order overrides it — say, a person by surname, or a card by rank:
\
\   METHOD: cmp ( a b:card -- n )  card>rank swap card>rank swap cmp ;
\
\ Like `equals?`, it is OPEN: your method joins the protocol without
\ touching the library, and every ordered algorithm follows suit.
\ (Named `cmp`, not `compare`, to leave Factor's `math.order:compare`
\ unshadowed.)
GENERIC: cmp ( a b -- n )

METHOD: cmp ( a b:object -- n )
    2dup < if 2drop -1 else > if 1 else 0 then then ;

\ Derived ordering words — written ONCE over `cmp`, so they serve every
\ type that implements it.

\ `before? ( a b -- ? )` — does a sort strictly before b?
: before? ( a b -- ? )  cmp 0< ;

\ `after? ( a b -- ? )` — does a sort strictly after b?
: after?  ( a b -- ? )  cmp 0> ;

\ `lesser ( a b -- x )` — the one that sorts first (a on a tie).
: lesser  ( a b -- x )  2dup before? if drop else nip then ;

\ `greater ( a b -- x )` — the one that sorts last (a on a tie).
: greater ( a b -- x )  2dup after?  if drop else nip then ;

\ ── Character predicates and case (ASCII) ─────────────────────────
\
\ One-liners over ASCII code ranges.  No locale awareness; for app
\ code that needs Unicode, route through the managed-string vocab.
\
\ char-upper? / char-lower?  case-class test
\ letter-char?                upper OR lower
\ digit-char?                 '0'..'9'
\ alphanumeric-char?          letter OR digit
\ whitespace-char?            space, tab, CR, LF

: char-upper? ( c -- ? )  dup 'A' >= swap 'Z' <= and ;
: char-lower? ( c -- ? )  dup 'a' >= swap 'z' <= and ;
: letter-char? ( c -- ? )  dup char-upper? swap char-lower? or ;
: digit-char? ( c -- ? )  dup '0' >= swap '9' <= and ;
: alphanumeric-char? ( c -- ? )  dup letter-char? swap digit-char? or ;
: whitespace-char? ( c -- ? )
    dup ' ' =   over '\t' =  or
    over '\n' = or  swap '\r' = or ;

\ upcase-char / downcase-char — case-flip a single ASCII letter, or
\ pass through unchanged if it isn't one.
: upcase-char   ( c -- c' )  dup char-lower? if 32 - then ;
: downcase-char ( c -- c' )  dup char-upper? if 32 + then ;

\ ── Functional combinators ────────────────────────────────────────
\
\ A pocket toolkit for the "apply this xt to a stack value (or
\ values) and either restore or stack the result" patterns.  These
\ are Factor's combinator family, rendered in ANS Forth on top of
\ our `call1` / `call2` / `call1>` / `call2>` runtime primitives,
\ and named the same as the originals so anyone coming from Factor
\ reads them at sight.
\
\ Two flavours per combinator: the *plain* form (xt has no output
\ — keeps the original on top), and the `>` form (xt produces one
\ output — stacks the result above the original).  Pick the
\ flavour that matches your xt's stack effect.
\
\ Why combinators when locals already work?  For SHORT patterns
\ they read like a single sentence:
\
\     5 ' show ' show-ln bi          ( show 5, then show-ln 5 )
\     pos vel ' . bi@                ( . each of pos and vel )
\
\ Versus the locals form `: foo ... {: x :} x show x show-ln ;`,
\ which is more typing for the same intent.  Use combinators when
\ the body is one line; use locals when it sprawls.
\
\ ── keep / 2keep — call an xt with values; restore them ──────────

\ All combinators bind every input as a local so the locals count
\ matches the declared effect (our emitter uses `locals.len()` as
\ the input count for the `::` form).  The bodies then push the
\ values back as needed — cleaner than the `dup … swap …` stack
\ dance, and obviously re-entrant.

\ keep ( x xt -- x ) — call `xt ( x -- )`; x remains on top.
: keep ( x xt -- x ) {: x xt :}
    x xt call1  x ;

\ keep> ( x xt -- y x ) — call `xt ( x -- y )`; result above x.
\ Useful when you want the rendering AND the original of the same
\ value, e.g. for diffing or logging.
: keep> ( x xt -- y x ) {: x xt :}
    x xt call1>  x ;

\ 2keep ( x y xt -- x y ) — call `xt ( x y -- )`; both preserved.
: 2keep ( x y xt -- x y ) {: x y xt :}
    x y xt call2  x y ;

\ ── bi / bi> — apply two xts to the same value ───────────────────

\ bi ( x p q -- ) — call `p ( x -- )` then `q ( x -- )`.
\ Useful for "do two side-effects on the same value", e.g.
\ `5 ' show ' show-ln bi`.
: bi ( x p q -- ) {: x p q :}
    x p call1  x q call1 ;

\ bi> ( x p q -- a b ) — both xts produce a value; results stacked
\ left to right (a is the result of p, b of q).
: bi> ( x p q -- a b ) {: x p q :}
    x p call1>  x q call1> ;

\ ── bi@ / bi* — same xt to two values, or two xts to two ─────────

\ bi@ ( x y q -- ) — apply the SAME xt `q ( v -- )` to each of x
\ and y, in order (x first, then y).
: bi@ ( x y q -- ) {: x y q :}
    x q call1  y q call1 ;

\ bi* ( x y p q -- ) — apply DIFFERENT xts: p to x, q to y.
: bi* ( x y p q -- ) {: x y p q :}
    x p call1  y q call1 ;

\ ── tri — three-way apply ────────────────────────────────────────

\ tri ( x p q r -- ) — apply p, q, r in order, all to x.
: tri ( x p q r -- ) {: x p q r :}
    x p call1  x q call1  x r call1 ;
