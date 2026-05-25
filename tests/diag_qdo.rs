#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

#[test]
#[ignore]
fn qdo_runtime_behaviour() {
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: out.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let mut ctx = CompileContext::new();

    let ir = compile_in_context(
        ": sum 0 swap 0 ?do i + loop ;  5 sum .  0 sum .",
        &mut ctx,
    ).expect("compile");
    eprintln!("IR:\n{ir}");
    session.eval(&ir).expect("eval");

    let captured = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
    eprintln!("captured: {captured:?}");
    // 5 sum = 0+1+2+3+4 = 10 ; 0 sum = 0 (?do skips when limit==start)
    assert!(captured.contains("10"), "expected 10 in output");
    assert!(captured.contains("0 "), "expected 0 in output");
}
