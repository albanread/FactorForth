//! Functional combinators (Layer 0, core.f) — end-to-end against
//! the embedded VM.  Each test exercises one combinator with a
//! short readable scenario, not a heavy use case.  The point is to
//! verify the stack effects and ordering — heavier composition is
//! exercised indirectly by everything else that uses them.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

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

fn run(s: &Session, ctx: &mut CompileContext, src: &str) {
    let ir = compile_in_context(src, ctx).unwrap_or_else(|e| panic!("compile: {e}"));
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
}

/// `keep` calls an xt with one value and restores that value on top.
#[test]
#[ignore]
fn keep_preserves_the_value() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : show-int ( n -- ) . ;
        ." kept=" 42 ' show-int keep .       \ prints "kept=42 42 "
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("kept=42 42 "), "keep restores: {cap}");
}

/// `keep>` stacks the xt's result above the original value.
#[test]
#[ignore]
fn keep_result_stacks_above_original() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : double ( n -- m ) 2 * ;
        ." orig+double=" 5 ' double keep> . .   \ stack: ( 10 5 ); prints "5 10 "
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("orig+double=5 10"), "keep> ordering: {cap}");
}

/// `2keep` is keep for two inputs — restores both.
#[test]
#[ignore]
fn two_keep_preserves_pair() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : add ( a b -- ) + . ;
        ." [" 3 4 ' add 2keep . . ." ]"    \ prints "[7 4 3 ]" (sum, then b, then a)
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[7 4 3 ]"), "2keep restores both: {cap}");
}

/// `bi` applies two xts to the same value in order — the classic
/// "do two side-effects on the same input" combinator.
#[test]
#[ignore]
fn bi_applies_both_to_same_value() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : as-doubled ( n -- ) ." dbl:" 2 * . ;
        : as-squared ( n -- ) ." sqr:" dup * . ;
        5 ' as-doubled ' as-squared bi
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("dbl:10"), "bi first xt: {cap}");
    assert!(cap.contains("sqr:25"), "bi second xt: {cap}");
}

/// `bi>` stacks both xts' results — p's first, q's second.
#[test]
#[ignore]
fn bi_result_stacks_both() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : double ( n -- m ) 2 * ;
        : square ( n -- m ) dup * ;
        ." [" 5 ' double ' square bi> . . ." ]"   \ ( 25 10 ); prints "[25 10 ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[25 10 ]"), "bi> stacks both: {cap}");
}

/// `bi@` applies the SAME xt to each of two values — left to right.
#[test]
#[ignore]
fn bi_each_applies_one_xt_to_two() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : prn ( n -- ) ." [" . ." ]" ;
        10 20 ' prn bi@
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[10 ][20 ]"), "bi@ order: {cap}");
}

/// `bi*` applies different xts to two values — p to x, q to y.
#[test]
#[ignore]
fn bi_star_routes_each_arg_to_its_xt() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : show-x ( x -- ) ." x=" . ;
        : show-y ( y -- ) ." y=" . ;
        7 8 ' show-x ' show-y bi*
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("x=7"), "bi* p to x: {cap}");
    assert!(cap.contains("y=8"), "bi* q to y: {cap}");
}

/// `tri` applies three xts to the same value, in order.
#[test]
#[ignore]
fn tri_applies_three_to_one() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        : a ( n -- ) ." a:" . ;
        : b ( n -- ) ." b:" . ;
        : c ( n -- ) ." c:" . ;
        9 ' a ' b ' c tri
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("a:9"), "tri first: {cap}");
    assert!(cap.contains("b:9"), "tri second: {cap}");
    assert!(cap.contains("c:9"), "tri third: {cap}");
}
