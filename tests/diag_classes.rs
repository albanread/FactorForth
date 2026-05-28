//! Runtime tests for the object system MVP (sprint 1).
//! Exercises CLASS: / SLOT: / GENERIC: / METHOD: end-to-end through
//! the embedded Factor VM.

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

/// Bare class + constructor: define a 2-slot class, build an instance,
/// pull each slot back out via the auto-generated getter.
#[test]
#[ignore]
fn class_construct_and_access() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point
            SLOT: x
            SLOT: y
        ;
        3 4 <point>            \ build a point on the stack
        dup point>x .          \ 3
        point>y .              \ 4
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("3"), "x slot: {cap}");
    assert!(cap.contains("4"), "y slot: {cap}");
}

/// Chainable setter `slot>>class ( p v -- p )` — returns the object,
/// composes for fluent transformation.
#[test]
#[ignore]
fn class_setter_chains() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point  SLOT: x  SLOT: y  ;
        3 4 <point>
        99 x>>point          \ ( p -- p' ) — chainable
        dup point>x .        \ 99
        point>y .            \ 4
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("99"), "setter: {cap}");
    assert!(cap.contains("4"), "untouched y: {cap}");
}

/// ANS-flavoured store `class.slot!  ( v p -- )` — drops the object,
/// matches the `42 var !` muscle memory.  Both setters mutate the
/// same underlying slot; they differ only in stack discipline.
#[test]
#[ignore]
fn class_ans_store_drops_object() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point  SLOT: x  SLOT: y  ;
        3 4 <point>           \ ( -- p )
        dup
        99 swap point.x!      \ ( v p -- ) drops the dup; original retained below
        dup point>x .         \ 99 — mutation took
        point>y .             \ 4
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("99"), "store via .slot!: {cap}");
    assert!(cap.contains("4"), "untouched y: {cap}");
}

/// Polymorphism: same slot accepts int, then float, then string —
/// either setter form works because Factor's tuple slots are
/// tag-erased.  Builds one instance, mutates the same slot three
/// times with three different types, reads it back each time.
#[test]
#[ignore]
fn class_slots_are_polymorphic() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: holder  SLOT: x  ;
        0 <holder>            \ ( -- h )  x=0 (int)
        dup 42 swap holder.x!
        dup holder>x .        \ 42

        dup 3.14e swap holder.x!
        dup holder>x drop ." float-ok " cr

        dup s$" hi" swap holder.x!
        holder>x $.           \ hi
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("42"), "int phase: {cap}");
    assert!(cap.contains("float-ok"), "float phase: {cap}");
    assert!(cap.contains("hi"), "string phase: {cap}");
}

/// Namespacing: two classes with same-named slots get distinct
/// accessor names, so `x>>point` and `x>>vector3` are different
/// words.  Setters' class-qualified naming prevents accidental
/// cross-type writes.
#[test]
#[ignore]
fn class_accessors_are_namespaced() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point    SLOT: x  SLOT: y  ;
        CLASS: vector3  SLOT: x  SLOT: y  SLOT: z  ;
        1 2 <point>           \ ( -- p )
        10 20 30 <vector3>    \ ( -- p v )
        dup vector3>x .       \ 10
        drop
        dup point>x .         \ 1 — confirms the two classes don't interfere
        drop
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("10"), "vector3>x: {cap}");
    assert!(cap.contains("1 "), "point>x: {cap}");
}

/// GENERIC: + METHOD: on a class — the textbook distance example.
/// Sprint 1: single dispatch on the FIRST arg only.
#[test]
#[ignore]
fn generic_and_method_dispatch() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point  SLOT: x  SLOT: y  ;
        GENERIC: describe ( p -- )
        METHOD: describe ( p:point -- )
            dup point>x .
            point>y .
        ;
        3 4 <point> describe
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("3"), "describe x: {cap}");
    assert!(cap.contains("4"), "describe y: {cap}");
}
