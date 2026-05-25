//! Runtime tests for `?DUP` (lower_qdup) and `RECURSE` (lower_recurse)
//! through the embedded Factor VM.  Exercises the two new pre-resolve
//! AST passes end-to-end.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

fn fresh() -> (Session, Arc<Mutex<Vec<u8>>>, CompileContext) {
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: out.clone(),
    });
    let s = Session::new(opts).expect("Session::new");
    (s, out, CompileContext::new())
}

fn captured(out: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&out.lock().unwrap()).to_string()
}

// ─── ?DUP ──────────────────────────────────────────────────────────────────

/// Classic `?DUP IF` pattern: non-zero input duplicates and the IF
/// runs with the value on the stack; zero input falls through with
/// nothing extra on the stack.
#[test]
#[ignore]
fn qdup_if_nonzero_runs_body() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        : describe ?dup if . else ." absent" then ;
        42 describe
        0 describe
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    // After lower_qdup + lower_exit there should be no Factor
    // continuations or with-return wraps for this shape.
    assert!(!ir.contains("with-return"), "no continuations needed: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("42"), "nonzero branch should print 42: {cap}");
    assert!(cap.contains("absent"), "zero branch should print absent: {cap}");
}

/// `?DUP IF` without ELSE — zero input leaves the stack with the 0
/// consumed (no else body to run).
#[test]
#[ignore]
fn qdup_if_no_else() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        : maybe-print ?dup if . then ;
        7  maybe-print
        0  maybe-print
        99 maybe-print
        "#,
        &mut ctx,
    ).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // Should see 7 and 99, but not a literal 0.
    assert!(cap.contains("7 "), "should print 7: {cap}");
    assert!(cap.contains("99 "), "should print 99: {cap}");
}

/// `?DUP` inside a loop body — common pattern for "iterate until
/// zero seen".  Just verify the IR compiles and runs.
#[test]
#[ignore]
fn qdup_inside_loop() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        : demo  3 0 do  i ?dup if . else ." z" then  loop ;
        demo
        "#,
        &mut ctx,
    ).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // i goes 0,1,2 → z, 1, 2.
    assert!(cap.contains("z"), "i=0 should print z: {cap}");
    assert!(cap.contains("1"), "i=1 should print 1: {cap}");
    assert!(cap.contains("2"), "i=2 should print 2: {cap}");
}

// ─── RECURSE ────────────────────────────────────────────────────────────────

/// The textbook recursive factorial.  Exercises:
///   * RECURSE bound to the enclosing `:` name
///   * Self-call type-checks against the declared `( n -- f )`
///   * Factor's JIT picks up tail recursion (no with-return wrap)
#[test]
#[ignore]
fn recurse_factorial() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        : fact ( n -- f )
            dup 1 < if drop 1 else dup 1 - recurse * then ;
        5 fact .
        10 fact .
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    assert!(!ir.contains("with-return"),
        "recursive fact shouldn't need continuations: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("120"), "5! = 120, got: {cap}");
    assert!(cap.contains("3628800"), "10! = 3628800, got: {cap}");
}

/// Deep recursion that would blow the stack without TCO.  With
/// lower_exit removing with-return for the non-loop EXIT shape and
/// Factor's JIT picking up the tail-call, this should run cleanly.
#[test]
#[ignore]
fn recurse_deep_countdown() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        : down ( n -- )
            dup 0= if drop exit then
            1 - recurse ;
        100000 down  ." done" cr
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("done"),
        "100k-deep recursion should run via TCO without stack overflow: {cap}");
}

/// RECURSE without an annotation must be rejected with a clear
/// compile-time error.
#[test]
fn recurse_without_annotation_rejected() {
    let mut ctx = CompileContext::new();
    let err = compile_in_context(
        ": bad dup 1 < if drop 1 else dup 1 - recurse * then ;",
        &mut ctx,
    ).expect_err("expected sema error for un-annotated RECURSE");
    eprintln!("err: {err}");
    assert!(err.contains("RECURSE") && err.contains("annotation"),
        "error should mention RECURSE and annotation: {err}");
}

/// Recursive mutual call: a calls b, b is RECURSE-free.  Sanity that
/// non-RECURSE self-references aren't flagged.
#[test]
#[ignore]
fn non_recurse_word_unaffected() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        ": square ( n -- nn ) dup * ;
         5 square .",
        &mut ctx,
    ).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("25"), "5*5 = 25, got: {cap}");
}
