//! Diagnose where the graphics FFI fails.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

fn run(src: &str) -> (Result<(), String>, String) {
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: output.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let mut ctx = CompileContext::new();
    let ir = match compile_in_context(src, &mut ctx) {
        Ok(ir) => ir,
        Err(e) => return (Err(format!("compile: {e}")), String::new()),
    };
    eprintln!("IR:\n{ir}---");
    let res = session.eval(&ir).map(|_| ()).map_err(|e| e.to_string());
    let captured = String::from_utf8_lossy(&output.lock().unwrap()).to_string();
    (res, captured)
}

#[test]
#[ignore]
fn diag_gpane_open_with_string() {
    let (r, out) = run(r#"
        400 300 S" Shapes" gpane-open .
    "#);
    eprintln!("result: {r:?}\ncaptured: {out:?}");
}

#[test]
#[ignore]
fn diag_gpane_fill_rect_no_pane() {
    let (r, out) = run("10 20 30 40 0xFF0000 gpane-fill-rect");
    eprintln!("result: {r:?}\ncaptured: {out:?}");
}

#[test]
#[ignore]
fn diag_full_gfx_shapes_flow() {
    let (r, out) = run(r#"
        : backdrop ( id -- id )
            dup gpane-begin
            0x101830 gpane-clear
        ;
        : shapes ( -- )
            50 60 100 80 0xE83800 gpane-fill-rect
        ;
        400 300 S" t" gpane-open
        dup 0= if drop ." failed" else backdrop shapes gpane-present drop then
    "#);
    eprintln!("result: {r:?}\ncaptured: {out:?}");
}
