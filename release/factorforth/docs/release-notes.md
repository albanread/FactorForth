# Release notes

## Unreleased (since v0.1.0)

**CoreProtocols** — the standard class library — landed Layers 0–3,
written in ordinary ANS Forth on the object system and shipped as
loadable source under `lib/`. Each layer has a reference page in
`docs/`; see [CoreProtocols](coreprotocols.md) for the design and layer
map.

- **Layer 0 · core protocol** (`lib/core.f`) — `show` / `show-ln`,
  `equals?`, and `clone`, each a generic with a total `object`
  catch-all (pretty-print, structural equality, shallow copy). The root
  protocol every later class opts into. → [Core protocol](core.md)
- **Layer 1 · collections** (`lib/collections.f`) — the collection
  protocol (`size` / `at` / `at!` / `new-like`) over `grid`, `darray`,
  `dict`, and `set`, plus the algorithms written once against it:
  `each` / `map` / `filter` / `fold` / `tally` / `any?` / `all?` /
  `find` / `sum` / `product`, and the `equals?`-based `member?` /
  `index-of`. → [Collections](collections.md)
- **Layer 2 · numerics** (`lib/numerics.f`) — a shared arithmetic
  protocol (`v+` / `v-` / `vscale` / `vmag`) over `vec2` and `complex`,
  with `v+` / `v-` keying on **both** arguments (multiple dispatch).
  Method bodies are written in LET; derived `vneg` / `vdist` / `vlerp`
  / `vmid` work for every protocol type. → [Numerics](numerics.md)
- **Layer 3 · text & streams** (`lib/streams.f`) — a `string` value
  type (which joins the collection + core protocols) and the STREAM
  protocol (`read-char` / `write-char`) whose signature idea is that
  end-of-file is an **object** (`<eof>`), not a flag. Readers/writers,
  `split` / `join` / `read-line`, and the protocol-derived
  `copy-stream` / `read-all`. → [Text & streams](streams.md)

Layers 4 (Files) and 5 (GUI & events) are designed but **not yet
shipped**; graphics is currently reached through the `gpane-*` FFI
primitives, not a CLOS event protocol.

New syntax: **character literals** — `'a'` (97), `','` (44), `' '`
(32), plus the backslash escapes `'\n'` / `'\t'` / `'\r'` / `'\0'` /
`'\s'` / `'\e'` / `'\\'` / `'\''` / `'\"'`. Each pushes a character's
byte code; it's sugar for the integer, so it composes anywhere a number
does — idiomatic for delimiters (`',' split`) and ASCII work. The
closing quote distinguishes it from `'` the tick. See the
[language reference](language-reference.md#literals).

Object system: every class now exposes a membership predicate
`classname?` ( x -- ? ), backed by Factor's auto-generated tuple
predicate (respects inheritance). The class docs are also corrected —
accessors for inherited slots (`colored-point>x`) and full
multi-method dispatch both ship; earlier "Sprint 1" caveats denying
them were stale. See [Classes and methods](classes.md).

Compiler fix: small integer powers in LET (`x^2`, `x^3`, …) now compile
as repeated multiplication rather than float `^`, so squaring a
negative value no longer drifts into the complex plane (#80).

## v0.1.0 — 2026-05-25

First public release of Factor4th.

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
  vocab (Factor4th extension).
- Pictured number output: `<# # #S SIGN HOLD #>`.
- File access: `INCLUDED`.
- Forth 2012 test-runner: `T{ ... -> ... }T` blocks.

### Factor4th extensions

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
- **Some Core stragglers**: `U<` `U>` `COUNT` `KEY?` `MOVE`
  `PICK` `ROLL` `.R` `2R@` `:NONAME` not yet shipped (use
  `language-reference.md`'s alternatives). (`EXIT` and `.S` were
  listed here at v0.1.0 but have since shipped.)
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
