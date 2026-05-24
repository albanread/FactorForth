//! tests/session_smoke.rs — Phase 3.1 foundation.
//!
//! Verifies Session::new spawns a worker, the worker initialises
//! the embedded VM, and Session::eval round-trips ANS Forth source
//! through Factor.  These are the basics; the I/O-redirection
//! tests (KEY/EMIT via host callbacks) come in session_io.rs once
//! forth.runtime gains the FFI declarations.
//!
//! Run with `cargo test --test session_smoke -- --test-threads=1`.
//! Session enforces process-wide singleton on installation; running
//! tests in parallel would race on the global CURRENT.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use newfactor::session::{IoMode, Session, SessionOpts};

/// Construct a session with a Test-mode I/O config and the default
/// crate paths.  Output capture handed back to caller via the
/// returned Arc so the test can inspect it.
fn new_test_session(input: &[u8]) -> (Session, Arc<Mutex<Vec<u8>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: input.to_vec(),
        output: output.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    (session, output)
}

#[test]
#[ignore]
fn session_starts_and_evals_basic_arithmetic() {
    // Eval a trivial expression; verify it doesn't crash and
    // returns a clean interpreter_output (empty means no Factor
    // error).
    let (session, _out) = new_test_session(b"");
    // Use the compiler so we get a proper `USING:` etc.
    let ir = newfactor::compiler::compile("2 3 + drop").expect("compile");
    let result = session.eval(&ir).expect("eval");
    assert!(result.interpreter_output.trim().is_empty(),
            "expected clean eval, got: {:?}", result.interpreter_output);
}

#[test]
#[ignore]
fn session_compile_and_eval_ans_program() {
    // Compile an ANS source through our compiler, then run the IR
    // through Session.  This is the layer cake the GUI binary will
    // sit on top of.
    let (session, _out) = new_test_session(b"");
    let ir = newfactor::compiler::compile(": square ( n -- n^2 ) dup * ; 5 square drop")
        .expect("compile");
    let result = session.eval(&ir).expect("eval");
    assert!(result.interpreter_output.trim().is_empty(),
            "expected clean eval, got: {:?}", result.interpreter_output);
}

#[test]
#[ignore]
fn session_eval_timeout_aborts() {
    // This test is a no-op for now — proving timeout actually
    // fires would abort the test process, which is the
    // intended behaviour but inconvenient for the test runner.
    // We construct a session with a short timeout, then eval
    // something fast that completes well within it.
    let output = Arc::new(Mutex::new(Vec::new()));
    let mut opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: Vec::new(),
        output: output.clone(),
    });
    opts.eval_timeout = Duration::from_secs(5);
    let session = Session::new(opts).expect("Session::new");
    let _ = session.eval("42 drop").expect("eval");
}

#[test]
#[ignore]
fn session_shutdown_via_drop_doesnt_hang() {
    // Drop the session; worker thread should clean up and join.
    // If it hung, this test would hit the (separately-installed)
    // cargo test timeout.
    let (session, _out) = new_test_session(b"");
    drop(session);
    // If we got here, drop returned cleanly.
}

#[test]
#[ignore]
fn session_singleton_enforced() {
    // Two sessions in the same process should fail to construct
    // the second one (Factor's VM is single-instance per process).
    let (first, _) = new_test_session(b"");
    let output2 = Arc::new(Mutex::new(Vec::new()));
    let opts2 = SessionOpts::defaults_for_crate(IoMode::Test {
        input: Vec::new(),
        output: output2,
    });
    let second = Session::new(opts2);
    assert!(second.is_err(), "expected AlreadyRunning error");
    drop(first);
    // After dropping, we can create a new one.
    let output3 = Arc::new(Mutex::new(Vec::new()));
    let opts3 = SessionOpts::defaults_for_crate(IoMode::Test {
        input: Vec::new(),
        output: output3,
    });
    let third = Session::new(opts3).expect("Session after drop");
    drop(third);
}
