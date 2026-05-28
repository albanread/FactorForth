//! Method combinations — `:before` and `:after` auxiliary methods.
//!
//! In CLOS-style dispatch:
//!   - Before methods run from most-specific-first BEFORE the primary;
//!     their return values are discarded.  Used for invariant checks,
//!     logging, instrumentation.
//!   - After methods run from least-specific-first AFTER the primary;
//!     same return-value discard.  Used for post-commit notifications,
//!     cleanup, audit trails.
//!   - The primary computes the actual value.  All aux methods see
//!     the same inputs as the primary.
//!
//! Same-eval requirement (sprint 1 of aux methods): the generic and
//! all its aux methods must live in one compile.  Cross-eval aux
//! requires persistent shadow-generic state in CompileContext, a
//! follow-up.

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

/// Sanity check: a generic with neither before nor after still
/// works.  Confirms the aux-detection logic doesn't regress the
/// no-aux fast path.
#[test]
#[ignore]
fn no_aux_still_works() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: dog ;
        GENERIC: speak ( a -- )
        METHOD: speak ( a:dog -- )  drop ." woof" cr ;
        <dog> speak
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    assert!(cap.contains("woof"), "got: {cap}");
}

/// Before-method runs first.  Output order proves the sequencing:
/// the before-method's `." check"` comes before the primary's
/// `." action"`.
#[test]
#[ignore]
fn before_runs_before_primary() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: account ;
        GENERIC: withdraw ( a -- )

        METHOD-BEFORE: withdraw ( a:account -- )  drop ." check " ;
        METHOD: withdraw ( a:account -- )  drop ." action " ;

        <account> withdraw
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("check action"), "expected 'check action', got: {cap}");
}

/// After-method runs after primary; its return value is discarded.
#[test]
#[ignore]
fn after_runs_after_primary() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: account ;
        GENERIC: withdraw ( a -- )

        METHOD: withdraw ( a:account -- )  drop ." action " ;
        METHOD-AFTER: withdraw ( a:account -- )  drop ." notify " ;

        <account> withdraw
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("action notify"), "expected 'action notify', got: {cap}");
}

/// Before AND after together — sequencing is `before primary after`.
#[test]
#[ignore]
fn before_and_after_together() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: txn ;
        GENERIC: commit ( a -- )

        METHOD-BEFORE: commit ( a:txn -- )  drop ." [ " ;
        METHOD: commit ( a:txn -- )  drop ." body " ;
        METHOD-AFTER:  commit ( a:txn -- )  drop ." ] " ;

        <txn> commit
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[ body ]"), "expected '[ body ]', got: {cap}");
}

/// Primary's return value passes through the wrapper.  Before and
/// after don't disturb it.  (Generic name avoids `value` since
/// that collides with the VALUE defining word at parse time.)
#[test]
#[ignore]
fn primary_return_value_preserved() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: box ;
        GENERIC: contents ( a -- v )

        METHOD-BEFORE: contents ( a:box -- )  drop ." entering " ;
        METHOD: contents ( a:box -- v )  drop 42 ;
        METHOD-AFTER:  contents ( a:box -- )  drop ." leaving " ;

        <box> contents .
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("entering"));
    assert!(cap.contains("leaving"));
    // 42 from primary, printed by the trailing `.`
    assert!(cap.contains("42"), "primary's return should print: {cap}");
}

/// Multi-input generic: before/after both see the same inputs as
/// the primary.  Stack-effect arity 2 in/1 out exercises the
/// wrapper's `2dup`-style locals handling.
#[test]
#[ignore]
fn aux_methods_on_two_input_generic() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: rect ;

        GENERIC: combine ( a b -- v )

        METHOD-BEFORE: combine ( a:rect b:rect -- )  2drop ." before " ;
        METHOD: combine ( a:rect b:rect -- v )  2drop 99 ;
        METHOD-AFTER:  combine ( a:rect b:rect -- )  2drop ." after " ;

        <rect> <rect> combine .
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("before"));
    assert!(cap.contains("after"));
    assert!(cap.contains("99"));
}

/// Before-method on a non-matching class is a no-op (the
/// `{ object }` default drops without doing anything).  Confirms
/// the default-method fallback is in place.
#[test]
#[ignore]
fn before_not_matching_is_noop() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        CLASS: cat ;
        CLASS: dog ;

        GENERIC: speak ( a -- )

        \ before only matches a cat:
        METHOD-BEFORE: speak ( a:cat -- )  drop ." purr " ;

        METHOD: speak ( a:cat -- )  drop ." meow " ;
        METHOD: speak ( a:dog -- )  drop ." woof " ;

        <cat> speak
        <dog> speak
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // cat: purr (before) then meow (primary)
    // dog: just woof (no before matches, falls through to no-op
    // default; primary still runs)
    assert!(cap.contains("purr meow"), "expected 'purr meow' from cat: {cap}");
    assert!(cap.contains("woof"), "expected 'woof' from dog: {cap}");
    // The dog branch should NOT have any "purr" in it.  Check that
    // "purr" only appears once (from the cat branch).
    assert_eq!(cap.matches("purr").count(), 1,
        "purr should only fire for the cat: {cap}");
}
