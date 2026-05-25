# 2026-05-25 — FactorForth IDE becomes real

A long arc.  This was the day FactorForth stopped being "a
compiler that emits Factor IR" and became "a usable interactive
Forth IDE that paints pixels, persists state, recovers from
errors, and interrupts runaway loops" — all from inside a
single GUI binary the user double-clicks.

## What ships today

```
release/factorforth/
├── factorforth-ui.exe   (1.5 MB, GUI subsystem, manifest-embedded)
├── factor.dll           (221 KB, patched VM with SEH-wrapped eval)
├── factorforth.image    (134 MB, listener loop + ANS Forth runtime)
├── doc-crate.exe        (604 KB, bundled markdown browser)
├── demos/               (7 files — gfx-shapes, fibonacci, let-algebra, …)
├── docs/                (12 files — getting-started, language-reference, …)
└── README.txt
```

## The arcs in roughly the order they happened

### 1. Rebrand: NewFactor → FactorForth

The IDE binary is now `factorforth-ui` (was `newfactor-ui`).  Title
bar reads "∴ FactorForth — Forth IDE".  Manifest embedded:
per-monitor-v2 DPI, Common Controls v6, UTF-8 active code page,
supportedOS through Windows 11.  Image renamed
`nf-mandelbrot.image` → `factorforth.image` (the old name was a
relic of M#10 milestone).  Release subsystem is now Windows-GUI
(no console flash on launch); debug builds still inherit the
parent's console for `eprintln!` traces.

### 2. The release folder + DocCrate integration

Mirrors WF64's release pattern.  `Help → Documentation` launches
`doc-crate.exe` against the bundled `docs/` folder — fully
self-contained markdown browser, no internet round-trip.  Initial
docs cover: getting-started, forth-tutorial, ide-guide,
language-reference, stack-effects, let-algebra, managed-strings,
architecture, release-notes, license.

### 3. Listener architecture (THE structural win)

Previously every `session.eval` did a fresh `nf_eval_string` call
through `OBJ_EVAL_CALLBACK`.  Each call crossed Factor's
alien-callback boundary, which zeroes the data stack via
`with-callback-frame`.  Net result: anything left on the stack
was wiped between evals; the persistent-residue REPL UX was
impossible.

Mirrored Factor's own `basis/listener/listener.factor:listener-step`
instead.  Now there's ONE long-running Factor function:

```factor
: nf-listener-loop ( datastack -- )
    nf_rt_next_command dup [
        nf-listener-eval
        dup nf-publish-datastack
        nf_rt_command_done
        nf-listener-loop
    ] [ 2drop ] if ;

:: nf-listener-eval ( datastack source -- datastack' )
    [ source parse-string :> quot
      datastack quot with-datastack ]
    [ nf-format-error flush datastack ]
    recover ;
```

The data stack flows through as a Factor value via `with-datastack`,
exactly like Slava's listener.  All evals run inside a single
`nf_eval_string("nf-listener-start")` call that blocks forever
in Factor — Rust talks to it over a host-side channel + condvar
pair (`listener_pending`, `listener_done`) accessed via three
new FFI exports: `nf_rt_next_command`, `nf_rt_command_done`,
plus the stack-snapshot trio.

Files:
- `factor/forth/runtime/runtime.factor` — added nf-listener-{start,
  loop, eval, publish-datastack}, plus the FFI declarations
- `src/session.rs` — added a dispatcher sibling thread that
  translates `Command::Eval` requests into queue pushes
- `build.rs` — exports the new FFI symbols

### 4. The FFI export trap

The very first run of the listener architecture hung for 20 seconds
per eval, then reported "session timed out".  The user pushed
hard for logging — I added a file-based trace
(`factorforth.log` next to the exe) that timestamps every key
event across threads.

The log instantly showed:

```
[ 31932ms] eval_inner returned, captured 125 bytes:
  "Cannot resolve C library function
   Library: f
   Symbol: nf_rt_next_command
   DlError: The specified procedure could not be found."
```

`build.rs` had `let bins_with_session = ["newfactor"];` — only
the CLI got the `/EXPORT:nf_rt_*` linker args.  The IDE binary
had the FFI symbols in its `.text` section but **not in its
export table**, so Factor's `GetProcAddress` couldn't find them.
Tests passed because the test framework gets exports
unconditionally via `cargo:rustc-link-arg-tests`.

Lesson: **logging at the boundary turned a 20-second-then-die
mystery into a 30-second diagnosis.**  Without the log it would
have been hours of guessing.  The same trace later surfaced the
graphics FFI byte-array/alien mismatch in one round (`expected
type: alien, actual class: byte-array`).

### 5. Output routing (console vs REPL pane)

When the user typed in the REPL pane, eval output was landing in
the console pane because `IoMode::Gui`'s `on_write` closure was
hardcoded to `fconsole::append`.  Added a shared
`Mutex<OutputSink>` enum to the IDE binary:

```rust
enum OutputSink { Console, Repl { child_id: i64 } }
static CURRENT_SINK: OnceLock<Mutex<OutputSink>>;

fn deliver_line(line: String) {
    match *CURRENT_SINK.lock().unwrap() {
        OutputSink::Console => fconsole::append(&line),
        OutputSink::Repl { child_id } =>
            repl_pane::append(child_id, line, AppendKind::Output),
    }
}
```

`handle_eval` calls `set_sink(Console)` before
`session.eval`; `handle_eval_repl` calls `set_sink(Repl {
child_id })`.  The closures are mutex-mediated; the IDE worker
thread writes the sink, the session worker thread reads it.
Worker is single-threaded so no race possible — eval blocks until
output is fully drained.

Also added `on_flush` to `IoMode::Gui` so partial-line output (Forth's
`.` emits `"42 "` with no trailing newline) gets pushed at end of
eval rather than sitting in the line buffer forever.

### 6. Live stack pane

The Tools → Stack pane existed but wasn't wired.  Added:

```factor
: nf-publish-datastack ( datastack -- )
    nf_rt_stack_begin
    [ dup fixnum? [ nf_rt_stack_item ] [ drop ] if ] each
    nf_rt_stack_end ;
```

Called after every listener iteration.  Three new FFI exports
collect items into a thread-local `Vec<i64>` between begin/end,
then `wf64::igui::stack_view::publish` ships the snapshot to the
GUI.  Top-of-stack-first ordering (Factor's bottom-up, reversed
on publish).  Non-fixnum values (quotations, strings, tuples)
are filtered — a richer marshalling story for later.

Bonus: dropped the 8-cell sentinel-zero startup we'd added as
underflow padding, since #10 (below) made it unnecessary.

### 7. Graphics FFI end-to-end

The `gpane-*` / `ev-*` words from `forth.wf64-gfx` weren't in the
resolver, and the underlying `rt_gpane_*` C symbols weren't in
the binary's export table.  Both fixed:

```rust
// build.rs
"rt_gpane_open", "rt_gpane_begin", "rt_gpane_present",
"rt_gpane_clear", "rt_gpane_fill_rect", "rt_gpane_stroke_rect",
"rt_gpane_line", "rt_gpane_fill_circle", "rt_gpane_next_event_for",
```

```rust
// resolve.rs — 18 new entries
("gpane-open",        QualifiedBuiltin { vocab: "forth.wf64-gfx", … }),
…
("ev-frame-close",    QualifiedBuiltin { vocab: "forth.wf64-gfx",
                                         factor_name: "EV_FRAME_CLOSE" }),
```

Then immediately hit the **byte-array / alien type-check** wall.
`gpane-open` was passing `S" title"`'s byte-array through
`nf-addr-raw` → `>c-ptr alien-address`.  But Factor's
`alien-address` strictly rejects byte-array-backed pointers
(vm/alien.cpp:14: `if (to_boolean(ptr->base)) type_error(ALIEN_TYPE,
obj);`) — byte-array data can be moved by GC, so taking its raw
address is unsafe.

Fix: declare the FFI parameter as `void*` instead of `longlong`.
Factor's marshaller then auto-pins the byte-array for the call
duration and passes its data pointer.  The wrapper becomes
just `c-addr ba>>` instead of `c-addr nf-addr-raw`.  Found this
in **one round** thanks to the improved error formatter (next item)
which printed `expected type: alien, actual class: byte-array`
instead of "kernel error 3".

The end-to-end graphics path now bounces correctly through
three thread boundaries:

```
Session worker (Factor):  rt_gpane_fill_rect
                            → batch::push(SurfaceCmd::FillRect)  [Mutex<PaneBatch>]
                          rt_gpane_present
                            → batch::submit → PostMessageW(WM_PAINT)
GUI thread:               WndProc → executor::execute(batch)
                            → Direct2D commands
                            → swap chain presents
                            → photons on glass
```

WF64's batch machinery already handled the bounce; we just had
to expose the symbols and add resolver entries.

### 8. fconstant accepts computed expressions

`3.5e 240e f/ fconstant mb-dx` used to fail with "only literal
values are supported in this milestone".  Added a `Computed(Vec<Expr>)`
variant to `ConstValue`; the parser accepts any non-literal
pending-expression chain; emit produces:

```factor
: mb-dx ( -- f ) 3.5 240.0 forth.runtime:f/ ; inline
```

Factor's compiler constant-folds the pure body to the same
machine code as the literal form — no runtime overhead.
Updated resolve, sema, dump to walk into computed-value bodies
so word references inside resolve correctly.

### 9. Better kernel-error formatter

`nf-format-kernel-error` used to print bare "kernel error 3" for
anything it didn't match.  Now decodes all 19 VM error codes
from `vm/errors.hpp` with payload extraction:

- code 3 (type-check) shows expected class + actual obj + actual class
- code 4 (divide-by-zero) → "ANS error -10"
- code 7 (fixnum range) shows the offending value
- code 8 (FFI) shows detail string
- code 9 (undefined symbol) shows symbol name
- codes 10–15 (stack under/overflow variants) → ANS-numbered messages
- code 17 (FP trap) → "ANS error -42"
- code 18 (interrupt) → "ANS error -28"

Wrapped `present` in a recover so weird tuple shapes don't crash
the formatter, and added `nf-unwrap-error` to peel `lexer-error`
and `condition` layers.

### 10. The big one — SEH installation around the eval-callback

User's reported symptom: typing `.` on an empty stack crashed
the session, IDE supervisor respawned, conversation lost.

Diagnostic path (this took the longest):

1. **First hypothesis**: WF64's VEH was racing Factor's SEH on
   access violations and winning (VEH always runs before SEH).
   Unregistered the session worker from WF64's VEH.
2. **Still crashed**.  Process actually died instead of being
   contained by WF64's catch.  So it wasn't a VEH race — Factor's
   SEH wasn't there at all.
3. **Discovery**: `factor.cpp:factor_eval_string` calls the
   eval-callback as a direct function pointer.  This **skips**
   `c_to_factor_toplevel` — which is the function that calls
   `RtlAddFunctionTable` to install the SEH unwind table for
   Factor's JIT'd code on Windows x64.
4. Without an SEH unwind table, Factor's `exception_handler`
   never gets dispatched.  Page faults inside the callback have
   no SEH handler to find, so Windows treats them as unhandled
   exceptions and terminates the process.

The fix needed to be inside `factor.dll` itself.  Patched the VM:

```cpp
// vm-build/vm/os-windows-x86.64.cpp
void factor_vm::install_seh_table() { /* RtlAddFunctionTable … */ }
void factor_vm::uninstall_seh_table() { /* RtlDeleteFunctionTable … */ }
void factor_vm::c_to_factor_toplevel(cell quot) {
    install_seh_table(); c_to_factor(quot); uninstall_seh_table();
}

// vm-build/vm/factor.cpp
char* factor_vm::factor_eval_string(char* string) {
#if defined(WINDOWS) && defined(FACTOR_64)
    install_seh_table();
#endif
    /* invoke eval-callback */
#if defined(WINDOWS) && defined(FACTOR_64)
    uninstall_seh_table();
#endif
    return result;
}
```

Rebuilt factor.dll via vcvars64 + nmake.  After redeployment:

```
> .
ANS error -4: Stack underflow
> 21 21 + .
42
>
```

**Single fix, three task items resolved simultaneously:**

- #47 (kernel underflow crashes process) — now caught
- #48 (hardware-trap recovery) — now caught
- #51 (enable nf_enqueue_interrupt) — now works, because the
  interrupt mechanism relies on Factor's safepoint-guard
  page-fault being caught by the SEH handler

Tested all three:

```
.                              → ANS error -4: Stack underflow
42 0 /                         → ANS error -10: Division by zero
begin 1 drop again  (after 2s) → ANS error -28: Interrupt
```

…and after each, the listener keeps prompting normally.

## Architectural reflection

These weren't 10 unrelated fixes.  Most of them were the same
class of mistake repeating: **we were embedding Factor via a
narrower entry point than Factor itself uses, and inheriting only
some of the surrounding infrastructure.**

| Stock Factor does                        | Our embedded path was missing |
|------------------------------------------|-------------------------------|
| c_to_factor_toplevel → SEH unwind table  | direct fnptr call, no SEH      |
| listener-step loops in one process       | one nf_eval_string per request, alien-callback churn each time |
| `:` definitions persist via dictionary   | also fine                      |
| `set-datastack` in listener (cooperative)| set-datastack across alien-callback boundary (broken)   |
| `with-file-vocabs` adds vocabs           | also fine                      |
| `print-error` is rich                    | our `nf-format-error` was sparse and could itself throw |
| `S" foo"` produces a pinned byte-array,  | we tried to extract `alien-address` from it (refused)  |
|   used as a c-ptr argument               |                                |

Each of these manifested as a separate-feeling bug but they're
all "we built a path-narrower-than-Factor-expects".  The fix in
every case was either to mirror what Factor does for itself
(listener loop, c_to_factor_toplevel, recover near with-datastack)
or to expose ONE missing piece (FFI symbol, SEH table, FFI param
type).

The codebase is now structurally closer to "Factor with an ANS
Forth front-end" than to "embedded interpreter with bolted-on
extras".

## Tests

```
session_smoke              5/5    ok
session_io                 5/5    ok
session_floats             3/3    ok
session_quickwins          7/7    ok
session_ans_booleans      19/19   ok
session_ans_core          19/19   ok
session_managed_strings   30/30   ok
session_ans_errors         3/3    ok
session_tick_execute       3/5    (2 pre-existing failures)
session_file_access        3/3    ok
session_test_runner        6/6    ok
session_let_lang          14/14   ok
session_crash_recovery     2/2    ok
session_repl_context       4/4    ok
session_persistence       13/13   ok
session_stack_survives    10/10   ok
diag_gui_output            2/2    ok    (new — GUI-mode output path)
diag_interrupt             1/1    ok    (new — runaway-loop interrupt)
─────────────────────────────────────
Grand total              149/151
```

## Files touched

Rust:
- `Cargo.toml` — rename bin `newfactor-ui` → `factorforth-ui`,
  add `embed-resource` build-dep
- `build.rs` — add `factorforth-ui` to bins_with_session, embed
  the manifest, add 8 graphics-FFI + 3 stack-publish + 2
  command-queue symbol exports
- `tools/factorforth-ui.exe.manifest`, `tools/factorforth-ui.rc` — new
- `src/bin/newfactor_ui.rs` — rename strings, retitle frame,
  add output-sink routing, gate GUI subsystem on release builds
- `src/session.rs` — listener-architecture dispatcher,
  listener-pending mutex/cv, listener-done mutex/cv, 5 new FFI
  exports (next_command, command_done, stack_begin/item/end),
  output-flusher callback, trace-logging to factorforth.log,
  extern "C-unwind" on all C-ABI fns and types, resolve_default_paths
  for release vs dev layout
- `src/compiler/resolve.rs` — 18 builtin entries (gpane-* / ev-*),
  walk into computed-CONSTANT bodies
- `src/compiler/parse.rs` — accept multi-token CONSTANT/FCONSTANT
  values, emit ConstValue::Computed
- `src/compiler/emit.rs` — emit Computed as `: name … ; inline`
- `src/compiler/ast.rs` — ConstValue::Computed variant
- `src/compiler/dump.rs` — pretty-print Computed
- `src/compiler/sema.rs` — walk into computed-CONSTANT bodies

Factor:
- `factor/forth/runtime/runtime.factor` — nf-listener-loop, eval,
  publish-datastack; SYMBOL nf-saved-datastack (kept for posterity,
  unused by listener); detailed nf-format-kernel-error; FFI decls
  for the new exports
- `factor/forth/wf64-gfx/wf64-gfx.factor` — `void*` for
  rt_gpane_open's title_addr; ba>> instead of nf-addr-raw

VM:
- `vm-build/vm/os-windows-x86.64.cpp` — split c_to_factor_toplevel
  into install_seh_table / uninstall_seh_table helpers
- `vm-build/vm/factor.cpp` — wrap factor_eval_string callback in
  install/uninstall pair
- `vm-build/vm/vm.hpp` — declare the new helpers

Release:
- `release/factorforth/` — fresh exe + DLL + image + 12 docs + 7
  demos + doc-crate.exe.  Now reproducible: edit code → cargo
  build --release → cp.

## What's not done

The list shortened today.  Still open:

- **#46 Core stragglers** (`exit`, `pick`, `roll`, `.s`, `count`,
  `u<`, `u>`, `move`) — small, mechanical resolver-table work plus
  thin runtime wrappers
- **#45 ?DUP polymorphic effect** — Factor's strict inference
  rejects ANS-style `?DUP`; needs an emit-time inline rewrite
- **#12 pure-Forth fractal-iter** — replaces WF64's MASM
  primitive so the Mandelbrot demo lights up
- **#49 DEFER / IS** — Programming-Tools word set
- **#50 LET WHERE clauses** — topological sort + forward refs
- **A manual Stop button (Ctrl+Break) in the IDE** — interrupt
  works on 20s timeout but no instant-from-keystroke yet.  The
  underlying `session.interrupt()` is public; just needs an
  accelerator and event routing.

## Today in one sentence

A Forth REPL that paints pixels, persists stack across evals,
shows a live stack monitor, routes output to the pane that
asked, recovers from every category of error (underflow,
overflow, divide-by-zero, type-check, FFI, FP-trap, interrupt),
and survives runaway loops — all from a single 1.5 MB exe that
launches with no console flash and embeds Factor's tagged-pointer
VM as its back end.
