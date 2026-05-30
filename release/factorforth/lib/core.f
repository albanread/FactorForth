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

METHOD: show ( x:object -- )
    drop ." <object>" ;

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
