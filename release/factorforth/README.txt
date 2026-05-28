Factor4th - ANS Forth IDE on Factor's VM
========================================

Quick start
-----------
Double-click factorforth-ui.exe.  The IDE opens straight to a
Forth console pane.  Type at the > prompt and press Enter:

    : square dup * ;
    7 square .

The Demos menu loads small programs you can play with.  Tools
menu opens the editor, the data-stack viewer, and the log pane.
Help -> Documentation opens the bundled docs in doc-crate.

Layout
------
    factorforth-ui.exe  - the IDE binary (drag a shortcut anywhere)
    factor.dll          - the patched Factor VM (loaded at boot)
    factorforth.image   - the Factor image: forth.runtime,
                          forth.wf64-gfx, and all standard vocabs
    doc-crate.exe       - the documentation browser
    demos\              - sample programs reachable via the Demos menu
    docs\               - README, language reference, tutorials

(Binary and image filenames keep their FactorForth prefix from
the earlier name; the product is now called Factor4th — see
docs/release-notes.md for the rename note.  A future release
will harmonise the filenames.)

Where things live at runtime
----------------------------
factorforth-ui.exe looks for factor.dll and factorforth.image
next to itself first, then falls back to the development repo
layout if it can't find them.  You can move the folder anywhere
on disk as long as the layout above stays intact.

What's inside
-------------
- A Rust ANS Forth compiler that emits Factor IR.  Every word
  you type or load goes through lex -> parse -> desugar passes
  (lower_qdup, lower_recurse, lower_exit) -> resolve ->
  effect-check -> sema -> emit, then the IR is handed to the
  embedded Factor VM.

- 95%+ of the ANS Forth Core word set, plus Factor4th
  extensions: LET algebra DSL, managed strings ($-suffix vocab),
  S$" string literals, Forth 2012 test-runner support,
  polymorphic VALUE / TO over Factor's tagged stack, TYPEOF +
  type predicates.

- An iGui MDI front-end (Direct2D / DirectWrite) borrowed from
  the WF64 project: REPL pane, source editor, log view, crash
  recovery, doc browser integration.

- Per-monitor v2 DPI awareness, UTF-8 active code page, common
  controls v6 visual styles (see the embedded manifest).

License
-------
BSD-3-Clause.  factor.dll and factorforth.image incorporate
portions of Factor (Copyright Slava Pestov and contributors,
BSD license).  iGui MDI front-end shared with WF64 (same
license).  See docs\license.md for the full text.

More
----
See docs\index.md for the table of contents:
- getting-started.md   - your first hour in the IDE
- forth-tutorial.md    - learn Forth from scratch
- language-reference.md - every Factor4th-specific word
- ide-guide.md         - panes, menus, keyboard shortcuts
- architecture.md      - how the compiler + VM fit together
