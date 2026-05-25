# How To Test A Failing Demo

This repo already has the pieces needed to answer a very specific question:

"Which word in this `.f` file compiles, which word loads into the Factor VM, and which word only fails when Factor lazily compiles it on first call?"

The important distinction is that there are two different failure layers:

1. The Rust front end can fail while lexing, parsing, resolving, or building semantic state.
2. The generated Factor IR can load successfully, but a specific word can still fail later when Factor's compiler first compiles that word body.

For demos like `letmandelbrot.f`, the second layer is the one that matters.

## What exists today

There are already two diagnostic patterns in the tree:

1. `tests/diag_mandel.rs`
   This compiles a whole demo, feeds the IR into a live session, and checks that the definitions load.

2. `tests/diag_force_compile.rs`
   This is the useful one for word-by-word diagnosis. It loads the demo, then evaluates small probe snippets that call each helper with valid inputs. That forces Factor's lazy compiler to compile each word and surface errors at the point of first use.

There is also a fast front-end-only CLI:

1. `src/main.rs`
   `newfactor --dump=tokens|ast|sema|effects|ir|all ...`

Use the CLI to catch Rust-side compiler problems. Use the session tests to catch Factor-side compile failures.

## Quick triage

If a demo does not compile or run, do these in order.

### 1. Check the Rust pipeline only

```powershell
cargo run --bin newfactor -- --dump=all demos/letmandelbrot.f
```

This tells you whether the source fails before it ever reaches the VM.

Useful narrower stages:

```powershell
cargo run --bin newfactor -- --dump=sema demos/letmandelbrot.f
cargo run --bin newfactor -- --dump=ir demos/letmandelbrot.f
```

If this fails, the problem is in lex/parse/resolve/sema/emit.

If this succeeds, that does not prove the demo is good. It only proves the front end emitted IR.

### 2. Check that the demo definitions load into a live session

Existing example:

```powershell
cargo test --test diag_mandel -- --ignored --nocapture
```

That test currently targets `release/factorforth/demos/gfx-mandelbrot.f` and verifies that the entire file compiles and evaluates as definitions.

### 3. Force Factor to compile individual words

Existing example:

```powershell
cargo test --test diag_force_compile -- --ignored --nocapture
```

That test loads the Mandelbrot demo, then runs probes such as:

```forth
mb-bounded? .
mb-have-budget? .
mb-step
0e 0e 0.3e 0.0e 10 fractal-iter .
```

This is the key capability for finding which word actually triggers the error.

## Best workflow for `letmandelbrot.f`

The best current workflow is:

1. Compile the whole file with `newfactor --dump=ir`.
2. Load the whole file into a `Session` with `compile_in_context(...)` and `session.eval(...)`.
3. Probe each word one by one with valid stack inputs.
4. Stop on the first probe that fails.

That first failing probe is the word whose generated Factor IR is not surviving Factor's stack/effect checker.

## Real example: `diag_letmandel`

There is now a concrete example in `tests/diag_letmandel.rs`.

It uses the same pattern as `tests/diag_force_compile.rs`, but points at `release/factorforth/demos/letmandelbrot.f` and probes the LET-based helper words directly.

The test body is:

```rust
#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

#[test]
#[ignore]
fn force_compile_letmandelbrot_words() {
    let source = std::fs::read_to_string(
        "release/factorforth/demos/letmandelbrot.f"
    ).expect("read demo");

    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: out.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let mut ctx = CompileContext::new();

    let ir = compile_in_context(&source, &mut ctx).expect("compile");
    session.eval(&ir).expect("eval defs");

    let probes: &[&str] = &[
        "0 mb-count !  0e mb-x f!  0e mb-y f!  0e mb-cx f!  0e mb-cy f!  10 mb-iters !",
        "mb-bounded-step? .",
        "0e 0e 0.3e 0.0e 10 fractal-iter .",
        "5 mb-colour .",
        "64 mb-colour .",
    ];

    for src in probes {
        out.lock().unwrap().clear();
        let probe = compile_in_context(src, &mut ctx)
            .unwrap_or_else(|e| panic!("compile {src:?}: {e}"));
        session.eval(&probe)
            .unwrap_or_else(|e| panic!("eval {src:?}: {e}"));
        let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        eprintln!("{src} -> {cap:?}");
    }
}
```

Run it with:

```powershell
cargo test --test diag_letmandel -- --ignored --nocapture
```

Concrete example outcome:

1. The first version of `diag_letmandel` showed that the LET kernel was fine.
2. `mb-bounded-step?` compiled and ran.
3. `fractal-iter` compiled and ran.
4. The first failing probe was `5 mb-colour .` with `unbalanced-branches-error`.
5. Rewriting the palette helper to avoid `case` fixed the demo-specific failure.
6. `diag_letmandel` now passes end to end.

That is the intended use of this workflow: identify the smallest failing word, change only that slice, and rerun the exact same probe test until it passes.

## How to choose probes

A good probe list goes from smallest helper to largest caller.

For `letmandelbrot.f`, use this order:

1. State setup for globals and variables.
2. `mb-bounded-step?`
3. `fractal-iter`
4. `mb-colour`
5. `mb-draw` only if you have a GUI-capable path and actually want graphics involved.
6. `letmandelbrot` last.

The rule is simple: probe the smallest word that can trigger the same compiler path.

If `mb-bounded-step?` fails, you do not need to run the full demo.

## Why this works

`compile_in_context(...)` catches Rust-side compile failures and preserves prior definitions across probes.

`session.eval(...)` is the same live VM path used by the UI session worker.

A probe like:

```forth
0e 0e 0.3e 0.0e 10 fractal-iter .
```

forces Factor to compile `fractal-iter` immediately. If the generated quotation has a branch-balance or stack-effect problem, the error surfaces here instead of later during a full demo run.

## Existing commands that were validated

These commands work today in this repo:

```powershell
cargo test --test diag_mandel -- --ignored --nocapture
cargo test --test diag_force_compile -- --ignored --nocapture
cargo test --test diag_letmandel -- --ignored --nocapture
```

`diag_mandel` verifies the demo loads.

`diag_force_compile` verifies the per-word probe workflow and prints each probe's captured output.

`diag_letmandel` is the concrete example for `letmandelbrot.f`. It was used to isolate the failure to `mb-colour`, and it now passes after the palette helper was rewritten to avoid the failing `case` shape in that demo.

## Practical rule of thumb

When a `.f` file "loads" but still fails at runtime, do not debug it only through the full entry word.

Write an ignored diagnostic test that:

1. loads the file once,
2. probes one word at a time,
3. uses the smallest valid stack setup for each word,
4. stops at the first failing probe.

That is the current best way to find exactly which generated word body Factor rejects.