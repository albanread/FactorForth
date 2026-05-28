//! Runtime tests for VALUE and TO through the embedded Factor VM.
//!
//! Exercises the polymorphic-VALUE design choice: the same VALUE slot
//! accepts integers, floats, and strings interchangeably, because
//! we lower to Factor's tag-aware `get-global` / `set-global` rather
//! than to a typed cell.

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

/// `42 VALUE x  x .` — the basic case.
#[test]
#[ignore]
fn value_basic_int() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context("42 value x  x .", &mut ctx).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("42"), "expected 42, got: {cap}");
}

/// `42 VALUE x  100 TO x  x .` — TO rebinds.
#[test]
#[ignore]
fn to_rebinds_value() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context("42 value x  100 to x  x .", &mut ctx).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("100"), "expected 100 after TO, got: {cap}");
    assert!(!cap.contains("42"), "42 should be replaced: {cap}");
}

/// Polymorphic: a single VALUE slot accepts int, float, and string
/// in succession because Factor's globals are tag-agnostic.
#[test]
#[ignore]
fn value_polymorphic() {
    let (s, out, mut ctx) = fresh();
    // Polymorphism is structural — the SAME slot accepts int, float,
    // and string in succession.  We exercise the int and string
    // forms via `.` (works for ints) and `$.` (managed-string print).
    // Float storage is exercised by storing-and-reading without
    // attempting to print — the value's identity round-tripping
    // through Factor's tagged stack is the property we're testing,
    // not the float printer (which our forth.runtime:. is happy to
    // handle but the ANS surface word `f.` isn't wired yet).
    // Use `s$"` (managed-string literal, single tagged value) rather
    // than `S"` (ANS two-cell string).  The single-cell shape is
    // what makes VALUE truly polymorphic — one slot, one value, any
    // type Factor can tag.
    let ir = compile_in_context(
        r#"
        42 value v
        v .
        3.14e to v   v drop ." float-ok " cr
        s$" hello" to v   v $.
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("42"), "int phase: {cap}");
    assert!(cap.contains("float-ok"), "float phase: {cap}");
    assert!(cap.contains("hello"), "string phase: {cap}");
}

/// VALUE in a `:` body via TO — the classic counter-style pattern.
#[test]
#[ignore]
fn value_inside_definition() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context(
        r#"
        0 value counter
        : bump ( -- )  counter 1 + to counter ;
        bump bump bump
        counter .
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("3"), "expected counter=3 after 3 bumps: {cap}");
}

/// `TO non-value` is rejected at compile time with a clear error.
#[test]
fn to_non_value_rejected() {
    let mut ctx = CompileContext::new();
    let err = compile_in_context(": foo dup * ;  42 to foo", &mut ctx)
        .expect_err("expected ToNotValue error");
    eprintln!("err: {err}");
    assert!(err.contains("TO") && err.contains("foo"),
        "error should mention TO and the bad name: {err}");
    assert!(err.contains("VALUE"),
        "error should explain TO needs a VALUE: {err}");
}

/// Cross-eval: VALUE defined in one compile must be settable by TO
/// in a later compile.  This is the F7-checker prior-state path.
#[test]
#[ignore]
fn value_across_evals() {
    let (s, out, mut ctx) = fresh();
    // Eval 1: define the VALUE.
    let ir1 = compile_in_context("99 value tracked", &mut ctx).expect("compile 1");
    s.eval(&ir1).expect("eval 1");
    // Eval 2: rebind it; reading should see new value.
    let ir2 = compile_in_context("500 to tracked  tracked .", &mut ctx).expect("compile 2");
    eprintln!("IR2: {ir2}");
    s.eval(&ir2).expect("eval 2");
    let cap = captured(&out);
    assert!(cap.contains("500"), "cross-eval TO should rebind: {cap}");
}
