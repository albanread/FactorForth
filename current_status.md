# NewFactor — Current Status

**Snapshot date:** 2026-05-24, end of day.

A working ANS Forth implementation with infix-algebra DSL, managed
strings, a Direct2D MDI IDE, and a Forth-2012-conformance test
runner — all in one Windows process, ~1.5 MB IDE binary.

---

## What you can do today

### Launch the IDE

```
E:\NewFactor\target\release\newfactor-ui.exe
```

A Direct2D MDI window opens.  The console pane auto-pops at
startup.  Banner: `∿ NewFactor IDE` → "ANS Forth front-end on
Factor's VM (in-process)".  REPL prompt waits.

### Type Forth at it

```forth
> 42 .
42
> : square dup * ;
> 5 square .
25
> : add1 1 + ; : doubled dup + ;
> 6 square add1 doubled .
74
> LET (r) -> (a) = pi * r * r END
> 2.0 LET (r) -> (a) = pi * r * r END .
12.566370614359172
> S$" Hello, " S$" World!" $+ $.
Hello, World!
```

### Run the conformance corpus

```
cargo test --test session_test_runner -- --ignored --test-threads=1
```

61 canonical Forth-2012 `T{ <code> -> <expected> }T` assertions.
All pass.

### Run the full test suite

```
cargo test -- --ignored --test-threads=1
```

