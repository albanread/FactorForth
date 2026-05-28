//! Multi-method dispatch — methods that dispatch on more than one
//! argument's class.  Backed by Factor's `multi-methods` vocab,
//! which we bake into our image at build time.

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

/// Rock/paper/scissors as classic multi-dispatch.  Three classes,
/// one generic that dispatches on BOTH arguments' classes.
/// Different class combinations get different method bodies.
#[test]
#[ignore]
fn rock_paper_scissors() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: rock     ;
        CLASS: paper    ;
        CLASS: scissors ;

        GENERIC: beats? ( a b -- ? )

        METHOD: beats? ( a:paper    b:rock     -- ? )  2drop -1 ;
        METHOD: beats? ( a:rock     b:scissors -- ? )  2drop -1 ;
        METHOD: beats? ( a:scissors b:paper    -- ? )  2drop -1 ;
        METHOD: beats? ( a:rock     b:rock     -- ? )  2drop 0 ;
        METHOD: beats? ( a:paper    b:paper    -- ? )  2drop 0 ;
        METHOD: beats? ( a:scissors b:scissors -- ? )  2drop 0 ;
        METHOD: beats? ( a:rock     b:paper    -- ? )  2drop 0 ;
        METHOD: beats? ( a:paper    b:scissors -- ? )  2drop 0 ;
        METHOD: beats? ( a:scissors b:rock     -- ? )  2drop 0 ;

        <paper>    <rock>     beats? .
        <scissors> <rock>     beats? .
        <rock>     <paper>    beats? .
        <paper>    <paper>    beats? .
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
    // paper beats rock -> -1
    // scissors loses to rock -> 0
    // rock loses to paper -> 0
    // paper ties paper -> 0
    assert!(cap.contains("-1 0 0 0"), "expected '-1 0 0 0', got: {cap}");
}

/// Geometric intersection — circle/line, line/line, line/circle each
/// get their own implementation.  This is the canonical reason
/// multi-dispatch exists: there's no "natural" owner for the operation.
#[test]
#[ignore]
fn geometric_intersect() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: line    ;
        CLASS: circle  ;

        GENERIC: intersect ( a b -- kind )

        METHOD: intersect ( a:line   b:line   -- kind )  2drop s$" line-line"    ;
        METHOD: intersect ( a:line   b:circle -- kind )  2drop s$" line-circle"  ;
        METHOD: intersect ( a:circle b:line   -- kind )  2drop s$" circle-line"  ;
        METHOD: intersect ( a:circle b:circle -- kind )  2drop s$" circle-circle";

        <line>   <line>   intersect $.   space
        <line>   <circle> intersect $.   space
        <circle> <line>   intersect $.   space
        <circle> <circle> intersect $.
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("line-line"));
    assert!(cap.contains("line-circle"));
    assert!(cap.contains("circle-line"));
    assert!(cap.contains("circle-circle"));
}

/// Method specificity: a more-specific class combo wins over a less-
/// specific one.  Set up a base + subclass and check that the
/// (subclass, subclass) method is preferred over (base, base) when
/// available.
#[test]
#[ignore]
fn multi_dispatch_specificity() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: animal ;
        CLASS: cat EXTENDS animal ;
        CLASS: dog EXTENDS animal ;

        GENERIC: greet ( a b -- )

        \ Generic fallback for any two animals:
        METHOD: greet ( a:animal b:animal -- )  2drop ." general greeting" cr ;
        \ Specific for two cats:
        METHOD: greet ( a:cat b:cat -- )  2drop ." purrs together" cr ;

        <cat> <cat> greet
        <dog> <dog> greet
        <cat> <dog> greet
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("purrs together"),  "cat+cat → specific method: {cap}");
    // dog+dog and cat+dog both fall through to the (animal, animal) base
    let general_count = cap.matches("general greeting").count();
    assert_eq!(general_count, 2, "two pairs should hit the general method: {cap}");
}
