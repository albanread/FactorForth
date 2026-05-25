//! Runtime test for ANS EXIT.  Exercises the AST tail-inlining
//! transform end-to-end: definitions that use EXIT should compile
//! and behave correctly under the embedded Factor VM, AND the
//! emitted IR should be free of `continuations:with-return` in
//! the common (non-loop) case so Factor's JIT picks up the fast
//! path.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{
    compile_in_context, compile_in_context_with_diagnostics,
    CompileContext,
};
use newfactor::session::{IoMode, Session, SessionOpts};

fn fresh_session() -> (Session, Arc<Mutex<Vec<u8>>>, CompileContext) {
    let out = Arc::new(Mutex::new(Vec::<u8>::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![], output: out.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let ctx = CompileContext::new();
    (session, out, ctx)
}

/// EXIT at top level: `: w 42 EXIT 99 ;`
/// Expected runtime: pushes 42 only.  `99` after EXIT is dead.
#[test]
#[ignore]
fn exit_top_level_drops_tail() {
    let (session, out, mut ctx) = fresh_session();
    let ir = compile_in_context(
        ": w 42 exit 99 ;  w .",
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    // Fast path: no with-return wrap should be needed.
    assert!(!ir.contains("with-return"),
        "top-level EXIT should be tail-inlined, not wrapped: {ir}");
    session.eval(&ir).expect("eval");
    let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("42"), "expected 42 from EXIT-truncated body: {cap}");
    assert!(!cap.contains("99"), "99 should be dead-stripped: {cap}");
}

/// EXIT inside IF, with code after THEN — the mb-colour shape.
/// `: w 1 if 42 exit then 99 ; w .` → 42 (then branch with EXIT).
/// `: w 0 if 42 exit then 99 ; w .` → 99 (else falls through to tail).
#[test]
#[ignore]
fn exit_inside_if_with_tail_after() {
    // Case A: condition true, EXIT fires.
    {
        let (session, out, mut ctx) = fresh_session();
        let ir = compile_in_context(
            ": wa -1 if 42 exit then 99 ;  wa .",
            &mut ctx,
        ).expect("compile");
        eprintln!("IR (true): {ir}");
        assert!(!ir.contains("with-return"),
            "IF-EXIT should be tail-inlined, not wrapped: {ir}");
        session.eval(&ir).expect("eval");
        let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        assert!(cap.contains("42"), "true-branch should print 42: {cap}");
        assert!(!cap.contains("99"), "true-branch must NOT print 99: {cap}");
    }
    // Case B: condition false, fall through to tail.
    {
        let (session, out, mut ctx) = fresh_session();
        let ir = compile_in_context(
            ": wb 0 if 42 exit then 99 ;  wb .",
            &mut ctx,
        ).expect("compile");
        eprintln!("IR (false): {ir}");
        assert!(!ir.contains("with-return"));
        session.eval(&ir).expect("eval");
        let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        assert!(cap.contains("99"), "false-branch should reach tail 99: {cap}");
        assert!(!cap.contains("42"), "false-branch must NOT print 42: {cap}");
    }
}

/// Mandelbrot-style: `dup MAX = if drop 0 exit then 15 and case ...`
/// This is the exact pattern that bit gfx-mandelbrot before the fix.
#[test]
#[ignore]
fn exit_then_case_mb_colour_shape() {
    let (session, out, mut ctx) = fresh_session();
    let src = r#"
        : mc ( n -- v )
            dup 64 = if drop 0 exit then
            3 and case
                0 of 100 endof
                1 of 200 endof
                2 of 300 endof
                3 of 400 endof
                999
            endcase ;
        64 mc .
         0 mc .
         1 mc .
         5 mc .
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    assert!(!ir.contains("with-return"),
        "mb-colour shape should compile without with-return: {ir}");
    session.eval(&ir).expect("eval");
    let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
    eprintln!("captured: {cap:?}");
    assert!(!cap.contains("ANS error"), "no ANS errors expected: {cap}");
    // 64 → 0 (the EXIT branch), 0 → 100, 1 → 200, 5 (= 5 & 3 = 1) → 200.
    for v in &["0", "100", "200"] {
        assert!(cap.contains(v), "expected {v} in: {cap}");
    }
}

/// EXIT inside a DO/LOOP body: lower_exit leaves the body opaque,
/// so the with-return wrap is still emitted.  This test pins the
/// behaviour we *do* produce today — the wrap is present.
///
/// Why we don't also assert runtime correctness here: Factor's
/// `?do-loop` combinator requires the body quotation to have a
/// fixed `( -- step )` effect, and a `[ ... return ]` quotation
/// has `( -- * )` which the strict effect checker rejects under
/// the loop combinator (independent of `with-return`).  Making
/// EXIT-in-loop also work at runtime is the Rec 2 follow-up
/// (lower the loop into a recursive tail-call form that breaks via
/// a flag rather than a non-local jump).  Tracked as task #54's
/// own follow-up.
#[test]
#[ignore]
fn exit_inside_loop_keeps_with_return_wrap() {
    let mut ctx = CompileContext::new();
    let (ir, _diag) = compile_in_context_with_diagnostics(
        r#"
            : scan ( -- i )
                10 0 ?do
                    i 3 = if i unloop exit then
                loop
                -1 ;
        "#,
        &mut ctx,
    ).expect("compile");
    eprintln!("IR:\n{ir}");
    assert!(ir.contains("with-return"),
        "EXIT inside DO/LOOP should retain with-return wrap: {ir}");
    assert!(ir.contains("continuations:return"),
        "EXIT in loop body should still emit as continuations:return: {ir}");
}

/// Both IF branches EXIT — tail after IF is dead.
#[test]
#[ignore]
fn both_branches_exit_kills_tail() {
    let (session, out, mut ctx) = fresh_session();
    let ir = compile_in_context(
        ": w -1 if 1 exit else 2 exit then 99 ;  w .",
        &mut ctx,
    ).expect("compile");
    eprintln!("IR: {ir}");
    assert!(!ir.contains("with-return"));
    session.eval(&ir).expect("eval");
    let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
    assert!(cap.contains("1"), "true-branch should print 1: {cap}");
    assert!(!cap.contains("99"), "99 is dead after both-exit: {cap}");
}
