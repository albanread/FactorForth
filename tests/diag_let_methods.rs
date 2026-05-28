//! Verify LET-method syntax: `name:class as slot1 slot2 ...` in the
//! LET input list lets method bodies be written as plain infix math.

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

/// The textbook LET-method: 2D distance.  Two point arguments, each
/// destructured into named slot-locals, body is pure infix math.
#[test]
#[ignore]
fn let_method_distance() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point  SLOT: x  SLOT: y  ;
        : distance ( a b -- d )
            LET ( a:point as ax ay, b:point as bx by ) -> ( d ) =
                sqrt((bx - ax)^2 + (by - ay)^2)
            END ;
        0.0e 0.0e <point>  3.0e 4.0e <point>  distance .
    "#;
    let ir = compile_in_context(src, &mut ctx);
    let ir = match ir {
        Ok(ir) => ir,
        Err(e) => { eprintln!("compile err: {e}"); panic!("compile: {e}"); }
    };
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // sqrt(3^2 + 4^2) = 5
    assert!(cap.contains("5."), "distance ~5.0: {cap}");
}

/// Mixed inputs: some destructured, some not.  The non-destructured
/// names are regular LET locals.
#[test]
#[ignore]
fn let_method_mixed_inputs() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point  SLOT: x  SLOT: y  ;
        \ Scale a point's distance-from-origin by a factor.  The point
        \ is destructured; the factor is a plain float input.
        : scaled-magnitude ( p factor -- d )
            LET ( p:point as x y, factor ) -> ( d ) =
                factor * sqrt(x^2 + y^2)
            END ;
        3.0e 4.0e <point>  2.0e  scaled-magnitude .
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // 2 * sqrt(9 + 16) = 2 * 5 = 10
    assert!(cap.contains("10."), "scaled magnitude = 10.0: {cap}");
}

/// LET-method body inside an actual METHOD: declaration on a class.
/// This is the full intent — method dispatched on class, body written
/// in algebraic LET form.
#[test]
#[ignore]
fn let_method_in_actual_method() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: vec2  SLOT: x  SLOT: y  ;
        GENERIC: magnitude ( v -- m )
        METHOD: magnitude ( v:vec2 -- m )
            LET ( v:vec2 as x y ) -> ( m ) =
                sqrt(x^2 + y^2)
            END ;
        3.0e 4.0e <vec2>  magnitude .
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("5."), "magnitude = 5.0: {cap}");
}

/// Plain LET (no destructuring) still works — the change is
/// strictly additive.  Regression guard.
#[test]
#[ignore]
fn plain_let_still_works() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        : hypot ( a b -- c )
            LET ( a b ) -> ( c ) =
                sqrt(a^2 + b^2)
            END ;
        3.0e 4.0e hypot .
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("5."), "hypot = 5.0: {cap}");
}
