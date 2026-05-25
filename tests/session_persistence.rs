//! tests/session_persistence.rs — REPL session state persistence.
//!
//! The promise: in one Session, you can compile multiple definitions,
//! multiple variables, multiple constants, multiple buffers — and
//! reference any of them in later evals.  This matches every
//! interactive Forth users have used since Chuck Moore.
//!
//! Each test drives a sequence of evals against ONE Session +
//! ONE CompileContext.  Output accumulates across evals; we
//! check that the final-eval visible behaviour reflects the
//! cumulative state.
//!
//! Failures here are the headline UX gap — the user can't use
//! NewFactor interactively until these pass.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

/// Shared state used by every test in this file.
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

    /// Run one snippet through the same Session + CompileContext.
    /// Panics on compile or eval failure (the test should drive
    /// only well-formed input; bad input would mask the
    /// persistence question we're testing).
    fn eval(&mut self, src: &str) {
        let ir = compile_in_context(src, &mut self.ctx)
            .unwrap_or_else(|e| panic!("compile {src:?}: {e}"));
        self.session.eval(&ir)
            .unwrap_or_else(|e| panic!("eval {src:?}: {e}"));
    }

    /// Drain the accumulated captured output, trim, return.
    fn captured(&self) -> String {
        let bytes = self.out.lock().unwrap().clone();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }

    /// Clear the captured output buffer.  Useful between evals
    /// when you only care about the LAST one's output.
    fn clear_output(&self) {
        self.out.lock().unwrap().clear();
    }
}

// ── Definitions ────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn two_definitions_compose() {
    let mut r = Repl::new();
    r.eval(": double dup + ;");
    r.eval(": quad double double ;");
    r.eval("5 quad .");
    assert!(r.captured().contains("20"),
        "5 quad should be 20, got {:?}", r.captured());
}

#[test]
#[ignore]
fn redefining_a_word_updates_dictionary() {
    // Factor allows redefinition; our REPL should too.
    let mut r = Repl::new();
    r.eval(": foo 1 ;");
    r.eval("foo .");
    r.clear_output();
    r.eval(": foo 2 ;");     // redefine
    r.eval("foo .");
    assert!(r.captured().contains("2"),
        "redefined foo should be 2, got {:?}", r.captured());
}

#[test]
#[ignore]
fn definitions_chain_three_deep() {
    let mut r = Repl::new();
    r.eval(": a 10 ;");
    r.eval(": b a 1+ ;");
    r.eval(": c b 1+ ;");
    r.eval("c .");
    assert!(r.captured().contains("12"),
        "10+1+1 should be 12, got {:?}", r.captured());
}

// ── Constants ──────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn multiple_constants_and_a_word_using_them() {
    let mut r = Repl::new();
    r.eval("100 constant cents-per-dollar");
    r.eval("25 constant quarter");
    r.eval(": dollars cents-per-dollar / ;");
    r.eval(": quarters quarter / ;");
    r.eval("400 dollars . 100 quarters .");
    let out = r.captured();
    assert!(out.contains("4") && out.contains("4"),
        "expected '4 4' for 400 dollars + 100 quarters, got {out:?}");
}

// ── Variables ──────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn variable_defined_in_eval1_readable_in_eval2() {
    // The bug from yesterday.  Variables should persist their
    // value-cell across evals.
    let mut r = Repl::new();
    r.eval("variable counter");
    r.eval("42 counter !");
    r.clear_output();
    r.eval("counter @ .");
    assert!(r.captured().contains("42"),
        "counter @ should be 42 across evals, got {:?}", r.captured());
}

#[test]
#[ignore]
fn two_variables_each_holds_its_own_value() {
    let mut r = Repl::new();
    r.eval("variable a   variable b");
    r.eval("10 a !   20 b !");
    r.clear_output();
    r.eval("a @ b @ + .");   // 30
    assert!(r.captured().contains("30"),
        "a + b should be 30, got {:?}", r.captured());
}

