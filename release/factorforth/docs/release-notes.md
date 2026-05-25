# Release notes

## v0.1.0 — 2026-05-25

First public release of FactorForth.

### What ships

- **factorforth-ui.exe** — IDE binary, 1.5 MB.  Direct2D /
  DirectWrite MDI front-end, defaults to GUI mode, embedded
  Windows manifest for per-monitor v2 DPI awareness, UTF-8
  active code page, supportedOS through Windows 11.
- **factor.dll** — patched Factor VM, 220 KB.  Embedding API
  (`nf_init_factor`, `nf_eval_string`) added on top of stock
  Factor.
- **factorforth.image** — bootstrapped Factor image, 134 MB.
  Includes forth.runtime, forth.wf64-gfx, and the standard
  Factor vocabularies.
- **doc-crate.exe** — bundled documentation browser, 0.6 MB.
- **demos/** — five sample programs.
- **docs/** — full user documentation, ~50 KB of markdown.

### Language features

- ~95% of ANS Forth Core word set.
- ANS booleans (`-1` / `0`), floored `MOD`, 64-bit cells.
- Control flow: `IF/ELSE/THEN`, `BEGIN/UNTIL/WHILE/REPEAT`,
  `DO/LOOP/+LOOP/I/J/LEAVE`, `CASE/OF/ENDOF/ENDCASE`.
- Defining words: `:` `;` `CONSTANT` `VARIABLE` `CREATE/ALLOT`
  `CREATE/DOES>`, `' (tick)` + `EXECUTE`.
- Strings: ANS `c-addr u` form AND the `$-suffix` managed-string
  vocab (FactorForth extension).
- Pictured number output: `<# # #S SIGN HOLD #>`.
- File access: `INCLUDED`.
- Forth 2012 test-runner: `T{ ... -> ... }T` blocks.

### FactorForth extensions

- **LET algebra** — infix math DSL with proper precedence,
  unary minus, trig/log functions.
- **`S$" ... "`** — managed-string literals.
- Persistent REPL state — definitions, variables, templates,
  and **data-stack values** all survive across evals (the
  "you can leave 5 on the stack and consume it next line"
  property).

### IDE features

- MDI panes: Forth console, source editor, data-stack viewer,
  log view.
- Demos menu auto-populates from `demos/`.
- Help → Documentation launches doc-crate against `docs/`.
- Crash recovery: Forth errors caught and reported as ANS error
  codes; Rust panics auto-respawn the IDE worker; hardware
  traps caught by a vectored exception handler.
- Ctrl+Shift+F5 — fresh session (your definitions persist via
  the compile context).

### REPL semantics

The IDE uses a Factor-listener-style persistent session: ONE
long-running Factor function loops, reading commands from the
host queue, threading the data stack as a value through each
iteration via `with-datastack`.  Mirrors basis/listener/listener.factor.

This means:

- **Values left on the stack persist across evals.**  Type `5`,
  see it stay; type `dup .` later, prints `5 5`.
- **Definitions, variables, constants, templates persist.**
- **Forth-level errors are caught**: no-method, generic-failure,
  type-mismatch tuple errors print as `ANS error -N` and the
  session stays alive.

### Known limitations

- **Kernel-level stack underflow crashes the process.**  Typing
  `.` on a truly empty stack (or `+` with fewer than 2 items)
  triggers a VM-level underflow that walks past the alien-callback
  boundary via Factor's `unwind_native_frames`.  Tracked as #47/#48.
  Workaround: always make sure your stack has the items a word
  expects before running it.  The Data Stack pane (Tools menu)
  lets you check.

- **Windows only**.  The iGui front-end uses Direct2D /
  DirectWrite; ports to other platforms are out of scope for
  this milestone.

- **`.` on empty stack crashes the IDE.**  See above under
  Known limitations.  This is the one rough edge in an
  otherwise persistent REPL.
- **Some Core stragglers**: `U<` `U>` `COUNT` `KEY?` `EXIT`
  `.S` `MOVE` `PICK` `ROLL` not yet shipped (use
  `language-reference.md`'s alternatives).
- **`?DUP`** has a polymorphic effect that Factor's inference
  rejects; needs an emit-time rewrite (tracked).
- **`DEFER` / `IS`** not shipped — use `' name VARIABLE+EXECUTE`
  pattern instead.
- **Hardware-trap recovery** is partial.  Forth-level errors
  recover cleanly; some VM-level traps still terminate the
  current worker thread (a fresh one spawns).

### Tests

199/199 session-based tests pass.  See `current_status.md` in
the source repo for the detailed test inventory.

### Acknowledgments

Built on Factor (Slava Pestov and contributors).  iGui MDI
front-end ported from the WF64 project.  DocCrate written as
a sibling tool.

— v0.1.0
