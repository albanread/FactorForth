//! CLOS object-system emit checks — pure (no VM).
//!
//! These compile a snippet to Factor IR via `compile_in_context` and
//! assert on the emitted text.  No `Session`, so they run under a
//! plain `cargo test` without the embedded VM.  They lock two
//! behaviours that user docs (classes.md) describe and that earlier
//! "Sprint 1" caveats wrongly denied:
//!
//!   1. a child class gets accessors for its INHERITED slots, not
//!      just its own;
//!   2. a METHOD: emits a FULL-ARITY specializer list — one class per
//!      input, `object` for unspecialised positions — so multi-method
//!      dispatch keys on every specialised argument, not just the first.

#![cfg(target_os = "windows")]

use newfactor::compiler::{compile_in_context, CompileContext};

/// Compile `src` to IR in a fresh context, panicking on a compile error.
fn ir(src: &str) -> String {
    let mut ctx = CompileContext::new();
    compile_in_context(src, &mut ctx)
        .unwrap_or_else(|e| panic!("compile failed: {e}\n--- source ---\n{src}"))
}

#[test]
fn child_class_gets_accessors_for_inherited_slots() {
    // `cp` inherits x, y from `point` and adds rgb.  All three slots
    // get a child-namespaced getter, so `cp>x` is valid even though
    // `x` is declared on the parent.
    let out = ir("CLASS: point SLOT: x SLOT: y ; \
                  CLASS: cp EXTENDS point SLOT: rgb ;");
    assert!(out.contains(": cp>x ( p -- v ) x>> ; inline"),
        "child getter for inherited slot x missing:\n{out}");
    assert!(out.contains(": cp>y ( p -- v ) y>> ; inline"),
        "child getter for inherited slot y missing:\n{out}");
    assert!(out.contains(": cp>rgb ( p -- v ) rgb>> ; inline"),
        "child getter for own slot rgb missing:\n{out}");
    // The constructor consumes ALL slots, parent-first.
    assert!(out.contains(": <cp> ( x y rgb -- p ) cp boa ; inline"),
        "flattened constructor missing:\n{out}");
}

#[test]
fn method_emits_full_arity_specializer_list() {
    // A two-input method specialised on both args emits `{ a b }` —
    // one class per input position, leftmost = deepest input.
    let out = ir("CLASS: a ; CLASS: b ; \
                  GENERIC: g ( x y -- z ) \
                  METHOD: g ( p:a q:b -- z ) 2drop 0 ;");
    assert!(out.contains("multi-methods:METHOD: g { a b }"),
        "expected full-arity specializer `{{ a b }}`:\n{out}");
}

#[test]
fn class_gets_membership_predicate() {
    // Every CLASS: exposes `<class>?` ( x -- ? ), backed by Factor's
    // auto-generated tuple predicate.  It resolves at a call site,
    // sizes as ( 1 -- 1 ), and emits as the bare predicate word — for
    // a child class too.
    let out = ir("CLASS: point SLOT: x SLOT: y ; \
                  CLASS: cp EXTENDS point SLOT: rgb ; \
                  : is-pt? ( z -- ? ) point? ; \
                  : is-cp? ( z -- ? ) cp? ;");
    assert!(out.contains(": is-pt? ( z -- ? ) point? ;"),
        "point? predicate didn't resolve/emit:\n{out}");
    assert!(out.contains(": is-cp? ( z -- ? ) cp? ;"),
        "cp? predicate (child class) didn't resolve/emit:\n{out}");
}

#[test]
fn unspecialised_input_position_fills_with_object() {
    // `( c:cat y -- z )` specialises only the FIRST (deepest) input,
    // so the list is `{ cat object }` — dispatch keys on the cat,
    // and the top input is a don't-care.  (The old, buggy behaviour
    // emitted just `{ cat }`, which multi-methods aligned to the TOP
    // input — dispatching on the wrong argument.)
    let out = ir("CLASS: cat ; \
                  GENERIC: g ( x y -- z ) \
                  METHOD: g ( c:cat y -- z ) 2drop 0 ;");
    assert!(out.contains("multi-methods:METHOD: g { cat object }"),
        "expected `{{ cat object }}` (object fill for the bare input):\n{out}");
}
