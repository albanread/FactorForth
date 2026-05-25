//! Force Factor to compile the demo's words by invoking each one
//! with dummy arguments — that surfaces "not compiled" errors
//! that only appear when the word is actually called.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

#[test]
#[ignore]
fn force_compile_fractal_iter() {
    let source = std::fs::read_to_string(
        "release/factorforth/demos/gfx-mandelbrot.f"
    ).expect("read demo");

    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: out.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let mut ctx = CompileContext::new();

    let ir = compile_in_context(&source, &mut ctx).expect("compile");
    session.eval(&ir).expect("eval defs");
    out.lock().unwrap().clear();

    // Force compilation of every word the IDE will touch
    // when it runs `gfx-mandelbrot`.  We don't actually open
    // a pane (that needs a GUI frame) — we just call each
    // word with valid stack inputs so Factor's lazy compile
    // fires and any effect-check error surfaces.
    let probes: &[&str] = &[
        // helpers
        "0 mb-count !  0e mb-x f!  0e mb-y f!  0e mb-cx f!  0e mb-cy f!  10 mb-iters !",
        "mb-bounded? .",
        "mb-have-budget? .",
        "mb-step",
        "0e 0e 0.3e 0.0e 10 fractal-iter .",
        "5 mb-colour .",
        "63 mb-colour .",
        "64 mb-colour .",
    ];
    for src in probes {
        out.lock().unwrap().clear();
        let probe = compile_in_context(src, &mut ctx)
            .unwrap_or_else(|e| panic!("compile {src:?}: {e}"));
        session.eval(&probe).unwrap_or_else(|e| panic!("eval {src:?}: {e}"));
        let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        eprintln!("{src} -> {cap:?}");
        assert!(!cap.contains("not compiled"),
            "src={src:?} got: {cap}");
        assert!(!cap.contains("ANS error"),
            "src={src:?} got: {cap}");
    }
}
