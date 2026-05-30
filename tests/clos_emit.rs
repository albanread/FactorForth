//! CLOS object-system emit checks — pure (no VM).
//!
//! These compile a snippet to Factor IR via `compile_in_context` and
//! assert on the emitted text.  No `Session`, so they run under a
//! plain `cargo test` without the embedded VM.  They lock three
//! things:
//!
//!   1. a child class gets accessors for its INHERITED slots, not
//!      just its own;
//!   2. a METHOD: emits a FULL-ARITY specializer list — one class
//!      per input, `object` for unspecialised positions — so
//!      multi-method dispatch keys on every specialised argument,
//!      not just the first;
//!   3. **name-mangling consistency** — every user word name is
//!      emitted with the reserved `z-` prefix, AT THE DEFINITION
//!      and AT EVERY REFERENCE.  This is the safety net for the
//!      uniform-mangling refactor: a mismatch would still compile
//!      (resolve only checks ANS-name existence) and only blow up
//!      at Factor eval — these tests catch it at compile level.

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
    // `x` is declared on the parent.  Class names mangle; slot names
    // and Factor's auto-generated `x>>` accessor stay raw.
    let out = ir("CLASS: point SLOT: x SLOT: y ; \
                  CLASS: cp EXTENDS point SLOT: rgb ;");
    assert!(out.contains(": z-cp>x ( p -- v ) x>> ; inline"),
        "child getter for inherited slot x missing:\n{out}");
    assert!(out.contains(": z-cp>y ( p -- v ) y>> ; inline"),
        "child getter for inherited slot y missing:\n{out}");
    assert!(out.contains(": z-cp>rgb ( p -- v ) rgb>> ; inline"),
        "child getter for own slot rgb missing:\n{out}");
    // Constructor consumes ALL slots, parent-first; the `<cp>` synth
    // name mangles AS A WHOLE (`z-` goes to the FRONT, not between
    // `<` and `cp`), and the `boa` arg is the mangled class word.
    assert!(out.contains(": z-<cp> ( x y rgb -- p ) z-cp boa ; inline"),
        "flattened constructor missing:\n{out}");
}

#[test]
fn method_emits_full_arity_specializer_list() {
    // A two-input method specialised on both args emits `{ a b }` —
    // one class per input position, leftmost = deepest input.  All
    // three user names (the generic and the two class specialisers)
    // mangle.
    let out = ir("CLASS: a ; CLASS: b ; \
                  GENERIC: g ( x y -- z ) \
                  METHOD: g ( p:a q:b -- z ) 2drop 0 ;");
    assert!(out.contains("multi-methods:METHOD: z-g { z-a z-b }"),
        "expected full-arity mangled specializer:\n{out}");
}

#[test]
fn class_gets_membership_predicate() {
    // Every CLASS: exposes `<class>?` ( x -- ? ), backed by Factor's
    // auto-generated tuple predicate.  Predicate, def, and child-class
    // predicate all mangle consistently.
    let out = ir("CLASS: point SLOT: x SLOT: y ; \
                  CLASS: cp EXTENDS point SLOT: rgb ; \
                  : is-pt? ( z -- ? ) point? ; \
                  : is-cp? ( z -- ? ) cp? ;");
    assert!(out.contains(": z-is-pt? ( z -- ? ) z-point? ;"),
        "point? predicate didn't resolve/emit (mangled):\n{out}");
    assert!(out.contains(": z-is-cp? ( z -- ? ) z-cp? ;"),
        "cp? predicate (child) didn't resolve/emit (mangled):\n{out}");
}

#[test]
fn unspecialised_input_position_fills_with_object() {
    // `( c:cat y -- z )` specialises only the FIRST (deepest) input,
    // so the list is `{ cat object }` — dispatch keys on the cat,
    // and the top input is a don't-care.  `cat` mangles; `object`
    // (Factor's universal class) does NOT.
    let out = ir("CLASS: cat ; \
                  GENERIC: g ( x y -- z ) \
                  METHOD: g ( c:cat y -- z ) 2drop 0 ;");
    assert!(out.contains("multi-methods:METHOD: z-g { z-cat object }"),
        "expected `{{ z-cat object }}` (mangled cat, raw object fill):\n{out}");
}

// ── Consistency tests: def-name == ref-name everywhere ──────────────
//
// These are the safety net the uniform-mangling refactor needs.
// A drifted emit site (a definition still raw while references mangle,
// or vice versa) would compile and dump fine — resolve only checks ANS
// existence — and only fail at Factor eval (VM-gated, can't run here).
// Each test compiles a definition AND a reference and asserts both
// emit the same mangled string.

