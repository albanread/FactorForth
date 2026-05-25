# M#34 — newfactor-ui shipped (2026-05-24)

The IDE binary.  ANS Forth in, results out, in a real Windows MDI
window.  Direct2D rendering, REPL pane, console pane, stack view,
three-level crash recovery, restart button.

## The path

Two phases that ended up combined.

### Phase A — restartable Session (Rust-side)

Replaced the per-eval watchdog's `std::process::abort()` with a
`recv_timeout` + `DeathCause` flag.  Worker panics are caught via
`std::panic::catch_unwind` and translated to `WorkerPanicked`.
Channel disconnect becomes `WorkerGone`.  Timeout becomes
`Timeout`.  Each carries the source that was running at the time.

`Session::is_dead()` and `Session::death_cause()` let the host
poll.  `Session::drop` detects the stuck-worker case (Timeout)
and detaches the thread rather than blocking on join.

The architectural promise — "language thread crashes, IDE
survives, dictionary stays intact" — holds for:
- Rust panics in our extern callbacks
- Worker disconnect
- Clean `Session::drop` + `Session::new()` cycles

Doesn't yet hold for:
- Hardware traps (DBZ, AV) inside Factor — these still kill the
  process via SEH that we don't catch.  Filed #48 already.
- Genuinely hung Factor loops — the FFI is wired
  (`nf_enqueue_interrupt`, `Session::interrupt()`) but Factor's
  safepoint mechanism requires SEH function tables that
  `nf_eval_string` doesn't install (only `c_to_factor_toplevel`
  does).  Filed #51 — natural to do alongside the GUI's "Stop"
  button.

### Phase C — IDE binary

WF64 already has `src/bin/newfactor_ui.rs` — a complete IDE built
on `wf64::igui` (Direct2D MDI, crash handler, REPL, console, stack
view) wired to WF64's subprocess-based `FactorSession`.  The
structure was right; we just needed to swap the backend.

Our binary mirrors WF64's structure exactly:

```text
newfactor-ui.exe
├── GUI thread        Direct2D MDI, Win32 message pump (wf64::igui)
│     ↕ IGuiEvent MPSC channel
├── IDE worker        receives events, drives Session
│     ↕ Command/EvalResult channels
└── Session worker    owns Factor VM (newfactor::session::Session)
      eval-callback → nf_rt_write_char → IoMode::Gui callback → fconsole
```

