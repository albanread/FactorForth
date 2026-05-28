//! SEE — compile-time word introspection.
//!
//! `SEE name` is a parsing word (like `'` and `TO`): it consumes the
//! next token and, at compile time, emits a report of what the
//! compiler knows about that word — kind, stack effect, origin, and
//! the retained original ANS source for user definitions.

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

/// SEE on a colon definition shows kind, effect, and the original
/// source text.
#[test]
#[ignore]
fn see_colon_definition_shows_source() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        : square ( n -- n2 ) dup * ;
        see square
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("square"), "name expected: {cap}");
    assert!(cap.contains("colon definition"), "kind expected: {cap}");
    assert!(cap.contains("( n -- n2 )"), "effect expected: {cap}");
    // the retained source
    assert!(cap.contains("dup *"), "source body expected: {cap}");
}

/// SEE on a builtin shows it's a builtin, its arity, and the Factor
/// word it maps to.
#[test]
#[ignore]
fn see_builtin_shows_factor_target() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        see dup
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("dup"), "name expected: {cap}");
    assert!(cap.contains("builtin"), "builtin tag expected: {cap}");
    // dup maps to Factor's `dup` (a bare kernel word in the default
    // search path, so our Target carries no vocab prefix).
    assert!(cap.contains("factor: dup"), "factor target expected: {cap}");
    // arity: dup is ( a -- a a ) — one in, two out
    assert!(cap.contains("( a -- "), "arity expected: {cap}");
}

/// SEE on a constant shows the kind and the value.
#[test]
#[ignore]
fn see_constant_shows_value() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        42 CONSTANT answer
        see answer
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("answer"), "name expected: {cap}");
    assert!(cap.contains("constant"), "kind expected: {cap}");
    assert!(cap.contains("42"), "value expected: {cap}");
}

/// SEE on a class shows the slot list.
#[test]
#[ignore]
fn see_class_shows_slots() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point SLOT: x SLOT: y ;
        see point
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("point"), "name expected: {cap}");
    assert!(cap.contains("class"), "kind expected: {cap}");
    assert!(cap.contains('x') && cap.contains('y'), "slots expected: {cap}");
}

/// SEE works across evals: define in one eval, SEE in the next.
/// (The doc store persists in CompileContext like other metadata.)
#[test]
#[ignore]
fn see_works_cross_eval() {
    let (s, out, mut ctx) = fresh();
    // Eval 1: define.
    let ir1 = compile_in_context(": triple ( n -- n3 ) 3 * ;", &mut ctx).expect("compile1");
    s.eval(&ir1).expect("eval1");
    // Eval 2: SEE it.
    let ir2 = compile_in_context("see triple", &mut ctx).expect("compile2");
    eprintln!("IR2:\n{ir2}");
    s.eval(&ir2).expect("eval2");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("triple"), "name expected: {cap}");
    assert!(cap.contains("3 *"), "retained source expected cross-eval: {cap}");
}

/// SEE on an unknown word reports it gracefully rather than failing
/// to compile.
#[test]
#[ignore]
fn see_unknown_word_is_graceful() {
    let (s, out, mut ctx) = fresh();
    let ir = compile_in_context("see nonesuch", &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("nonesuch"), "name echoed: {cap}");
    assert!(cap.to_lowercase().contains("unknown"), "unknown note expected: {cap}");
}
