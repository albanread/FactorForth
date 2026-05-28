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
const OTHELLO: &str = include_str!("../release/factorforth/lib/othello.f");

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

/// darray grows on push; size/at read it back in order.
#[test]
#[ignore]
fn darray_grows_and_reads() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        10 xs d-push
        20 xs d-push
        30 xs d-push
        xs size .            \ 3
        0 xs at .           \ 10
        2 xs at .           \ 30
        \ overwrite element 1
        99 1 xs at!
        1 xs at .           \ 99
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("3 10 30 99"), "size/at/at!: {cap}");
}

/// The collection protocol is polymorphic: `size` and `at` work on a
/// grid and a darray through the same generics — write an algorithm
/// once, run it on either backing.
#[test]
#[ignore]
fn collection_protocol_is_polymorphic() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ a generic word over ANY collection: print its size
        : .size ( c -- )  size . ;

        3 2 new-grid .size      \ grid: 6 cells
        new-darray
        dup 5 swap d-push
        dup 6 swap d-push
        .size                   \ darray: 2 elements
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // 6 from the grid, 2 from the darray — same `.size` word, two types
    assert!(cap.contains("6 2"), "polymorphic size over grid + darray: {cap}");
}

/// `each` runs an xt over every element — defined once over the
/// protocol, so it works on a darray and a grid alike.  Here the xt
/// is the builtin `.` (print), gathered with `'`.
#[test]
#[ignore]
fn each_iterates_any_collection() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        2 xs d-push  3 xs d-push  4 xs d-push
        ." darray: " xs ' . each cr

        \ each also walks a grid's cells in linear (row-major) order
        2 2 new-grid VALUE g
        7  0 0 g at-xy!          \ only (0,0) is non-zero
        ." grid: " g ' . each cr
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // darray elements in push order
    assert!(cap.contains("darray: 2 3 4"), "each over darray: {cap}");
    // grid cells linear: (0,0)=7 then three zeros
    assert!(cap.contains("grid: 7 0 0 0"), "each over grid cells: {cap}");
}

/// `each` composes with user accumulators too (xt defined in an
/// earlier eval, so it's in the dictionary when ticked — see the
/// same-compile tick-ordering note).
#[test]
#[ignore]
fn each_with_user_accumulator() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, "0 VALUE acc  : add-acc ( n -- ) acc + TO acc ;");
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        2 xs d-push  3 xs d-push  4 xs d-push
        xs ' add-acc each
        ." sum=" acc .          \ 9
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("sum=9"), "each + accumulator: {cap}");
}

// ── Phase 1 capstone: text Othello ──────────────────────────────

/// The opening position renders as the standard Othello board — the
/// central four squares, everything else empty.  Proves Layer 0 +
/// Layer 1 compose into a real program.
#[test]
#[ignore]
fn othello_opening_board_renders() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, OTHELLO);
    run(&s, &mut ctx, "othello-new show-board");
    let cap = captured(&out);
    eprintln!("captured:\n{cap}");
    // rows 3 and 4 carry the centre; the rest are all dots.
    assert!(cap.contains("...OX..."), "row 3 (O X): {cap}");
    assert!(cap.contains("...XO..."), "row 4 (X O): {cap}");
    assert!(cap.contains("........"), "an empty row: {cap}");
    // 64 cells total: 60 empty dots + 2 X + 2 O
    assert_eq!(cap.matches('.').count(), 60, "60 empty cells: {cap}");
    assert_eq!(cap.matches('X').count(), 2, "2 black: {cap}");
    assert_eq!(cap.matches('O').count(), 2, "2 white: {cap}");
}

/// Playing a legal opening move flips the bracketed disc.  Black at
/// (2,3) brackets the white at (3,3) against the black at (4,3),
/// flipping it: row 3 becomes `..XXX...`, and the board goes 4 black
/// / 1 white.
#[test]
#[ignore]
fn othello_play_flips_a_disc() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, OTHELLO);
    run(&s, &mut ctx, "othello-new  2 3 black play  show-board");
    let cap = captured(&out);
    eprintln!("captured:\n{cap}");
    assert!(cap.contains("..XXX..."), "row 3 after flip: {cap}");
    assert!(cap.contains("...XO..."), "row 4 unchanged: {cap}");
    assert_eq!(cap.matches('X').count(), 4, "4 black after flip: {cap}");
    assert_eq!(cap.matches('O').count(), 1, "1 white after flip: {cap}");
}
