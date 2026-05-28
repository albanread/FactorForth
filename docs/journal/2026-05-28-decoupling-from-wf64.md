# 2026-05-28 — cutting the cord from WF64

The day started innocently: land the `:before`/`:after` follow-ups,
maybe fix the multi-dispatch emit bug.  It ended with NewFactor
finally standing on its own — no WF64, no JASM, no NewGC in the build
graph.  The path there was a small comedy of realization.

## The build broke for reasons that weren't ours

I went to test the full-arity specializer fix (#70) and `cargo build`
fell over with a wall of errors in `E:\NewGC\crates\newgc-core` —
`no field poisoned`, `bytes_alloc_since_gc` is a method not a field.
A half-finished refactor in the **garbage collector of a different
project** had stopped *our* build.

My first instinct was the wrong one: I started characterizing the
NewGC breakage, as if it were mine to understand.  The user cut
through it:

> "no you have drifted we do not depend on WF64 at all WF64 uses masm
> we use factor, they use NewGC we use Factor, they use JASM we use
> Factor what do we use?"

The answer is **Factor**.  The embedded `factor.dll` is our entire
runtime substrate — VM, GC, compiler, FFI.  We have no business in
NewGC's or JASM's blast radius.  Yet `Cargo.toml` said
`wf64 = { path = "../WF64" }`, and depending on the whole `wf64` crate
to borrow one thing dragged its Forth *engine* — JASM (assembler) and
NewGC (collector) — along for the ride.

## What we actually borrowed

Grepping our own source: the only real coupling was `wf64::igui` —
WF64's Direct2D/Win32 MDI shell.  And `igui` itself references
*nothing* from WF64's runtime (the grep for `crate::runtime` / `jit`
/ `newgc` inside `src/igui/` came back empty).  It's a self-contained
window library that merely *lives* in the wrong crate.  Depending on
it forced cargo to compile all of `wf64`, engine included — pure
collateral.

## "this means we have been clobbering WF64"

Then the sharper realization, and it cut both ways.  The F7 checker,
the Factor REPL pane, the stack view, the console — all the
NewFactor-IDE-specific pieces — *live inside WF64's `src/igui/`*,
because the `newfactor-ui` binary was originally seeded in WF64's
tree (WF64's `Cargo.toml` still has a `newfactor-ui` bin).  Every IDE
change we'd made had been landing in **WF64's repo**.  The user's
reaction:

> "this means we have been clobbering WF64 with our changes ffs."

Exactly.  There was never a clean boundary between "the shared window
shell" and "the NewFactor IDE built on it," so our work leaked into a
sibling project — a MASM/JASM/NewGC Forth that has no use for a
Factor REPL pane.

## The cut: copy and fork

The decision was clean:

> "we copy and fork and WF64 is going to have fix all the damage we
> did to them."

Copy (not move) — leaving WF64's tree untouched, since it had its own
in-flight edits — `src/igui/`'s 28 files into `crates/igui`, a
standalone crate depending only on `windows` + `windows-numerics`.
The fork brought our additions *with* it (they were in WF64's igui all
along; copying lifts them out).  The single cross-crate reference —
the editor highlighting words from `wf64::PRIMITIVES` — became an
injectable `install_primitives` hook (the same pattern as
`install_checker`).

Then: drop `wf64` from `Cargo.toml`, add `igui = { path = crates/igui }`,
`wf64::igui::` → `igui::` across the source.  Build graph is now
**`newfactor + igui + windows + factor.dll`**.  A broken sibling GC
can never break us again.

## Re-homing the graphics

Cutting `wf64` lost the `rt_gpane_*` graphics FFI — those were
*defined* in `wf64::runtime`, and the `forth.wf64-gfx` Factor vocab
draws through them.  The linker named all nine as unresolved
externals (the `/EXPORT:` directives in `build.rs` had nothing to
point at).

So I re-homed them: `src/gfx.rs`, the nine `rt_gpane_*` functions
rewritten to call **our** `igui` crate (`batch`/`channels`/`window`).
The whole graphics path is ours now: Factor → `rt_gpane_*` → `igui` →
Direct2D.  Re-added the exports; `dumpbin` confirms all nine in the
exe's export table.

## Proof it works — including a false alarm

I launched the freshly-built IDE and screenshotted a **Mandelbrot
rendered in a Direct2D pane** — the new `src/gfx.rs` path drawing
through the forked shell.  Then a moment of doubt when the dev
instance exited after a few seconds; I worried the build was
crashing and chased it with Event Log queries.  The user knew better:

> "hey we know it works, release it to the release folder."

Right again — the fractal *had* drawn; the short-lived instance was
the throwaway debug exe and its auto-run demo window closing.  Built
the proper `--release` binary (2.07 MB), deployed it, verified the
exports, kept a `.pre-decouple` backup.

## The principle, stated for the record

From the SEE work earlier and reinforced here: **we use Factor; we
do not write Factor, and we do not depend on WF64's engine.**  We
*borrow* a window shell — a library to steal, now genuinely stolen
into our own crate — but borrowing the shell must never drag the
borrowed project's runtime along.  The boundary is finally where it
belongs.

## Stats

  - `crates/igui`: 28 files, standalone, deps = windows + windows-numerics
  - `src/gfx.rs`: 9 `rt_gpane_*` + helpers, on our igui
  - build graph: newfactor + igui + windows + factor.dll (was: + wf64 →
    JASM + newgc-core)
  - #70 full-arity specializer emit landed on the unblocked build,
    with a regression test for non-top-argument dispatch
  - commits: 3fd1945 (decouple), 59a7feb (graphics re-home)

## Reflection

Two course-corrections from the user, both the same shape: I'd
started *understanding* a sibling project's internals when the right
move was to stop depending on them at all.  "What do we use?" is a
question worth asking of every dependency.  The answer — Factor, and
one honestly-stolen window library — is now what the build graph
says, not just what the architecture diagram claims.

— end of decoupling entry
