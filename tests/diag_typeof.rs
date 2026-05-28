//! Runtime tests for TYPEOF and the type-introspection predicates.
//! Exercises the session-boot Factor injection: `nf-typeof`,
//! `nf-int?` / `nf-float?` / `nf-string?` / `nf-xt?` / `nf-addr-pred?`,
//! and the type-code CONSTANT:s the resolver maps to.

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

/// `42 TYPEOF .` → 1 (int-type).
#[test]
#[ignore]
fn typeof_integer() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context("42 typeof .", &mut ctx).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("1"), "int → 1, got: {cap}");
}

/// `3.14e TYPEOF .` → 2 (float-type).
#[test]
#[ignore]
fn typeof_float() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context("3.14e typeof .", &mut ctx).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("2"), "float → 2, got: {cap}");
}

/// `s$" hi" TYPEOF .` → 3 (string-type).
#[test]
#[ignore]
fn typeof_string() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(r#"s$" hi" typeof ."#, &mut ctx).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("3"), "string → 3, got: {cap}");
}

/// Type predicates return ANS booleans (-1 / 0).
#[test]
#[ignore]
fn predicates_return_ans_booleans() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        42    int?    .
        3.14e int?    .
        42    float?  .
        3.14e float?  .
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // Output should be: -1 0 0 -1
    assert!(cap.contains("-1 0 0 -1"), "predicate truths wrong: {cap}");
}

/// CASE/OF dispatch on TYPEOF — the canonical user pattern.
#[test]
#[ignore]
fn case_on_typeof() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        : describe ( x -- )
            typeof case
                int-type    of ." int "    endof
                float-type  of ." float "  endof
                string-type of ." string " endof
                ." other "
            endcase ;

        42      describe
        3.14e   describe
        s$" hi" describe
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("int "), "int branch: {cap}");
    assert!(cap.contains("float "), "float branch: {cap}");
    assert!(cap.contains("string "), "string branch: {cap}");
}

/// Round-trip through a polymorphic VALUE — TYPEOF reflects the
/// current contents.
#[test]
#[ignore]
fn typeof_through_value() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        0 value v
        v typeof .
        3.14e to v   v typeof .
        s$" x" to v  v typeof .
        "#,
        &mut ctx,
    ).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // 1 (int) 2 (float) 3 (string)
    assert!(cap.contains("1 2 3"), "value TYPEOF progression: {cap}");
}