#[test]
#[ignore]
fn variable_incremented_across_evals() {
    let mut r = Repl::new();
    r.eval("variable n   0 n !");
    r.eval("1 n +!");
    r.eval("1 n +!");
    r.eval("1 n +!");
    r.clear_output();
    r.eval("n @ .");
    assert!(r.captured().contains("3"),
        "+! across evals should accumulate to 3, got {:?}", r.captured());
}

#[test]
#[ignore]
fn variable_used_by_word_defined_later() {
    let mut r = Repl::new();
    r.eval("variable score");
    r.eval("100 score !");
    r.eval(": bonus  10 score +! ;");
    r.eval("bonus bonus bonus");      // +30
    r.clear_output();
    r.eval("score @ .");
    assert!(r.captured().contains("130"),
        "score after 3 bonuses should be 130, got {:?}", r.captured());
}

// ── Mixed: definitions + variables + constants ─────────────────────────────

#[test]
#[ignore]
fn mixed_state_compounds() {
    let mut r = Repl::new();
    r.eval("3 constant width");
    r.eval("4 constant height");
    r.eval(": area  width height * ;");
    r.eval("variable total");
    r.eval("0 total !");
    r.eval(": add-area  area total +! ;");
    r.eval("add-area add-area add-area");   // 3 areas of 12
    r.clear_output();
    r.eval("total @ .");
    assert!(r.captured().contains("36"),
        "3 × 12 should be 36, got {:?}", r.captured());
}

// ── Arrays / collections ───────────────────────────────────────────────────

#[test]
#[ignore]
fn array_persists_across_evals() {
    let mut r = Repl::new();
    r.eval("5 array nums");
    r.eval("10 0 nums !");
    r.eval("20 1 nums !");
    r.eval("30 2 nums !");
    r.clear_output();
    r.eval("0 nums @  1 nums @  2 nums @  + + .");
    assert!(r.captured().contains("60"),
        "array sum should be 60, got {:?}", r.captured());
}

#[test]
#[ignore]
fn array_initialized_and_summed_in_loop() {
    let mut r = Repl::new();
    r.eval("3 array xs");
    r.eval("100 0 xs !   200 1 xs !   300 2 xs !");
    r.eval(": sum-xs   0   3 0 do  i xs @ +  loop ;");
    r.clear_output();
    r.eval("sum-xs .");
    assert!(r.captured().contains("600"),
        "100+200+300 should be 600, got {:?}", r.captured());
}

// ── CREATE/DOES> templates ─────────────────────────────────────────────────

#[test]
#[ignore]
fn create_does_template_persists_and_instances_are_independent() {
    let mut r = Repl::new();
    // The array-template idiom — our M2.9b CREATE/DOES> shape.
    // (The ANS `: foo create , does> @ ;` constant-template
    // shape uses `,` which we don't ship — our `array` defining
    // word covers the constant-via-allot use case differently.)
    //
    // Each eval must be net `( -- )` (Factor's `(eval)` enforces
    // it).  So we read + print in one eval rather than pushing
    // four values in one eval and printing in the next.  Filed
    // #54 for the broader "values survive between evals" issue.
    r.eval(": myarray  create cells allot  does> swap cells + ;");
    r.eval("4 myarray xs");
    r.eval("4 myarray ys");
    r.eval("100 0 xs !   200 1 xs !");
    r.eval("777 0 ys !   888 1 ys !");
    r.clear_output();
    r.eval("0 xs @ . 1 xs @ . 0 ys @ . 1 ys @ .");
    let out = r.captured();
    // Either ordering — what matters is that all four values
    // appear (xs and ys are independent instances).
    assert!(out.contains("100") && out.contains("200")
         && out.contains("777") && out.contains("888"),
        "expected all four cells distinct, got {out:?}");
}

// ── Sanity: REPL works at all ──────────────────────────────────────────────

#[test]
#[ignore]
fn basic_smoke_repl_evaluates() {
    let mut r = Repl::new();
    r.eval("2 3 + .");
    assert!(r.captured().contains("5"),
        "basic 2 3 + . should print 5, got {:?}", r.captured());
}
