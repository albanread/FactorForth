//! CoreProtocols — the standard class library (Layer 0 onward).
//!
//! The library source ships as `release/factorforth/lib/*.f`, written
//! in ordinary ANS Forth on the object system.  These tests load a
//! layer's source and exercise its protocol the way user code would.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

/// Library source, embedded from the shipped files so the tests and
/// the release artifacts never drift.
const CORE: &str = include_str!("../release/factorforth/lib/core.f");
const COLLECTIONS: &str = include_str!("../release/factorforth/lib/collections.f");

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

// ── Layer 1: grid ───────────────────────────────────────────────

/// A grid stores and retrieves cells by (x, y), 0-based.  Write a
/// few cells, read them back.
#[test]
#[ignore]
fn grid_stores_and_reads_by_xy() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ a 3-wide, 2-tall grid, held in a VALUE for clean access
        3 2 new-grid VALUE board

        \ set (0,0)=11, (2,0)=22, (1,1)=33
        11  0 0 board at-xy!
        22  2 0 board at-xy!
        33  1 1 board at-xy!

        \ read them back, in order
        0 0 board at-xy .
        2 0 board at-xy .
        1 1 board at-xy .
        \ an untouched cell reads 0
        1 0 board at-xy .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("11 ") && cap.contains("22 ") && cap.contains("33 "),
        "stored cells read back: {cap}");
    // the four `.` outputs, in order: 11 22 33 0
    assert!(cap.contains("11 22 33 0"), "in (x,y) order incl untouched=0: {cap}");
}

/// in-bounds? is 0-based and (x,y): valid columns are 0..w, rows
/// 0..h; negatives and over-edge are out.
#[test]
#[ignore]
fn grid_in_bounds_is_zero_based_xy() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        3 2 new-grid          \ x in 0..2, y in 0..1

        dup 0 0 rot in-bounds? .   \ -1  (origin, in)
        dup 2 1 rot in-bounds? .   \ -1  (far corner, in)
        dup 3 0 rot in-bounds? .   \  0  (x == w, out)
        dup 0 2 rot in-bounds? .   \  0  (y == h, out)
        -1 0 rot in-bounds? .      \  0  (negative x, out)
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // -1 -1 0 0 0  (ANS true is -1, false is 0)
    assert!(cap.contains("-1 -1 0 0 0"), "bounds flags: {cap}");
}
