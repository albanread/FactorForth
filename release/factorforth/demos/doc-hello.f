\ doc-hello.f - the simplest doc-pane demo
\
\ Opens a Forth-writable Markdown pane, sets a document, then appends
\ more text to show live updates.  This exercises the doc-* FFI:
\
\   Forth code  ->  rt_doc_*  ->  igui::doc_pane  ->  docpane render
\                                              ->  GUI thread paints.
\
\ Unlike a gpane (immediate-mode drawing) a doc-pane owns its source:
\ doc-set replaces it, doc-append streams onto the end, and the pane
\ re-parses + repaints itself each time.
\
\ Note: S" reads its body literally up to the next ", so the real
\ line breaks below land in the string — exactly what Markdown wants.

: doc-hello ( -- )
    cr ." opening a markdown doc-pane..." cr
    S" Hello Doc" doc-open
    dup 0= if
        drop ." doc-pane open failed" cr
    else
        >r   \ stash the child id on the return stack

        \ Set the initial document.
        S" # Hello from Forth

This page was written by a **Forth** program through `doc-set`.

- Markdown is parsed by the shared docpane core
- The same renderer drives the Help browser
- `doc-append` can stream more text in later
" r@ doc-set

        \ Append a second section onto the same document.
        S"
## Appended

This paragraph arrived via `doc-append`, and the pane
re-laid-out itself.
" r@ doc-append

        r> drop
        ." done — the pane shows the rendered markdown" cr
    then
;

." Loaded: doc-hello.  Try 'doc-hello'" cr
