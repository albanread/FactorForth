//! Cross-eval class persistence: define a CLASS in one compile,
//! use its constructor / accessors / methods from a later compile
//! against the same Session.

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

/// Define a class in eval 1.  Construct, getter, ANS-store setter,
/// chainable setter all called from separate later evals.
#[test]
#[ignore]
fn class_visible_in_later_evals() {
    let (s, out, mut ctx) = fresh();

    // Eval 1: declare the class.  Nothing else.
    let ir1 = compile_in_context(
        "CLASS: point SLOT: x SLOT: y ;",
        &mut ctx,
    ).expect("eval 1 compile");
    s.eval(&ir1).expect("eval 1 run");

    // Eval 2: use the constructor + getters from outside the class
    // declaration.  This is what failed before — point's `<point>`
    // wasn't visible in eval N+1.
    let ir2 = compile_in_context(
        "3 4 <point>  dup point>x .  point>y .",
        &mut ctx,
    ).expect("eval 2 compile");
    eprintln!("IR2:\n{ir2}");
    s.eval(&ir2).expect("eval 2 run");
    let cap = captured(&out);
    eprintln!("captured after eval 2: {cap:?}");
    assert!(cap.contains("3"), "x via eval 2: {cap}");
    assert!(cap.contains("4"), "y via eval 2: {cap}");

    // Eval 3: ANS-style setter + chainable setter on a NEW instance.
    let ir3 = compile_in_context(
        "5 6 <point>  99 over point.x!  point>x .",
        &mut ctx,
    ).expect("eval 3 compile");
    s.eval(&ir3).expect("eval 3 run");
    let cap = captured(&out);
    assert!(cap.contains("99"), "ANS-store across evals: {cap}");
}

/// Define a generic + class in eval 1, attach a method in eval 2,
/// call it from eval 3.  This is what real REPL usage looks like.
#[test]
#[ignore]
fn method_added_in_later_eval() {
    let (s, out, mut ctx) = fresh();

    // Eval 1: class + generic, no methods yet.
    let ir1 = compile_in_context(r#"
        CLASS: shape ;
        CLASS: square EXTENDS shape  SLOT: side  ;
        GENERIC: area ( s -- a )
    "#, &mut ctx).expect("eval 1");
    s.eval(&ir1).expect("eval 1 run");

    // Eval 2: attach a method on the existing generic.
    let ir2 = compile_in_context(r#"
        METHOD: area ( s:square -- a )
            square>side dup f* ;
    "#, &mut ctx).expect("eval 2");
    eprintln!("IR2:\n{ir2}");
    s.eval(&ir2).expect("eval 2 run");

    // Eval 3: build an instance and dispatch.
    let ir3 = compile_in_context(
        "4.0e <square> area .",
        &mut ctx,
    ).expect("eval 3");
    s.eval(&ir3).expect("eval 3 run");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("16"), "4^2 = 16: {cap}");
}
