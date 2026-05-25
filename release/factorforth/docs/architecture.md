# Architecture

FactorForth is two pieces glued together:

1. A Rust-written **compiler** that reads ANS Forth source and
   emits Factor source code as its intermediate representation.
2. The **Factor VM**, patched with an embedding API, loaded as
   a Windows DLL and driven from Rust via callbacks.

The IDE is a third piece on top: an MDI Direct2D / DirectWrite
front-end borrowed from the WF64 project.

## Compiler pipeline

When you type a Forth line and hit Enter, this is what happens:

```
   "  : square dup * ;  3 4 + .  "
            |
            v
   +------------------+
   | lex              |  tokens: COLON IDENT NUM SEMI ...
   +------------------+
            |
            v
   +------------------+
   | parse            |  AST: Definition(square, [dup, *])
   |                  |       TopLevel([3, 4, +, .])
   +------------------+
            |
            v
   +------------------+
   | resolve          |  every name -> built-in vocab word OR
   |                  |  user-defined word OR an error
   +------------------+
            |
            v
   +------------------+
   | effect-check     |  square: ( a -- a )   inferred from dup *
   |                  |  toplevel: ( -- )      everything balances
   +------------------+
            |
            v
   +------------------+
   | sema             |  escape analysis on variables, template
   |                  |  detection, $-string literal lifting,
   |                  |  LET expansion
   +------------------+
            |
            v
   +------------------+
   | emit             |  Factor source IR:
   |                  |    USING: ... ;
   |                  |    : square ( a -- a ) dup * ;
   |                  |    3 4 + .
   +------------------+
            |
            v
   +------------------+
   | Session::eval    |  hand IR to the Factor VM, capture
   |                  |  any output, return errors
   +------------------+
```

Each phase is a separate module under `src/compiler/`.  Each is
testable in isolation; each can produce a dump for diagnostics
(`newfactor --dump=tokens|ast|effects|ir source.f`).

## Why Factor?

Three reasons:

1. **It's a real, mature VM** with a GC, threads, an FFI, a
   debugger, native compilation, a 2000+ vocabulary stdlib.
   We don't have to write any of that.

2. **Its IR is close to Forth's**.  Factor's surface syntax is
   stack-shuffle words, quotations, defining words — the same
   shapes ANS Forth uses.  Compiling Forth-to-Factor is mostly
   renaming and adding stack-effect annotations.

3. **The VM already supports embedding**.  Factor's VM is
   normally driven by `factor.exe`; with a small patch
   (`vm/factor.cpp` — exposing `nf_init_factor`,
   `nf_eval_string`, etc.) we can `LoadLibrary` it from Rust
   and drive it via callbacks.

## VM integration

```
factorforth-ui.exe         (the Rust binary you launched)
+- libloading::Library     (LoadLibrary on factor.dll)
|     ^- vtable of nf_* embedding API entry points
+- Session worker thread   (owns the VM; single-threaded by design)
|     +- nf_init_factor    (boot the image)
|     +- nf_eval_string    (compile + run user source)
|     +- OBJ_EVAL_CALLBACK (our custom Forth callback)
|           +- parse-string + with-datastack
|           +- nf-format-error (ANS-style error messages)
+- GUI thread              (Direct2D MDI message pump)
```

The VM is single-threaded and TLS-resident.  Every call into it
must happen on the thread that initialised it.  We own that
thread — the Session worker — and route requests in via
channels.  The GUI thread keeps pumping Windows messages while
the VM blocks on `KEY` or a long compile, so the IDE never
freezes.

## I/O routing

Forth `EMIT` / `KEY` / `TYPE` / `."` go through three extern C
functions exported by factorforth-ui.exe:

```
nf_rt_write_char    KEY                   nf_rt_read_char
nf_rt_read_line     TYPE / EMIT
```

These are exported via build.rs `/EXPORT:` linker args.  Factor
finds them via `GetProcAddress(GetModuleHandle(NULL), ...)`.

Inside the host they fan out by `IoMode`:

- **`Test`** — pre-fed input vec, captured output vec.  Used by
  the test suite.
- **`Terminal`** — stdin / stdout.  Used by CLI tools.
- **`Gui`** — caller-provided closure.  The IDE wires this to
  the Forth console pane.

## REPL persistence

The IDE's REPL behaves like a real Forth listener — values left
on the stack persist across evals, definitions accumulate.

Stack persistence is handled by a custom OBJ_EVAL_CALLBACK
that uses `parse-string` + `with-datastack` to save / restore
the data stack around each callback invocation, instead of
the stock `eval>string` which enforces `( -- )` net effect.

Word / variable / template persistence is handled by a
`CompileContext` in Rust that threads `user_words`,
`user_effects`, and `templates` across compiles.  Every eval
sees every name from every previous eval.

## Crash recovery

Three layers:

1. **Forth errors** — caught inside the VM by our custom
   `nf-format-error`.  Printed as `ANS error -N: ...`.
   Session stays alive.

2. **Rust panics** — `std::panic::catch_unwind` around the IDE
   worker.  Crash report displayed, fresh session spawned.

3. **Hardware traps** — vectored exception handler at the
   thread level.  Crashed thread is replaced; the IDE never
   takes down the whole process.

## Project layout

```
NewFactor/                    (source repo)
+- src/                       Rust code
|  +- compiler/               compiler pipeline modules
|  +- session.rs              VM session + I/O routing
|  +- bin/newfactor_ui.rs     IDE entry point
+- factor/                    Forth-side vocabs loaded into the image
|  +- forth/runtime/          ANS Forth runtime support
|  +- wf64-gfx/               graphics FFI (gpane-*, ev-*)
+- vm-build/factor.dll        the patched Factor VM
+- images/factorforth.image   the bootstrapped image
+- release/factorforth/       shippable folder (this is what you have)
+- tests/                     ~250 Rust tests
+- docs/                      developer docs (manifesto, journal)
```

The release folder ships:

```
release/factorforth/
+- factorforth-ui.exe         the IDE
+- factor.dll                 patched Factor VM
+- factorforth.image          the image (forth.runtime baked in)
+- doc-crate.exe              the doc browser
+- demos/                     sample .f files
+- docs/                      what you're reading
+- README.txt                 quick reference
```

That's the whole picture.  Read `getting-started.md` to use it,
or `language-reference.md` to look up words.
