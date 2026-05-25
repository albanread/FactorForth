//! Smoke: the Mandelbrot demo's definitions must load into the
//! Factor VM without effect/type errors.  We don't actually call
//! gfx-mandelbrot (that opens a window); we just compile + eval
//! everything up to (but not including) the entry point invocation.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

#[test]
#[ignore]
fn mandel_demo_loads() {
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
    eprintln!("compiled IR ({} bytes)", ir.len());
    session.eval(&ir).expect("eval");

    let captured = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
    eprintln!("captured: {captured:?}");
    assert!(
        !captured.contains("ANS error"),
        "demo load surfaced an ANS error: {captured}",
    );
    assert!(
        captured.contains("Loaded:"),
        "expected 'Loaded:' banner from demo's final say-hello line; got {captured:?}",
    );
}
