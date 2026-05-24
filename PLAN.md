# NewFactor — Plan (2026-05-24)

> Companion to `MANIFESTO.md` (the *why*) and `docs/dead-ends.md` (what we learned not to do).  This document is the *what* and *how*: the project structure, the phases, the success criteria.

---

## Mission, in one paragraph

A Rust ANS Forth compiler that targets Factor's VM as its back-end JIT.  The user writes ANS Forth; the user never sees Factor — not in source, not in error messages, not in tooling.  Rust does the entire ANS Forth front end (lex, parse, parsing words, stack-effect inference, dictionary, error model); the output is canonical machine-generated Factor source as an internal IR; Factor's optimising compiler and JIT execute it.  The headline demonstration is the **Mandelbrot side-by-side**: the same demo source (`demos/gfx-mandelbrot.f`) runs under both WF64 (hand-tuned MASM `fractal-iter`) and NewFactor (`: fractal-iter ... ;` in plain Forth, compiled by Factor's optimiser).  Same iGui, same algorithm, two JITs — and the bet is that Factor's compiler matches or beats the hand-written assembly without anyone writing assembly.

---

## Architecture

### The two layers

```
┌──────────────────────────────────────────────────────────────────┐
│  NewFactor host (Rust)                                           │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  ANS Forth compiler                                        │  │
│  │    lex → parse → resolve → stack-check → emit              │  │
│  │  Output: canonical Factor source as IR (internal only)     │  │
│  └────────────────────────────────────────────────────────────┘  │
│                              │                                   │
│                              │  factor_eval_string               │
│                              ▼                                   │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  Embedded Factor VM session                                │  │
│  │    factor.dll (patched, ~700 KB) + nf-mandelbrot.image     │  │
│  │    Image contains: forth.runtime + forth.wf64-gfx          │  │
│  └────────────────────────────────────────────────────────────┘  │
│                              ▲                                   │
│                              │  alien.libraries: GetProcAddress  │
│                              │                                   │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  Host runtime (Rust extern "C" exports)                    │  │
│  │    rt_gpane_open, rt_gpane_fill_rect, …                    │  │
│  └────────────────────────────────────────────────────────────┘  │
│                              │                                   │
│                              ▼                                   │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  iGui (from wf64::igui via path dep — no fork)             │  │
│  │    Direct2D + DirectWrite, MDI, batched render             │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### Two Factor-side vocabs (small, by design)

- **`forth.runtime`** — ANS Forth runtime words Factor doesn't directly express: cell-addressed memory model (`@`, `!`, `+!`, `c@`, `c!`, `cells`, `cell+`, `nf-addr` tuple), the `do`/`loop`/`i`/`j` machinery (loop-frame stack via special-object slot 82's neighbour), the return stack (`>r` `r>` `r@` `rdrop`, reusing yesterday's `forth.fstack` trick), ANS-style `.` with trailing space, ANS-floored `mod`, ANS booleans (`true` = -1).  Target: 200–400 lines, one file.

- **`forth.wf64-gfx`** — `FUNCTION:` declarations for the host's iGui callbacks: `rt_gpane_open`, `rt_gpane_begin`, `rt_gpane_clear`, `rt_gpane_fill_rect`, `rt_gpane_present`, `rt_gpane_next_event_for`, plus ANS-style aliases.  Modeled on WF64's `src/lib.rs:170-196` table; same names, same semantics.  Target: ~100 lines.

### What lives where, summary

| Concern | Lives in |
|---|---|
| Parsing words (`:` `;` `IF` `DO` `CREATE` …) | Rust compiler (`src/compiler/parse.rs`) |
| Stack-effect inference | Rust compiler (`src/compiler/stack_check.rs`) |
| Word resolution + dictionary | Rust compiler (`src/compiler/resolve.rs`) |
| Error rendering in ANS terms | Rust (`src/error.rs`) |
| Canonical Factor IR generation | Rust (`src/compiler/emit.rs`) |
| ANS runtime semantics (`@`, `!`, `do`/`loop`, …) | Factor (`forth.runtime`) |
| Host callbacks (gpane, events) | Rust `extern "C"` (`src/runtime/`) + Factor `FUNCTION:` (`forth.wf64-gfx`) |
| User-defined Forth words | Rust compiler emits as Factor word definitions |
| GUI rendering | wf64's iGui module (path dep) |
| Embedded VM session lifecycle | Rust (`src/session.rs`) |

---

## Project layout (proposed)

NewFactor as its own Rust project at `E:\NewFactor\`.  Path dependency on the `wf64` lib for iGui only.  WF64 is not modified.

```
E:\NewFactor\
├── Cargo.toml                  # New: independent Rust project.
│                               # [dependencies] wf64 = { path = "../WF64" }
│                               # plus windows-rs (for FFI to factor.dll)
├── README.md                   # New: short orientation
├── MANIFESTO.md                # Existing, the architectural intent
├── PLAN.md                     # This file
│
├── src\                        # New: Rust source
│   ├── lib.rs                  # Library exports (compiler + session)
│   ├── main.rs                 # CLI: `newfactor demo.f` compiles + runs headless
│   ├── error.rs                # ANS-terms error type + reporter
│   ├── ffi.rs                  # Raw FFI declarations for nf_* exports of factor.dll
│   ├── session.rs              # Embedded Factor VM session (the smoke6 pattern, in Rust)
│   ├── compiler\               # The ANS Forth → canonical Factor IR pipeline
│   │   ├── mod.rs
│   │   ├── lex.rs              # Tokenizer; handles BASE, hex/oct/bin literals, comments
│   │   ├── ast.rs              # AST node types
│   │   ├── parse.rs            # Parsing words & control flow construction
│   │   ├── resolve.rs          # Symbol table; ANS-name → Factor-name resolution
│   │   ├── stack_check.rs      # Abstract interpretation for stack-effect inference
│   │   └── emit.rs             # Canonical Factor IR emitter
│   ├── runtime\                # Rust extern "C" exposed to embedded Factor
│   │   ├── mod.rs
│   │   └── gpane.rs            # rt_gpane_* — thin wrappers around wf64::igui calls
│   └── bin\
│       └── newfactor_ui.rs     # The UI binary; loads .f files, drives embedded VM,
│                               # owns the iGui window
│
├── factor\                     # Factor-side vocab sources (loaded into the image)
│   ├── runtime\
│   │   └── runtime.factor      # forth.runtime
│   └── wf64-gfx\
│       └── wf64-gfx.factor     # forth.wf64-gfx
│
├── vm-build\                   # Existing, the patched factor.dll build tree
│   ├── factor.dll              # ~700 KB, with the 18 nf_* embedding API exports
│   ├── factor.dll.lib
│   ├── Nmakefile               # /MD + minor build patches
│   ├── build.bat               # vcvars64 + nmake one-shot
│   └── vm\                     # ~150 vm/*.{cpp,hpp} files (Factor C++ source + our patches)
│
├── images\                     # New home for the .image artifacts
│   ├── boot.windows-x86.64.image    # 4.5 MB; the canonical boot image
│   ├── nf-slim-v1.image             # 77 MB; slim Factor with the optimising compiler
│   └── nf-mandelbrot.image          # 81 MB; slim + forth.runtime + forth.wf64-gfx
│                                    #         + alien.remote-control initialised
│
├── demos\                      # ANS Forth source — mirror of WF64's demos
│   ├── gcd.f
│   ├── factorial.f
│   ├── gfx-mandelbrot.f        # The headline demo
│   └── gfx-julia.f
│
├── tests\                      # Rust integration tests
│   ├── core_word_set.rs        # Hayes-style ANS test fixtures driven from Rust
│   └── fixtures\
│       └── *.f
│
├── docs\                       # Design docs and dead-end records
│   ├── architecture.md         # Long-form of this PLAN's architecture section
│   ├── dls10-synthesis.md      # The DLS '10 paper synthesis
│   ├── embedding-api-findings.md
│   ├── vm-layouts-reference.md # Quotation / word / array layouts
│   ├── dead-ends.md            # What we tried that didn't work
│   ├── traces\
│   │   ├── gcd-trace.md        # Hand-trace of GCD through Option B'
│   │   └── mandelbrot-trace.md # Hand-trace of the Mandelbrot demo
│   ├── benchmarks\
│   │   └── fractal-iter.md     # Phase 6 outcome
│   └── upstream\               # Mirror of Factor's handbook source + DLS '10 PDF
│
└── scripts\
    ├── build-vm.sh             # Wraps vm-build/build.bat
    └── build-image.sh          # Bootstraps a fresh slim image + applies forth.runtime
```

---

## Phases

Each phase has a single, falsifiable success criterion.  No phase ends until its criterion passes.

### Phase 0 — Workspace setup

Create the NewFactor Rust project; rearrange existing artifacts into the new tree.

**Tasks:**
- `cargo init --lib` in `E:\NewFactor\`
- Author `Cargo.toml` with `wf64 = { path = "../WF64" }` and `windows = { version = "0.62", features = [...] }`
- Move existing `vm-build/`, `docs/`, image files into new positions (`images/`)
- Drop the legacy 11-vocab `forth/` tree; archive it under `docs/upstream/legacy-forth-vocabs/` so the bug-fix work isn't lost
- Update `MANIFESTO.md` and `docs/dead-ends.md` references to new paths

**Success criterion:** `cargo build` in `E:\NewFactor\` compiles a placeholder lib that links against `wf64` and `factor.dll.lib`, with no errors.

**Estimate:** half a day.

---

### Phase 1 — Author `forth.runtime` and `forth.wf64-gfx`, build `nf-mandelbrot.image`

Distill the existing (now-archived) 11-vocab tree into two minimal Factor vocabs.

**`forth.runtime` contents** (per the Mandelbrot trace):

- Memory model: `nf-addr` tuple, `<variable>`, `<constant>`, `@`, `!`, `+!`, `c@`, `c!`, `cells`, `cell+`, `chars`, `char+`
- Loop frames: `loop-frames` symbol, `i`, `j`, `do-loop`, `?do-loop`, `+loop-loop`, `leave-throw`
- ANS-style I/O: `.` (with trailing space), `u.`, `cr`, `space`, `spaces`, `emit`, `type`
- ANS booleans: `true` (-1), `false` (0), `bool>flag`, `flag>bool`, conversions on comparisons
- ANS-floored `mod` (corrected from Factor's truncated semantics)
- Return stack: `>r`, `r>`, `r@`, `rdrop`, `2>r`, `2r>` via the existing `forth.fstack` slot-82 trick
- Number aliases: `s>d` (identity), `d>f` (→float), `f+`/`f-`/`f*`/`f/` (aliases to polymorphic `+ - * /`)
- Stack words: `?dup`, `2swap` if Factor's kernel lacks them
- Word execution: `execute` (call XT)

**`forth.wf64-gfx` contents** (mirror of WF64's `lib.rs:170-196`):

- `FUNCTION:` declarations for every `rt_gpane_*` in WF64's runtime
- ANS-name `ALIAS:`es (`gpane-open` → `rt_gpane_open`)
- Event constants: `ev-close`, `ev-frame-close`, etc.

**Build script:** `scripts/build-image.sh` boots `images/boot.windows-x86.64.image`, loads `forth.runtime` and `forth.wf64-gfx`, calls `init-remote-control` + sets `OBJ_STARTUP_QUOT` to `[ boot do-startup-hooks init-remote-control ]`, and saves as `images/nf-mandelbrot.image`.

**Success criterion:** A Rust unit test runs the headless smoke pattern (the `smoke6.c` equivalent in Rust): loads `nf-mandelbrot.image` via the embedded VM, evaluates `"42 forth.runtime:."` and gets `"42 "` back.  Memory model test: `"VARIABLE x 5 x ! x @"` evaluates and produces `5`.

**Estimate:** 1-2 days (much of the code exists in the archived vocabs; this is consolidation + bug-fix migration).

---

### Phase 2 — Rust ANS Forth compiler

The big phase.  Implemented in milestones, each shipping a working partial compiler.

**Milestone 2.1 — Tokeniser + number parser.**
Lex any ANS Forth source into a stream of typed tokens.  Handle BASE, hex/oct/bin prefixes, character literals (`[CHAR] x`), strings (`."`/`S"` parse-rest-of-line), comments (`\` and `(` ... `)`).
*Success:* tokenise `demos/gcd.f` and `demos/gfx-mandelbrot.f` losslessly; round-trip checks pass.

**Milestone 2.2 — Parse + AST for non-control-flow.**
Parse `:`/`;` definitions with stack-effect annotations; numeric and string literals; word references.  No control flow yet.
*Success:* `: square ( n -- n^2 ) dup * ; 5 square` parses to a correct AST.

**Milestone 2.3 — Word resolution + emit, simple cases.**
Resolve word names against the built-in ANS-to-Factor table (~150 entries — see `docs/word-table.md` to be authored).  Emit canonical Factor IR.  Submit via `factor_eval_string`.  Get results.
*Success:* `5 square` evaluated end-to-end returns `25`.

**Milestone 2.4 — Control flow: `IF`/`ELSE`/`THEN`, `BEGIN`/`UNTIL`/`WHILE`/`REPEAT`.**
Parser builds branch and loop AST nodes.  Emitter produces Factor combinator-form (`[ ... ] [ ... ] if`, `[ ... ] [ ... ] until`).
*Success:* `: abs ( n -- ) dup 0 < if negate then ; -5 abs` → `5`.

**Milestone 2.5 — `DO`/`LOOP`/`+LOOP`/`LEAVE` + `I`/`J`.**
Emit calls to `forth.runtime:do-loop` etc.  Translate `I` and `J` to runtime word calls.
*Success:* `: sum ( n -- s ) 0 swap 0 ?do i + loop ; 10 sum` → `45`.

**Milestone 2.6 — `CASE`/`OF`/`ENDOF`/`ENDCASE`.**
Parser builds case AST node; emitter produces Factor's `case` combinator with pairs.
*Success:* a small dispatch program returns the right branch.

**Milestone 2.7 — Stack-effect inference.**
Abstract interpretation over basic blocks; unification at IF/THEN merges; bounded depth for recursion.  Reject programs whose declared effect doesn't match inferred; produce ANS-style error messages.
*Success:* `: bad ( -- ) 1 2 ;` errors with "declared `( -- )` but produces 2 items".

**Milestone 2.8 — `VARIABLE`/`CONSTANT`/`FCONSTANT`.**
Compile-time defining words.  Rust evaluates constant expressions at parse time; emits Factor `CONSTANT:` for true constants and `: name <value> ; inline` for foldable computations.
*Success:* `64 constant maxiter  maxiter 2 *` → `128`.

**Milestone 2.9 — `CREATE`/`DOES>`.**
Trickier ANS construct.  Emit a Factor parsing-word equivalent that allocates data space at compile time and bound the runtime quotation.
*Success:* a counted-string-defining program works as expected.

**Milestone 2.10 — `EXIT` and ANS-faithful early-return.**
AST-level rewrite: splice rest-of-word into else-branch of containing IF.
*Success:* a word with `EXIT` inside an `IF` produces the same result as its hand-rewritten equivalent.

**Milestone 2.11 — Error model translation.**
Catch Factor errors at the `factor_eval_string` boundary; translate stack underflow, type errors, etc. into ANS-flavoured messages with source line numbers.
*Success:* `dup` on empty stack reports "stack underflow at line N, column M of demo.f"; no mention of Factor.

**Success criterion for Phase 2 (overall):** `demos/gcd.f` compiles and runs end-to-end through the pipeline, producing correct output, with no user-visible Factor.

**Estimate:** 1–2 weeks of focused work.

---

### Phase 3 — `newfactor-ui` binary with iGui

Build the UI binary that loads `.f` files and renders via wf64's iGui.

**Tasks:**
- `src/bin/newfactor_ui.rs`: process startup, embedded VM init (using `session.rs`), iGui window opening, REPL pane wiring
- `src/runtime/gpane.rs`: `extern "C"` functions matching the symbols `forth.wf64-gfx` declares; thin wrappers around `wf64::igui::*` calls
- File-load command in the UI: open a `.f` file, compile via `forthc-factor`, execute, render output to the console pane

**Success criterion:** Launch `newfactor-ui.exe`, open the console pane, click "Load demos/gcd.f", run it, see `6` printed.

**Estimate:** 2–3 days (mostly UI integration; the heavy lifting is done).

---

### Phase 4 — Pure-Forth `fractal-iter` + Mandelbrot demo

Author `demos/gfx-mandelbrot.f` (mirror WF64's verbatim) but with `: fractal-iter ... ;` defined in pure Forth instead of pulled from the host primitives.

**Tasks:**
- Write `: fractal-iter ( z0x z0y cx cy maxiter -- n ) ... ;` — ~30 lines of Forth
- Verify Rust compiler handles the FP-stack idiom correctly (might need tweaks to FP-vs-integer-stack distinction)
- Inspect the emitted Factor IR
- Run via the embedded VM, render via iGui

**Success criterion:** `newfactor-ui.exe` launched with `demos/gfx-mandelbrot.f` renders a visually-correct Mandelbrot set in the same colours as `wf64-ui.exe`'s render of the same file.

**Estimate:** 1–2 days.

---

### Phase 5 — Benchmark + dissection

Quantify the bet.

**Tasks:**
- Wall-clock frame-time measurement (render N frames at fixed parameters, average)
- Disassemble Factor's emitted code for `fractal-iter` (Factor has `disassemble.` tooling that dumps the JIT'd machine code)
- Compare to the MASM in `WF64/kernel/igui_gfx.masm`
- Note differences: register allocation, FMA usage, instruction scheduling
- If NewFactor is slower: identify the gap concretely, file as `docs/benchmarks/fractal-iter-gap.md`
- If NewFactor is faster: write `docs/benchmarks/fractal-iter-win.md` with the disassembly diff
- Try SIMD: rewrite `fractal-iter` using Factor's `math.vectors.simd` to process 2 or 4 pixels per iteration; re-benchmark

**Success criterion:** A signed, dated benchmark doc with timings and disassembly listings.  The *outcome* is whichever way the comparison falls; the *success* is the rigorous comparison itself.

**Estimate:** 2-3 days.

---

### Phase 6 — Polish, write up, share

- Tidy the Rust crate's public API
- Author a real README that orient newcomers
- Cross-publish to the Factor community channels (mailing list, Discord) — there are people there who would be delighted to learn their VM has a non-Factor user
- Write a short technical post on the side-by-side approach and the result (this is the publishable artifact)
- Open the upstream issue #2658 PR (the `init_factor_from_args` revival + the `init-remote-control` startup-quotation fix) — this is the give-back to the Factor project

**Success criterion:** Someone other than us can clone the repo, follow the README, and run the side-by-side comparison on their machine.

**Estimate:** 2-3 days.

---

## Risks and what we'll do about them

| Risk | Likelihood | Mitigation |
|---|---|---|
| Factor's emitted code for `fractal-iter` is significantly slower than the MASM | Medium | Drop into Factor's SIMD vocab; annotate with `typed`/`inline`; reshape the loop to match what the optimiser recognises.  If still slower, write up *why* — that's its own contribution. |
| Some ANS Forth construct doesn't translate cleanly to Factor IR | Low-medium | We hand-trace each significant construct before writing the emitter for it.  GCD and Mandelbrot already validate most of them. |
| `forth.runtime` grows past 400 lines and becomes its own maintenance burden | Medium | Re-evaluate per word: can the Rust resolver handle it via direct rename instead?  Most of `forth.runtime` is "Factor doesn't have this exact semantic"; cheap pieces go away.  Budget alarm at 600 lines. |
| iGui FFI bindings change as wf64 evolves | Low | Pin the wf64 path-dep to a specific commit; bump deliberately.  Path-dep means we see the change at our next `cargo build`. |
| Embedded VM crashes mid-session (the kind of issue we crushed yesterday but more might exist) | Medium | We have the diagnostic playbook from yesterday: instrument with `eprintln!`, examine call stacks, check for missing initialisation.  The patched factor.dll is now well-understood. |
| Stack-effect inference is harder than it looks for ANS Forth's looser conventions | Medium | Fall back to "trust the declared effect; check at runtime in debug builds" mode.  Strict inference is a v2 feature; v1 accepts what the user declares. |
| Scope creep into "fix Factor's documentation" or "improve Factor's compiler" | High | Explicitly out of scope.  Issue #2658 PR is the *only* upstream contribution committed; everything else is "use, don't fix." |

---

## Out of scope (explicit)

- **Microcomputer-era memory poking** (raw `@` on arbitrary integer addresses, hardware register access, `BLK`/`BLOCK`/`LOAD`).  Programs needing this should use the FFI to access real hardware; pure ANS memory ops go through our tuple-based addresses.
- **Factor source as a user-facing language.**  Users write ANS Forth.  Period.
- **Modifying WF64.**  Path dep on its lib; no edits to its source.
- **The legacy 11-vocab `forth/*` tree.**  Archived for reference; replaced by `forth.runtime` + `forth.wf64-gfx`.
- **Cross-platform support.**  v1 is Windows-only (where iGui lives).  Linux/macOS are theoretically possible once iGui is portable, but not in our scope.
- **A Factor-side REPL exposing Forth.**  The `newfactor-ui` REPL is *Forth*; behind it Factor exists, but the user's experience is "I'm in a Forth REPL with a graphics pane."
- **Beating LuaJIT or hand-tuned C.**  We're beating *hand-tuned MASM written for a Forth kernel* — the specific comparison that's meaningful to the Forth community.
- **ANS Forth conformance certification.**  The Hayes test suite is a v2 goal.

---

## Decision log

Decisions that shaped this plan, with one-line rationales:

- **Option B′ over A/B/C** (architecture).  Rust does the full ANS front-end; Factor source as internal IR.  See MANIFESTO point 2 (revised 2026-05-24).
- **Factor IR over direct VM cell emission** (initial choice; future optimisation possible).  Factor's parser is fast and correct; reimplementing it in Rust to bypass it gains nothing real for v1.
- **Path dep on wf64 over forking iGui or extracting it to a shared crate.**  Preserves WF64's independence; no two-project refactor required; honours the user's stated preference.
- **Two Factor vocabs, not eleven.**  `forth.runtime` (ANS runtime) and `forth.wf64-gfx` (host FFI).  The legacy 11-vocab tree was over-decomposed for a compile-by-aliasing model that we no longer use.
- **Mandelbrot side-by-side as v1 milestone.**  Single-bet demo with a falsifiable claim; ties together everything we're building.
- **`fractal-iter` written in pure Forth, not as a primitive.**  The whole point of the demo.
- **Slim image carries `forth.runtime` + `forth.wf64-gfx` pre-loaded.**  Eliminates per-session vocab loading; users hit a ready VM.
- **Embedded VM via FFI (not subprocess, not unmodified factor.dll, not patched-in-place).**  See dead-ends.md.

---

## What needs your call before we proceed

1. **Confirm the project layout above.**  Particularly: NewFactor as independent Rust project with `wf64` path dep; legacy `forth/*` archived rather than deleted; `vm-build/` stays put.
2. **Confirm the Phase 0 archive step.**  Move existing `E:\NewFactor\forth\*` to `E:\NewFactor\docs\upstream\legacy-forth-vocabs\`, where the bug-fix work today (in `forth.numeric`) is preserved as a reference but not loaded.
3. **Confirm what happens to the existing `nf-embedded-v2.image`, `nf-full-master.image`, `nf-slim-v1.image`, `nf-embedded.image`, `nf-noop.image`, `nf-ready.image` artifacts.**  Keep all under `images/` for historical reference, or prune to just `nf-slim-v1.image` (the canonical one) plus the eventual `nf-mandelbrot.image`?
4. **Confirm WF64 is read-only from our side.**  No PRs into WF64, no edits to its iGui module, no shared changes.  Only consume.

Once those four are confirmed, Phase 0 starts.
