//! Shape hierarchy — the textbook OO probe, applied to Factor4th
//! to surface real-world rough edges in the sprint-1 class system.
//!
//! Tests written as if I'm a user, not as if I'm the implementor.
//! Each test fails the way a user would experience it; the fix
//! reveals something to improve.

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

/// The classic CLOS demo: a base class with no slots, two subclasses
/// with different slots, a generic that does something different
/// based on which concrete class shows up.
#[test]
#[ignore]
fn shape_hierarchy_area_dispatch() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: shape ;
        CLASS: circle EXTENDS shape  SLOT: r  ;
        CLASS: square EXTENDS shape  SLOT: side  ;

        GENERIC: area ( s -- a )
        METHOD: area ( c:circle -- a )
            circle>r dup f* 3.14159e f* ;
        METHOD: area ( s:square -- a )
            square>side dup f* ;

        5.0e <circle> area .
        3.0e <square> area .
    "#;
    let ir = compile_in_context(src, &mut ctx);
    match &ir {
        Ok(ir) => eprintln!("IR:\n{ir}"),
        Err(e) => { eprintln!("compile err: {e}"); panic!("compile: {e}"); }
    }
    let ir = ir.unwrap();
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // Circle area: π·25 ≈ 78.54 — accept any "78." prefix.
    assert!(cap.contains("78."), "circle area ~78.54: {cap}");
    // Square area: 9.0
    assert!(cap.contains("9."), "square area = 9.0: {cap}");
}

/// Inheritance: a child class instance should still respond to the
/// parent's accessors.  Today this fails because sprint 1 doesn't
/// flatten inherited slots into the child's accessor set.
#[test]
#[ignore]
fn child_class_can_read_parent_slot() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: point  SLOT: x  SLOT: y  ;
        CLASS: colored-point EXTENDS point  SLOT: rgb  ;
        \ Constructor order: parent slots first.
        3 4 255 <colored-point>
        \ Access parent's slot using PARENT's getter — Factor tuple
        \ inheritance preserves the slot access, but our auto-
        \ generated accessor word is namespaced to the parent.
        dup point>x .          \ 3
        dup point>y .          \ 4
        colored-point>rgb .    \ 255
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
    assert!(cap.contains("3"), "x: {cap}");
    assert!(cap.contains("4"), "y: {cap}");
    assert!(cap.contains("255"), "rgb: {cap}");
}
