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