Suite-by-suite tally (each runs cleanly individually; some
back-to-back combinations hit Session singleton fragility — see
#31):

| Suite | Tests | Status |
|---|---|---|
| session_smoke | 5 | ok |
| session_io | 5 | ok |
| session_floats | 3 | ok |
| session_quickwins | 7 | ok |
| session_ans_booleans | 19 | ok |
| session_ans_core | 19 | ok |
| session_managed_strings | 30 | ok |
| session_ans_errors | 3 | ok |
| session_tick_execute | 5 | ok |
| session_file_access | 3 | ok |
| session_test_runner | 6 | ok |
| session_let_lang | 14 | ok |
| session_crash_recovery | 2 | ok |
| session_repl_context | 4 | ok |
| ans-core-corpus | 61 | ok (run by session_test_runner) |
| **Session-based + corpus total** | **186** | **ok** |
| smoke_runtime (legacy direct-VM) | 40 | ok (run separately) |
| **Grand total** | **226** | **ok** |

---

## Architecture

```text
newfactor-ui.exe  (one Windows process, ~1.5 MB)
├── GUI thread        Direct2D MDI + DirectWrite (wf64::igui)
│     ↕ IGuiEvent MPSC channel
├── IDE worker        receives events, holds CompileContext
│     ↕ Command / EvalResult channels
└── Session worker    owns Factor VM (newfactor::session::Session)
                      eval-callback → nf_rt_write_char →
                      IoMode::Gui callback → fconsole

Compile pipeline:
  ANS Forth source
  → lex (tokens)
  → parse (AST + LET-block + tick + DOES> templates)
  → resolve_with_prior (names → Factor targets + prior session ctx)
  → infer_with_prior (effect synth + prior effects)
  → emit (Factor IR string)
  → nf_eval_string (patched factor.dll)
  → Factor VM execution
  → host streams (rt_*) → IDE console pane
```

### Key substrate decisions

- **In-process Factor VM** via patched factor.dll.  The three
  issues WF64 cited for choosing subprocess (init_ffi GetModuleHandle
  returning host EXE; CRT stdio in GUI subsystem; ui.tools auto-launch)
  are all fixed in our patched VM and our nf-mandelbrot.image.
- **Single-thread VM, dedicated worker**.  Factor's VM is
  TLS-bound; we own it on one worker thread, communicate via
  channels.
- **Synth-is-authoritative stack effects**.  User declarations
  are documentation; body inference is truth.  Mismatch is a
  warning, not an error (matches Forth's permissive culture).
- **Persistent compile context**.  IDE worker holds a
  `CompileContext { user_words, user_effects }` for the session's
  life.  Definitions in eval N are visible in eval N+1.
- **Three-level crash recovery** in the IDE: SEH (via WF64's
  VEH crash handler) → Rust panic (catch_unwind) → Session death
  (drop + new).  Each level keeps the IDE alive.

---

## Language surface

### ANS Core (95%+ coverage)

| Family | Status |
|---|---|
| Stack ops | DUP DROP SWAP OVER ROT -ROT NIP TUCK 2DUP 2DROP 2SWAP 2OVER DEPTH |
| Return stack | >R R> R@ RDROP 2>R 2R> |
| Arithmetic | + - * / MOD NEGATE ABS MIN MAX 1+ 1- 2* 2/ /MOD */ */MOD (MOD is ANS-floored) |
| Comparison | = <> < > <= >= 0= 0< 0> 0<> U< U> — all return ANS -1/0 |
| Bitwise | AND OR XOR INVERT LSHIFT RSHIFT |
| Memory | @ ! C@ C! +! CELL+ CHAR+ CELLS CHARS FLOATS 2@ 2! MOVE ERASE |
| Control flow | IF ELSE THEN BEGIN UNTIL WHILE REPEAT AGAIN DO ?DO LOOP +LOOP LEAVE UNLOOP I J CASE OF ENDOF ENDCASE |
| Defining words | : ; VARIABLE CONSTANT FCONSTANT CREATE DOES> ARRAY FARRAY CBUFFER |
| Strings | S" ." C" TYPE CMOVE FILL BL COUNT |
| I/O | . CR EMIT SPACE SPACES KEY ACCEPT U. .S |
| Pictured numeric | <# # #S SIGN HOLD #> n>$ |
| Floats | F@ F! F+ F- F* F/ F< F> F= D>F F>D FCONSTANT |
| Execution | ' EXECUTE |
| Bases | HEX DECIMAL BINARY OCTAL |
| File access | INCLUDED (others deferred) |

### NewFactor extensions

| Feature | Surface |
|---|---|
| **LET DSL** | `LET (in1, in2) -> (out1, out2) = expr, expr WHERE name = expr END` — infix algebra, comparisons, `**`, `select()`, sqrt/abs/min/max/floor/ceil/round/trunc, sin/cos/tan/asin/acos/atan/atan2/exp/log/pow/hypot, pi, e. Factor's compiler does the unboxing. |
| **Managed strings** | `S$" lit"` literal + `$len $clen $+ $upper $lower $find $contains? $starts? $ends? $slice $cmp $hash $. $.cr int>$ $>int >$ $>addr` — backed by Factor's native string type, no PAD, no lifetime traps |
| **Defining-word templates** | `CREATE name N CELLS ALLOT DOES> + ;` → closure-construction |
| **Forth-level vectoring** | `'` (tick) + `EXECUTE` + VARIABLE for ttester.fr-style ERROR vectoring |
| **Live word definitions** | Each REPL eval's defs persist into subsequent evals (via `CompileContext`) |

### What's NOT shipped

| Feature | Status | Why |
|---|---|---|
| `?DUP` | Not exposed | Stack-effect-polymorphic; Factor's static inference can't model. Filed #45 — needs emit-time inline rewrite |
| `DEFER`/`IS` | Not implemented | `' + EXECUTE` covers the same use case for ttester.fr; full ANS Programming-Tools — #49 |
| `[CHAR]`/`CHAR` | Not implemented | Parser-level, awkward in our compile-then-eval model |
| `IMMEDIATE`/`POSTPONE`/`LITERAL` | Not implemented | Tied to immediate-word machinery; macros are an open design question |
| `EXIT`/`RECURSE` | Not implemented | Small adds, #46 |
| `SOURCE`/`>IN`/`WORD`/`EVALUATE` | Will not implement | Forth source-input introspection model; incompatible with our compile-then-eval pipeline |
| BLOCK | Will not ship | Pre-1980s 1K-block-based storage; modern Forths don't use this |
| LOCALS (ANS `{: :}`) | Not implemented | Could land on Factor's `::` natively; deferred |
| Search-order vocabularies | Not implemented | Factor has vocabs; ANS surface different; defer |
| Full File Access Word Set | Only INCLUDED | OPEN-FILE / READ-FILE / WRITE-FILE / CLOSE-FILE etc. — small wrappers around Factor's io.files when needed |
| Exception (CATCH/THROW/ABORT) | Not implemented | Filed #35 partial — error visibility shipped, full THROW codes still pending |

---

## Known limitations

### Things that work in one eval but not across evals

| Item | Bug | Filed |
|---|---|---|
| **Variables** | Defined in eval 1, refed in eval 2 — fails. Per-compile escape analysis hoists to Factor SYMBOL: which subsequent compiles don't know about. | **#52** |
| **CREATE'd buffers / ARRAY / FARRAY / CBUFFER** | Names persist, but the index→address machinery depends on per-compile sema state.  Probably broken cross-eval like variables. | (#52-adjacent — investigate) |

### Crashes / hangs

| Failure mode | Outcome | Filed |
|---|---|---|
| Rust panic in our extern callbacks | Caught by `catch_unwind`, surfaces as `DeathCause::WorkerPanicked`, session restarts | (works) |
| Factor `throw` (no-method, bounds, etc.) | Caught by Factor's eval-callback recover, error visible in fconsole, session continues | (works) |
| **Hardware SEH** (DBZ, AV, stack overflow) | Kills the process — Factor's safepoint guard page needs SEH function tables installed during `nf_eval_string`, currently only `c_to_factor_toplevel` installs them | **#48** |
| **Infinite Factor loop** | Watchdog times out after `eval_timeout` (default 20s).  Session marked `DeathCause::Timeout`.  `nf_enqueue_interrupt` FFI is wired but inert until #48 lands (FEP needs safepoint SEH too). | **#51** |
| **`.` followed by no `cr`** | Output sits in our line-buffer until next `\n`.  Visible as "delayed prompt".  Probable cause of the late-session crash the user observed. | (not filed — fix is "flush remaining buffer on eval completion") |

### Custom eval-callback `recover` issue

| Description | Filed |
|---|---|
| The bespoke `nf-eval-callback` + `nf-format-error` machinery in `forth.runtime` §13 is wired but not installed in session setup. Reason: putting `recover` as the outermost quotation of an alien-callback trips `combinators:wrong-values → kernel:die` on some error paths.  Until cracked, we use Factor's stock eval-callback (which has its own internal recover via `eval>string`) — diagnostics still visible via the error-stream binding. | **#47** |

### Test-suite-bleed fragility

| Description | Filed |
|---|---|
| When running multiple test suites back-to-back in fast succession, occasional crashes happen due to Session singleton state not fully cleaned up between cargo-test processes.  Each suite passes individually.  Annoying but documented. | **#31** |

---

## Architectural insights (worth keeping)

1. **The substrate bet paid off.**  LET ported from WF64 in one day
   (~1250 LoC including tests) because Factor's optimiser is the
   DSL's backend.  WF64 spent ~1900 LoC on hand-written LLVM-MC
   codegen we don't need.  Same story for managed strings: WF64
   built GC + tagged-ptr machinery; we use Factor's native string
   type and write thin wrappers.

2. **CompileContext is accidentally an isolate primitive.**  Each
   `compile_in_context(src, &mut ctx)` is parametric over its
   prior state.  Pass an empty ctx → fresh isolate.  Pass a
   shared ctx → continuity.  Combined with per-vocab Factor IN:
   declarations, this supports actor-model / sandbox semantics
   trivially.  Not a current goal but a future avenue worth
   noting.

3. **Synth-from-body is more believable than declared.**  The
   stack-effect inference treats user annotations as
   documentation, body synthesis as truth.  Mismatches are
   warnings.  This works because Factor's strict inference
   catches anything we get wrong at the IR level — we don't
   need to be the cop, Factor is.

4. **Three-level crash recovery**.  Each level catches a
   different failure class: VEH for hardware SEH, catch_unwind
   for Rust panics, Session::is_dead() for Factor-side errors.
   None of them can take down the IDE process by themselves.

5. **Treat test files as data.**  The Forth-2012 conformance
   corpus runs not by feeding `runtests.fth` to Factor (which
   would need SOURCE/>IN/?DUP we don't have), but by parsing
   `T{ ... -> ... }T` blocks in Rust and running each as two
   separate evals on a shared session.  Each assertion is fully
   isolated; the watchdog kills runaway tests cleanly.

---

## What's next

Priority order, with effort estimates and dependencies:

| # | Task | Effort | Why |
|---|------|--------|-----|
| #52 | Variables in compile_in_context — force wide form | 1 hr | Closes the obvious UX gap users hit in the REPL |
| (n/a) | Fix line-buffer flush on eval completion | 30 min | Fixes the prompt-display / probable-crash from `.`-without-`cr` |
| #51 | Enable nf_enqueue_interrupt via SEH-install around nf_eval_string | ~half day | Adds Stop-button capability for runaway loops |
| #48 | Hardware-trap recovery | ~half day if #51 lands | DBZ no longer kills the IDE |
| #10 | v1 milestone: Mandelbrot side-by-side vs WF64 | 1-2 days | The proof point we've been building toward; Mandelbrot kernel = LET form, render through iGui, compare frame times against WF64 |
| #37 | Graphics command-queue design | 2 days after #10 | Shared float-buffer protocol for vertex/audio streams |
| #46 | ANS Core stragglers (U< U> COUNT KEY? EXIT .S MOVE PICK ROLL) | half day | Each unlocks more conformance corpus tests |
| #45 | ?DUP via emit-time inline rewrite | half day | Common ANS word still missing |
| #49 | DEFER / IS proper | half day | ANS Programming-Tools conformance |
| #50 | LET WHERE topological sort | half day | Allows out-of-order WHERE clauses |
| #31 | smoke_runtime migration onto Session | half day | Removes test-suite-bleed fragility |
| #47 | Custom eval-callback recover | 1 day investigation | Unlocks bespoke ANS THROW codes; lower priority |

---

## Project layout

```
E:\NewFactor\
├── src/
│   ├── compiler/          ANS → Factor IR pipeline
│   │   ├── lex.rs         tokeniser (incl. LET-block capture)
│   │   ├── parse.rs       AST builder
│   │   ├── ast.rs         AST types
│   │   ├── resolve.rs     name → Factor target; user_words tracking
│   │   ├── effect.rs      stack-effect inference (synth + declared)
│   │   ├── sema.rs        whole-program model; orchestrator
│   │   ├── emit.rs        AST → Factor IR text
│   │   ├── dump.rs        pretty-print per-phase for diagnostics
│   │   ├── error.rs       Span + per-stage error types
│   │   └── let_lang/      LET DSL sub-language
│   │       ├── parser.rs  (ported from WF64 verbatim)
│   │       └── codegen.rs (new — lowers to Factor IR)
│   ├── session.rs         in-process Factor VM driver, Session,
│   │                      worker thread, IoMode, CompileContext callers
│   ├── lib.rs             pub exports
│   ├── main.rs            `newfactor` CLI (offline compile)
│   └── bin/
│       ├── newfactor_ui.rs  IDE binary (wired to wf64::igui)
│       └── embed_smoke.rs   legacy direct-VM smoke test
├── factor/
│   └── forth/
│       └── runtime/
│           └── runtime.factor   Factor-side ANS runtime vocab
├── vm-build/
│   ├── vm/                Factor VM source (with our patches)
│   │   └── factor.cpp     embedding API exports (nf_*)
│   ├── factor.dll         built artefact (rebuild via build.bat)
│   └── build.bat          nmake + vcvars64 → factor.dll
├── images/
│   └── nf-mandelbrot.image   baked Factor image (built by scripts/build-image.sh)
├── tests/                 (16 integration test files, see suite list above)
│   └── fixtures/
│       ├── ans-core-corpus.fs   61-assertion T{ }T conformance corpus
│       └── included-hello.fs    INCLUDED smoke fixture
├── docs/
│   ├── ans-gap-analysis.md
│   ├── wf64-extensions-feasibility.md
│   ├── embedding-api-findings.md
│   └── journal/           per-milestone diary (15 entries so far)
├── scripts/
│   └── build-image.sh     rebuilds nf-mandelbrot.image
├── current_status.md      this file
├── PLAN.md / MANIFESTO.md  high-level vision
└── Cargo.toml
```

---

## Build commands

| Command | What it does |
|---|---|
| `cargo build` | Library + `newfactor` CLI + `embed-smoke` + `newfactor-ui` |
| `cargo build --bin newfactor-ui --release` | Release IDE binary |
| `cargo test --lib --tests` | Non-ignored tests (fast, no VM) |
| `cargo test -- --ignored --test-threads=1` | Full integration suite (loads Factor VM) |
| `cargo test --test <suite> -- --ignored --test-threads=1` | One suite |
| `bash scripts/build-image.sh` | Rebuild nf-mandelbrot.image from runtime.factor + bootstrap |
| `cd vm-build && cmd /c build.bat` | Rebuild factor.dll from vm/ source |

When `factor/forth/runtime/runtime.factor` changes, re-run
`build-image.sh`.  When `vm-build/vm/*.cpp` changes, re-run
`build.bat`.  Rust changes are picked up by cargo build.

---

## How to find your way around

- **Latest journal entry**: `docs/journal/` (newest first by date)
- **Per-milestone history**: same place — each milestone has its
  own entry documenting decisions, bugs, and what shipped
- **Conformance details**: `docs/ans-gap-analysis.md`
- **WF64 extension feasibility notes**: `docs/wf64-extensions-feasibility.md`
- **Architecture deep-dive**: `MANIFESTO.md`, `PLAN.md`
- **Embedding API details**: `docs/embedding-api-findings.md`

---

## Open architectural questions

Captured here because they don't fit a single milestone but
shape later work:

1. **Variables-as-isolates**.  The fact that variables don't
   cross compile contexts is a bug today (#52) but maps cleanly
   to actor-model isolation in a hypothetical future.  Each
   `CompileContext` could be a sandbox with its own dictionary;
   each worker thread an isolate; messages between them via
   channels.  Factor's vocab system (one IN: per isolate) makes
   the namespace separation cheap.  Not a current goal.  Worth
   keeping the design space open as we fix #52.

2. **Hardware-trap recovery vs in-process embedding**.  WF64
   chose subprocess to bypass SEH integration complexity.  We
   chose in-process for performance.  #48 + #51 are the bills
   coming due — Factor's SEH function-table installation needs
   to happen around `nf_eval_string`, not just
   `c_to_factor_toplevel`.  Small VM patch, big payoff
   (interruptible loops + DBZ recovery + safepoint GC works as
   designed).

3. **Float-buffer protocol for graphics**.  The shared-buffer
   approach we sketched in #37 sidesteps Factor's per-call
   float boxing entirely.  Vertex streams, audio samples,
   physics state — all written into a Rust-owned `Vec<f64>` via
   `nf-addr+` from inside a LET form.  Composes well with #44
   LET; #10 Mandelbrot will be the proving ground.

4. **What's the "live" UX**.  Today the IDE is REPL + console +
   editor panes.  Future considerations: hot-reload of saved
   word libraries; pinned stack-watch widgets; LET expression
   playground that auto-recompiles as you type; integrated
   benchmark runner for `cargo bench`-style perf tracking.
   All on top of the WF64 iGui substrate we already use.
