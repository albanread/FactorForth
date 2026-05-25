//! tests/session_tick_execute.rs — M2.x #33 partial: `'` (tick) + EXECUTE.
//!
//! The minimum machinery needed for ttester.fr-style vectoring:
//!
//!   VARIABLE ERROR-XT
//!   : ERROR  ERROR-XT @ EXECUTE ;
//!   : my-error  ." caught" cr ;
//!   ' my-error ERROR-XT !
//!
//! With `'` and EXECUTE (and the existing VARIABLE / @ / !), user
//! code can implement deferred-word vectoring without DEFER/IS at
//! all.  The Forth 2012 tester does exactly this.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

fn run_capturing(src: &str) -> String {
    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![],
        output: output.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let ir = newfactor::compiler::compile(src).expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = output.lock().unwrap();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

#[test]
#[ignore]
fn tick_then_execute_runs_user_word() {
    // ' foo execute  ≡  foo
    let out = run_capturing(r#": say-hi  ." hi from say-hi" ;  ' say-hi execute"#);
    assert_eq!(out, "hi from say-hi", "got {out:?}");
}

#[test]
#[ignore]
fn tick_xt_can_round_trip_through_stack() {
    // Push two XTs, then execute them in reverse order.  Verifies
    // an XT is just a stack value.
    let out = run_capturing(r#"
        : a  ." A" ;
        : b  ." B" ;
        ' a  ' b  execute  execute
    "#);
    assert_eq!(out, "BA", "got {out:?}");
}

#[test]
#[ignore]
fn tick_xt_stored_in_variable() {
    // The whole point of #33: the ttester pattern.
    let out = run_capturing(r#"
        variable error-xt
        : my-error  ." caught" ;
        ' my-error  error-xt !
        error-xt @ execute
    "#);
    assert_eq!(out, "caught", "got {out:?}");
}

#[test]
#[ignore]
fn tick_xt_vectoring_through_helper() {
    // Verbose simulation of ttester's `: ERROR  ERROR-XT @ EXECUTE ;`
    // pattern.  Defines the vector once, then "dispatches" through it.
    let out = run_capturing(r#"
        variable handler-xt
        : dispatch  handler-xt @ execute ;
        : impl-a    ." A!" ;
        : impl-b    ." B!" ;
        ' impl-a handler-xt !  dispatch
        ' impl-b handler-xt !  dispatch
    "#);
    assert_eq!(out, "A!B!", "got {out:?}");
}

#[test]
#[ignore]
fn tick_on_builtin_resolves() {
    // ' on a builtin word also works — Factor word-as-quotation
    // works for any resolvable name.
    let out = run_capturing("' cr execute");
    // cr writes a single newline; trimmed should be empty.
    assert_eq!(out, "", "got {out:?}");
}
