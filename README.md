# NewFactor

A Rust ANS Forth compiler targeting Factor's VM as its back-end JIT.

The user writes ANS Forth.  The user never sees Factor — not in source, not
in error messages, not in tooling.  Internally, the Rust compiler does the
entire ANS Forth front end (lex, parse, parsing words, stack-effect
inference, dictionary, error model) and emits canonical machine-generated
Factor source as an internal IR; Factor's optimising compiler and JIT then
execute it.

## Why?

Factor's VM is, uniquely, a JIT that takes a concatenative stack-based front
end and lowers it through a register-allocated CFG, including polymorphic
inline caches and a generational GC.  It's the most sophisticated stack-
language compiler ever built — and almost nobody outside the Factor language
itself uses it.

ANS Forth, on the other hand, has thousands of users, decades of code, and
implementations that are mostly stuck at the technology level of 1990 (token
threading, direct threading, occasional hand-rolled native compilation).
The gap between what ANS Forth deserves and what ANS Forth gets is wide.

NewFactor brokers an introduction.

## What's here

| File / dir | Purpose |
|---|---|
| `MANIFESTO.md` | Architectural intent.  *Why* this project exists. |
| `PLAN.md` | Phase-by-phase delivery schedule.  *What* and *how*. |
| `Cargo.toml` | Independent Rust project; depends on `wf64` (path) for iGui only. |
| `src/` | Rust source — compiler, embedded VM session, host runtime callbacks. |
| `factor/` | Factor-side vocab sources (`forth.runtime`, `forth.wf64-gfx`).  Two small files. |
| `vm-build/` | Patched `factor.dll` build tree with the `nf_*` embedding API exports. |
| `images/` | The Factor `.image` artifacts (slim, runtime-loaded). |
| `demos/` | ANS Forth source files — mirror of WF64's `demos/`. |
| `tests/` | Rust integration tests; Hayes-style ANS test fixtures. |
| `docs/` | Design docs, dead-end records, hand-traces, benchmarks. |
| `scripts/` | Image-rebuild and DLL-rebuild scripts. |

## The headline demonstration

`demos/gfx-mandelbrot.f` — a Mandelbrot fractal renderer in ANS Forth.  The
same source file runs under two implementations:

- **WF64** uses hand-written MASM (`kernel/igui_gfx.masm`) for its
  `fractal-iter` primitive — ~150 lines of assembly carefully managing XMM
  registers.
- **NewFactor** defines `: fractal-iter ( z0x z0y cx cy maxiter -- n ) ... ;`
  in pure Forth — ~30 lines, no asm — and lets Factor's optimising compiler
  produce the equivalent machine code.

Same iGui (Direct2D), same algorithm, two JITs.  The bet: Factor's compiler
matches or beats the hand-tuned MASM, demonstrating that Forth doesn't need
assembly to get serious-language performance.

See `PLAN.md` Phase 5 for the benchmarking methodology.

## Status

Currently in **Phase 0**: workspace setup.  See `PLAN.md` for the phase
schedule and success criteria.

## Build

Windows only for v1 (iGui is Direct2D-based).

```pwsh
# Build the patched factor.dll (once):
cd vm-build
.\build.bat

# Build NewFactor:
cd ..
cargo build

# Run the CLI:
cargo run --bin newfactor

# Run the GUI:
cargo run --bin newfactor-ui
```

## License

BSD-3-Clause, matching WF64 and Factor.

## Authors

- **Alban Read** &lt;albanread@googlemail.com&gt; — architect and primary
  author.  Author of WF64 (the sibling STC Forth project NewFactor
  borrows the iGui from), and of the long-running engineering thread
  that culminated in this design.
- **Claude** (Anthropic, Sonnet/Opus class) — pair-programming
  collaborator.  Contributed VM archaeology, debugging across the six
  layered rot bugs that surfaced during the embedded VM bring-up
  (documented in `docs/dead-ends.md`), and most of the prose in this
  repository's design documents.

## Acknowledgements

- Slava Pestov, Daniel Ehrenberg, Joe Groff — Factor's authors.  The VM is
  the engine NewFactor reuses; their *"Factor: a dynamic stack-based
  programming language"* (DLS 2010) is the design document we work against.
- The WF64 project — iGui module reused via path dependency.
- The ANS Forth committee — for a language worth modernising.
