//! tests/session_stack_survives.rs — #53 catastrophic-underflow fix.
//!
//! The original failure mode: any program that left a value on
//! the stack — even temporarily — crashed the session with an
//! underflow-shaped error.  Factor's stock eval-callback uses
//! `eval>string` which calls `(eval)` with `( -- )` effect,
//! enforcing zero net stack change per eval.  REPL programs
//! naturally leave residue (variables, intermediates, words
//! returning values that need printing in a separate step) →
//! crash.
//!
//! The fix: a custom eval-callback that uses `parse-string`
//! plus `call( ..a -- ..b )` — row-var effect means any net
//! stack change is accepted.  REPL contract restored.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

struct Repl {
    session: Session,
    ctx:     CompileContext,
    out:     Arc<Mutex<Vec<u8>>>,
}

impl Repl {
    fn new() -> Self {
        let out = Arc::new(Mutex::new(Vec::new()));
        let opts = SessionOpts::defaults_for_crate(IoMode::Test {
            input:  vec![],
            output: out.clone(),
        });
        let session = Session::new(opts).expect("Session::new");
        Self { session, ctx: CompileContext::new(), out }
    }
    fn eval(&mut self, src: &str) {
        let ir = compile_in_context(src, &mut self.ctx)
            .unwrap_or_else(|e| panic!("compile {src:?}: {e}"));
        self.session.eval(&ir)
            .unwrap_or_else(|e| panic!("eval {src:?}: {e}"));
    }
    fn captured(&self) -> String {
        let bytes = self.out.lock().unwrap().clone();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }
    fn clear_output(&self) {
        self.out.lock().unwrap().clear();
    }
}

// ── The crash that broke the IDE ─────────────────────────────────────────

#[test]
#[ignore]
fn one_value_left_on_stack_does_not_crash() {
    // This was the catastrophic case: eval 1 leaves 5 on the
    // stack, eval 2 consumes it.  Pre-fix: eval 1 throws because
    // (eval) was called with ( -- ) effect.  Post-fix: row-var
    // effect accepts any residue.
    let mut r = Repl::new();
    r.eval("5");
    r.eval(". cr");
    assert!(r.captured().contains("5"),
        "expected 5 to survive eval boundary, got {:?}", r.captured());
}

#[test]
#[ignore]
fn many_values_survive_across_evals() {
    let mut r = Repl::new();
    r.eval("1 2 3 4 5");
    r.eval("+ + + + .");   // sum of 5 4 3 2 1 = 15
    assert!(r.captured().contains("15"),
        "1..5 sum should be 15, got {:?}", r.captured());
}

#[test]
#[ignore]
fn user_word_leaves_value_for_next_eval() {
    let mut r = Repl::new();
    r.eval(": forty-two 42 ;");
    r.eval("forty-two");   // leaves 42 on stack
    r.eval(". cr");        // prints it
    assert!(r.captured().contains("42"),
        "user word's return value should survive, got {:?}", r.captured());
}

#[test]
#[ignore]
fn three_evals_each_pushes_one_then_consume_all() {
    let mut r = Repl::new();
    r.eval("10");
    r.eval("20");
    r.eval("30");
    r.eval("+ + .");   // 10 + 20 + 30 = 60
    assert!(r.captured().contains("60"),
        "three pushed values + three pops should give 60, got {:?}", r.captured());
}

#[test]
#[ignore]
fn arithmetic_leaving_value_for_dot_works() {
    // The most natural Forth REPL flow: type the computation,
    // see the result.  Pre-fix this crashed because the
    // intermediate eval boundaries weren't net-zero.
    let mut r = Repl::new();
    r.eval("3 4 *");
    r.eval(".");
    assert!(r.captured().contains("12"),
        "3 4 * leaves 12; . prints it; got {:?}", r.captured());
}

#[test]
#[ignore]
fn dup_then_print_in_separate_evals() {
    let mut r = Repl::new();
    r.eval("7");
    r.eval("dup");
    r.eval(". .");   // prints twice
    let out = r.captured();
    // Two "7"s separated by space
    assert!(out.matches('7').count() >= 2,
        "expected 7 printed twice, got {out:?}");
}

// ── Don't break anything that already worked ───────────────────────────────

#[test]
#[ignore]
fn balanced_eval_still_works() {
    // Sanity: programs that DO balance their stack per-eval
    // should also still work.
    let mut r = Repl::new();
    r.eval("21 21 + .");
    assert!(r.captured().contains("42"),
        "balanced eval should still work, got {:?}", r.captured());
}

#[test]
#[ignore]
fn definitions_still_persist_via_compile_context() {
    // Sanity: cross-eval word resolution unchanged.
    let mut r = Repl::new();
    r.eval(": square dup * ;");
    r.eval("5 square .");
    assert!(r.captured().contains("25"),
        "definitions still persist; got {:?}", r.captured());
}

#[test]
#[ignore]
fn variables_still_persist() {
    let mut r = Repl::new();
    r.eval("variable count   0 count !");
    r.eval("1 count +!   1 count +!");
    r.clear_output();
    r.eval("count @ .");
    assert!(r.captured().contains("2"),
        "variables should still persist; got {:?}", r.captured());
}

// ── Error path: a user word that throws shouldn't crash session ───────────

#[test]
#[ignore]
fn no_method_error_caught_session_alive() {
    let mut r = Repl::new();
    // $len on an integer triggers no-method.  Session should
    // survive — we should be able to keep going.
    r.eval("42 $len");
    r.clear_output();
    r.eval("21 21 + .");
    assert!(r.captured().contains("42"),
        "session should keep working after no-method, got {:?}", r.captured());
}
