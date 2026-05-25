//! Verify that the interrupt-on-timeout machinery actually
//! interrupts an infinite loop now that Factor's SEH handler is
//! registered for the eval-callback path.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

#[test]
#[ignore]
fn diag_infinite_loop_interrupts_cleanly() {
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: output.clone(),
    });
    // Tight timeout so the test runs in a reasonable time.
    opts.eval_timeout = Duration::from_secs(2);
    let session = Session::new(opts).expect("Session::new");

    let mut ctx = CompileContext::new();
    let ir = compile_in_context("begin 1 drop again", &mut ctx)
        .expect("compile");
    eprintln!("IR:\n{ir}");

    // This should run for ~2s, get interrupted, and return.
    let start = std::time::Instant::now();
    let result = session.eval(&ir);
    let elapsed = start.elapsed();
    eprintln!("eval result: {result:?}");
    eprintln!("elapsed: {elapsed:?}");
    eprintln!("captured: {:?}",
        String::from_utf8_lossy(&output.lock().unwrap()));

    // After interrupt, session should still be usable.
    if !session.is_dead() {
        eprintln!("session still alive — running follow-up eval");
        let ir2 = compile_in_context("21 21 + .", &mut ctx).expect("compile");
        let res2 = session.eval(&ir2);
        eprintln!("follow-up: {res2:?}");
        eprintln!("captured after: {:?}",
            String::from_utf8_lossy(&output.lock().unwrap()));
    } else {
        eprintln!("session died: {:?}", session.death_cause());
    }
}
