//! CoreProtocols — the standard class library (Layer 0 onward).
//!
//! The library source ships as `release/factorforth/lib/*.f`, written
//! in ordinary ANS Forth on the object system.  These tests load a
//! layer's source and exercise its protocol the way user code would.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

/// Layer 0 source, embedded from the shipped library file so the test
/// and the release artifact never drift.
const CORE: &str = include_str!("../release/factorforth/lib/core.f");

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

/// Compile + eval a source string, panicking on a compile error.
fn run(s: &Session, ctx: &mut CompileContext, src: &str) {
    let ir = compile_in_context(src, ctx).unwrap_or_else(|e| panic!("compile: {e}"));
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
}

/// A class that implements `show` gets its own rendering; calling
/// `show` dispatches to it.
#[test]
#[ignore]
fn show_dispatches_to_class_method() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: point SLOT: x SLOT: y ;
        METHOD: show ( p:point -- )
            ." (" dup point>x . ." ," point>y . ." )" ;

        3 4 <point> show
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // `.` prints "n " (trailing space), so the rendering is "(3 ,4 )".
    assert!(cap.contains("(3 ,4 )"), "point show: {cap}");
}

/// A type with no `show` method falls back to the object catch-all,
/// so `show` is total — it never fails to dispatch.
#[test]
#[ignore]
fn show_object_default_is_total() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: widget ;
        <widget> show
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("<object>"), "object default: {cap}");
}

/// `show-ln` is defined once over the generic and works for any class
/// that implements `show` — protocol reuse, not per-class code.
#[test]
#[ignore]
fn show_ln_reuses_the_protocol() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: tag SLOT: n ;
        METHOD: show ( t:tag -- )  ." #" tag>n . ;

        5 <tag> show-ln
        9 <tag> show-ln
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // `show-ln` ran the per-class `show` twice, each on its own line.
    assert!(cap.contains("#5"), "tag1: {cap}");
    assert!(cap.contains("#9"), "tag2: {cap}");
    assert_eq!(cap.matches('#').count(), 2, "two shows: {cap}");
}