#[test]
fn colon_def_name_matches_caller_side() {
    let out = ir(": dbl ( n -- m ) 2 * ; 5 dbl");
    assert!(out.contains(": z-dbl"),
        "colon def must be mangled:\n{out}");
    assert!(out.contains("5 z-dbl"),
        "caller-side reference must use the same mangled name:\n{out}");
}

#[test]
fn class_accessor_def_matches_caller_side() {
    let out = ir("CLASS: pt SLOT: x ; : getx ( p -- v ) pt>x ;");
    assert!(out.contains(": z-pt>x ( p -- v ) x>> ; inline"),
        "accessor def must be mangled (slot accessor `x>>` stays raw):\n{out}");
    assert!(out.contains("z-pt>x"),
        "caller in getx must reference the same mangled accessor:\n{out}");
    // The caller-side body should be the colon def `z-getx` calling
    // `z-pt>x` — both mangled.
    assert!(out.contains(": z-getx ( p -- v ) z-pt>x ;"),
        "caller def + ref both mangled:\n{out}");
}

#[test]
fn constructor_def_matches_caller_side() {
    let out = ir("CLASS: pt SLOT: x SLOT: y ; : origin ( -- p ) 0 0 <pt> ;");
    assert!(out.contains(": z-<pt> ( x y -- p ) z-pt boa ; inline"),
        "constructor def `z-<pt>` (full synth name mangled) + `z-pt boa`:\n{out}");
    // No-input synth annotation renders as `(  -- p )` — two spaces.
    assert!(out.contains(": z-origin (  -- p ) 0 0 z-<pt> ;"),
        "caller's reference to `<pt>` is `z-<pt>`:\n{out}");
}

#[test]
fn generic_def_matches_caller_side() {
    // Definition writes `multi-methods:GENERIC: z-foo`; a colon-def
    // body calling `foo` references `z-foo` — same mangled token.
    let out = ir("GENERIC: foo ( x -- y ) \
                  METHOD: foo ( x:object -- y ) ; \
                  : use-foo ( x -- y ) foo ;");
    assert!(out.contains("multi-methods:GENERIC: z-foo"),
        "generic def must be mangled:\n{out}");
    assert!(out.contains(": z-use-foo ( x -- y ) z-foo ;"),
        "caller of foo must reference z-foo:\n{out}");
}

#[test]
fn predicate_call_matches_tuple_auto_predicate() {
    // Factor auto-generates `<class>?` for a `TUPLE: <class>`.
    // If we emit `TUPLE: z-pt`, the auto predicate is `z-pt?`, and
    // a Forth-side `pt?` call must resolve to `z-pt?`.
    let out = ir("CLASS: pt ; : is-pt? ( x -- ? ) pt? ;");
    assert!(out.contains("TUPLE: z-pt"),
        "TUPLE must be mangled so its auto predicate becomes `z-pt?`:\n{out}");
    assert!(out.contains(": z-is-pt? ( x -- ? ) z-pt? ;"),
        "predicate call `pt?` must mangle to `z-pt?`:\n{out}");
}

#[test]
fn variable_def_matches_reference() {
    let out = ir("VARIABLE counter : bump counter @ 1+ counter ! ;");
    // This snippet hits the WIDE-variable path (reader returns the
    // addr, which @/! then read/write).  Both the reader def and
    // every caller-side reference must use the same mangled name.
    // The hidden storage handle `nf-var-counter` stays raw — it's
    // an internal symbol, not user-callable.
    assert!(out.contains(": z-counter ( -- addr ) nf-var-counter get-global ; inline"),
        "wide-variable reader def must be mangled:\n{out}");
    assert!(out.contains("z-counter forth.runtime:@"),
        "@ reference must mangle the variable:\n{out}");
    assert!(out.contains("z-counter forth.runtime:nf-!"),
        "! reference must mangle the variable:\n{out}");
}

#[test]
fn constant_def_matches_reference() {
    let out = ir("42 CONSTANT answer : ask ( -- n ) answer ;");
    assert!(out.contains("CONSTANT: z-answer 42"),
        "CONSTANT def must be mangled:\n{out}");
    // No-input synth annotation renders as `(  -- n )` — two spaces.
    assert!(out.contains(": z-ask (  -- n ) z-answer ;"),
        "caller-side reference must use the same mangled name:\n{out}");
}