Three-level supervisor:
1. **SEH crash** → `crash_handler::take_dump` → respawn IDE worker
2. **Rust panic** → `catch_unwind` → `report_panic` → reboot session
3. **Session death** → `drop` + `Session::new()` → keep going
   (Factor VM persists across worker restarts, so any previously-
   defined user words remain in the image's dictionary)

`IGuiEvent::EvalBuffer` and `IGuiEvent::ReplSubmit` both go through
`newfactor::compiler::compile` → `session.eval`.  Output flows
through `IoMode::Gui`'s callback which buffers per-byte into
lines and calls `wf64::igui::fconsole::append`.

## The two-symbol collision

The first build attempt failed with `LNK2005`:

```
libwf64.rlib: rt_read_line already defined in libnewfactor.rlib
```

Both crates define `rt_read_char`, `rt_write_char`, `rt_read_line`
with different signatures (WF64's are `u64`-typed for its JIT
Forth host calling convention; ours are `*mut u8 / i64` for our
nf-host FFI library).

**Fix**: renamed our exports to `nf_rt_*`.  Three places need to
agree:
- `src/session.rs` — `#[no_mangle] pub extern "C" fn nf_rt_*`
- `build.rs` — `/EXPORT:nf_rt_*` linker args
- `factor/forth/runtime/runtime.factor` — `FUNCTION:
  nf_rt_write_char (...)` etc.

After rebuilding the image with the renamed FUNCTION: declarations,
everything works.

## Why we can be in-process when WF64 chose subprocess

WF64's `factor_session.rs` documents three reasons they chose
to spawn `factor.com` as a subprocess rather than embed in-process:

1. `factor_vm::init_ffi()` calls `GetModuleHandle(NULL)` which
   returns the **host EXE**'s HMODULE, not factor.dll's →
   primitive lookups fail.
2. Factor's C++ runtime captures CRT `stdin`/`stdout` FILE*
   directly; in a GUI subsystem these are `_fileno == -2` and
   Factor falls back to `fopen("nul", …)`.
3. The stock `factor.image` launches `ui.tools` (Factor's own
   IDE) when no `-run=` is supplied.

For us, all three are solved:
1. Our patched factor.dll has the `hHostExe` fallback in
   `ffi_dlsym` (the patch from session #1 of this project).
2. We don't use Factor's CRT stdio — host streams via
   `nf_rt_write_char` bypass it entirely.
3. Our `nf-mandelbrot.image` doesn't launch ui.tools; the
   startup-quotation is just `init-remote-control`.

So in-process embedding works for us where it didn't for WF64.
That's a meaningful performance win — no IPC, no pipe buffering,
no subprocess teardown on restart.

## What to do with it

```
E:\NewFactor\target\release\newfactor-ui.exe
```

Run it.  An MDI frame opens, the console appears auto-popped via
the `Ctrl+Shift+R` shortcut programmed at startup, and the
"NewFactor session ready" banner prints once Factor finishes
loading the image.

Then type ANS Forth at the prompt:

```
> 42 .
42
> : square dup * ;  5 square .
25
> LET (r) -> (a) = pi * r * r END
> 2.0 LET (r) -> (a) = pi * r * r END .
12.566370614359172
```

That's NewFactor compiling ANS Forth → Factor IR → Factor VM →
output → IDE pane.  The full pipeline running in a single
process, with crash recovery, with LET working inside the REPL.

## Tests

The IDE itself is interactive-only — no automated lock-in tests
for the GUI surface (that's WF64's iGui territory and well-tested
on their side).  What we DO test: Phase A's restartable Session
in `tests/session_crash_recovery.rs`:

- `healthy_session_starts_alive_and_evaluates` — happy path
- `session_can_be_recreated_after_clean_shutdown` — restart works

The interrupt / FEP tests are deferred to #51 along with the
SEH-table patch they need.

## Files touched

- `src/session.rs` — restart machinery, Interrupter (inert),
  rename rt_* → nf_rt_*, catch_unwind around worker, recv_timeout
  replaces process::abort
- `src/bin/newfactor_ui.rs` — full IDE binary based on WF64's
  newfactor_ui.rs structure, swapped to our in-process Session
- `build.rs` — /EXPORT renamed for nf_rt_*
- `factor/forth/runtime/runtime.factor` — FUNCTION: nf_rt_*
- `vm-build/vm/factor.cpp` — `nf_enqueue_interrupt` export
  (wired, awaiting #51 to be active)
- `vm-build/factor.dll` — rebuilt with the new export
- `images/nf-mandelbrot.image` — rebuilt with renamed FUNCTION:
- `tests/session_crash_recovery.rs` — 2 lock-in tests
- 1 docs/journal entry (this one)

## What's next

The IDE is real.  Natural next moves:

- **#10 Mandelbrot side-by-side** — write the kernel in LET,
  render in a pane, compare against WF64 on a frame-time
  stopwatch.  v1 milestone.
- **#51 enable nf_enqueue_interrupt** — VM-side SEH patch.
  Natural alongside a "Stop" button in the IDE.
- **#46 ANS Core stragglers** — each unlocks more corpus tests.
- **#37 graphics command queue** — once Mandelbrot exists,
  the shared-float-buffer protocol becomes the obvious next
  architectural piece for vertex / pixel streams.

The user can type Forth into a real IDE today and watch it run.
That's the milestone.
