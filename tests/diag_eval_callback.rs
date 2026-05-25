//! Diagnose what specifically fails in the new eval callback.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

fn make() -> (Session, Arc<Mutex<Vec<u8>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: output.clone(),
    });
    (Session::new(opts).expect("Session::new"), output)
}

#[test]
#[ignore]
fn diag_balanced_no_flush() {
    let (s, _) = make();
    let r = s.eval("42 drop");
    eprintln!("balanced (no flush): {:?}", r);
    assert!(r.is_ok(), "balanced source should work, got {r:?}");
}

#[test]
#[ignore]
fn diag_balanced_with_flush() {
    let (s, _) = make();
    let r = s.eval("42 drop flush");
    eprintln!("balanced (with flush): {:?}", r);
    assert!(r.is_ok());
}

#[test]
#[ignore]
fn diag_just_a_number() {
    let (s, _) = make();
    let r = s.eval("42");
    eprintln!("just 42: {:?}", r);
    assert!(r.is_ok());
}

#[test]
#[ignore]
fn diag_with_in_scratchpad() {
    let (s, _) = make();
    let r = s.eval("IN: scratchpad 42 drop");
    eprintln!("with IN scratchpad: {:?}", r);
    assert!(r.is_ok());
}

#[test]
#[ignore]
fn diag_compiled_ir() {
    // What our compiler actually produces for "42 drop"
    let (s, _) = make();
    let ir = newfactor::compiler::compile("42 drop").expect("compile");
    eprintln!("compiled IR:\n{ir}");
    let r = s.eval(&ir);
    eprintln!("eval result: {:?}", r);
    assert!(r.is_ok());
}
