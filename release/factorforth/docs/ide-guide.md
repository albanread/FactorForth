# IDE guide

The FactorForth IDE is an MDI (multiple-document interface)
window hosting several pane types.  The frame is a single
Direct2D surface — every pane renders into it via DirectWrite.

## Panes

### Forth console

The default pane that opens at startup.  A REPL with line
history, syntax-coloured prompt, scrollback.

- **Enter** at the prompt evaluates the line.
- **Ctrl+Up / Down** walks the history.
- **PageUp / PageDown** scrolls the buffer.
- Mouse-select copies (Ctrl+C is unnecessary; selection IS the
  clipboard).

### Editor

A multi-line buffer for programs longer than one line.

- **F5** evaluates the whole buffer.
- **Ctrl+Enter** evaluates the current line.
- **Ctrl+S** saves to disk (prompts for path the first time).
- **Ctrl+O** loads a `.f` file.

### Data stack

Read-only view of the current data stack.  Refreshes after each
eval.  Useful when you want to see the effect of a word without
calling `.s` manually.

### Log

Internal diagnostic stream.  Usually empty; populated on errors
or restarts.

## Menus

### File

- **Open...** — load a `.f` file into the Editor.
- **Save / Save As...** — write the Editor's buffer to disk.
- **Exit** — close the IDE.

### Edit

- Standard cut/copy/paste/undo on the Editor pane.

### Tools

- **Console** (Ctrl+Shift+R) — open a new Forth console.
- **Editor** (Ctrl+Shift+E) — open the source editor.
- **Data Stack** — open the stack viewer.
- **Log** — open the log pane.
- **Restart Forth** (Ctrl+Shift+F5) — kill the current session
  and start a fresh one.  Useful after editing a word
  incorrectly or wedging the stack.

### Demos

Lists everything in `demos\` next to the .exe.  Click to load.

### Help

- **Documentation** — opens `doc-crate.exe` against the bundled
  `docs\` folder.
- **About** — version, build date, credits.

## Keyboard shortcuts

| key                 | does                                          |
|---------------------|-----------------------------------------------|
| Enter (console)     | evaluate current line                         |
| Ctrl+Up / Down      | navigate console history                      |
| F5 (editor)         | evaluate whole buffer                          |
| Ctrl+Enter (editor) | evaluate current line                          |
| Ctrl+S / O          | save / open file in editor                     |
| Ctrl+Shift+R        | open console pane                              |
| Ctrl+Shift+E        | open editor pane                               |
| Ctrl+Shift+F5       | restart Forth session (fresh image)            |
| Ctrl+W              | close current pane                             |
| Ctrl+Tab            | cycle through open panes                       |
| F1                  | open documentation                             |

## Crash recovery

If a Forth program crashes the session (memory access, fatal
runtime error), the IDE catches it, displays a crash report,
and starts a fresh session.  Your previously-defined words
persist in the session worker's compile context — they'll be
available again immediately.

The three layers:

1. **Forth-level errors** (undefined word, type mismatch, stack
   underflow) are caught inside the VM, formatted as `ANS error
   -N: ...` and printed to the console.  Session stays alive.

2. **Rust panics** (compiler bug, channel breakdown) are caught
   by `catch_unwind`.  Crash report + auto-restart.

3. **Hardware traps** (segfault, divide-by-zero from `0 /`)
   are caught by a structured exception handler.  The current
   worker thread dies; a fresh one is spawned.

You can't crash the IDE from within a Forth program.  At worst
the session re-boots.

## Layout under the hood

```
factorforth-ui.exe        (single Windows process)
+-- GUI thread            Direct2D MDI, Win32 message pump
|       ^- IGuiEvent MPSC channel
+-- IDE worker            receives events, drives Session
|       ^- Command / EvalResult channels
+-- Session worker        owns the Factor VM (in-process)
        ^- eval-callback flows output back through MPSC
```

The Factor VM is single-threaded and TLS-resident — every call
into it has to happen on the thread that initialised it.  We
own that thread (the Session worker) and route requests in via
channels.  This is why the IDE never hangs on `KEY` (waiting
for user input from the VM): the GUI thread keeps pumping
messages while the VM blocks.
