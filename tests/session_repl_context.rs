//! tests/session_repl_context.rs — REPL-style cross-eval context.
//!
//! Proves that a user word defined in one eval is callable in a
//! subsequent eval on the same Session.  This is the bug the user
//! hit in the GUI's first launch:
//!
//!   > : square dup * ;
//!   > 5 square .
//!   ⚠ compile: unknown word `square`     ← BAD
//!
//! With `CompileContext` threaded across compiles, `square` should
//! resolve on the second eval.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

fn make_session() -> (Session, Arc<Mutex<Vec<u8>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![],
        output: output.clone(),
    });
    let s = Session::new(opts).expect("Session::new");
    (s, output)
}

#[test]
#[ignore]
fn user_def_from_first_eval_resolves_in_second() {
    let (s, out) = make_session();
    let mut ctx = CompileContext::new();

    // Eval 1: define a word.
    let ir1 = compile_in_context(": square dup * ;", &mut ctx)
        .expect("compile def");
    s.eval(&ir1).expect("eval def");
    out.lock().unwrap().clear();

    // Ctx should now know about `square`.
    assert!(ctx.user_words.contains_key("square"),
        "after defining `square`, ctx.user_words should include it; got {:?}",
        ctx.user_words.keys().collect::<Vec<_>>());

    // Eval 2: call it.  This is the failing case before the fix.
    let ir2 = compile_in_context("5 square .", &mut ctx)
        .expect("compile call — this was the GUI bug");
    s.eval(&ir2).expect("eval call");

    let captured = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    assert!(captured.contains("25"),
        "expected 5 square => 25 in output, got {captured:?}");
}

// ── Variables across evals: known limitation ────────────────────────────────
//
// Variables are subject to per-compile escape analysis that
// hoists them to "narrow" (Factor SYMBOL: + get-global) when all
// their uses are direct @/!/+!/c@/c! within one compile.  The
// narrow form emits a SYMBOL: declaration in eval 1, but eval 2's
// reference to the variable name doesn't know it's a symbol and
// emits a plain word reference — which Factor then evaluates as
// "push the symbol object" rather than "fetch the cell value".
//
// Fix is a separate task (#52): in compile_in_context mode, force
// all variables to wide (nf-addr-backed) so the cross-eval shape
// is consistent.  Within a single compile, narrow is still the
// right call.
//
// Until then, interactive variables work within ONE eval, not across.

#[test]
#[ignore]
fn constant_persists_across_evals() {
    let (s, out) = make_session();
    let mut ctx = CompileContext::new();

    let ir1 = compile_in_context("7 constant lucky", &mut ctx).expect("compile const");
    s.eval(&ir1).expect("eval const");
    out.lock().unwrap().clear();

    let ir2 = compile_in_context("lucky lucky + .", &mut ctx).expect("compile use");
    s.eval(&ir2).expect("eval use");

    let captured = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    assert!(captured.contains("14"), "got {captured:?}");
}

#[test]
#[ignore]
fn three_evals_word_calls_prior_word() {
    // Eval 1: : a 10 ;
    // Eval 2: : b a a + ;
    // Eval 3: b .   →   20
    let (s, out) = make_session();
    let mut ctx = CompileContext::new();

    let ir1 = compile_in_context(": a 10 ;", &mut ctx).expect("compile a");
    s.eval(&ir1).expect("eval a");

    let ir2 = compile_in_context(": b a a + ;", &mut ctx).expect("compile b — uses a");
    s.eval(&ir2).expect("eval b");
    out.lock().unwrap().clear();

    let ir3 = compile_in_context("b .", &mut ctx).expect("compile use");
    s.eval(&ir3).expect("eval use");

    let captured = String::from_utf8_lossy(&out.lock().unwrap()).into_owned();
    assert!(captured.contains("20"), "got {captured:?}");
}

#[test]
#[ignore]
fn fresh_context_does_not_see_old_defs() {
    // Sanity check: a fresh CompileContext does NOT inherit
    // user_words from a prior context.  Different sessions are
    // independent.
    let (s, _out) = make_session();
    let mut ctx1 = CompileContext::new();

    let ir1 = compile_in_context(": private 99 ;", &mut ctx1).expect("compile");
    s.eval(&ir1).expect("eval");

    let mut ctx2 = CompileContext::new();
    let err = compile_in_context("private .", &mut ctx2);
    assert!(err.is_err(),
        "fresh context shouldn't resolve a prior context's words; got {err:?}");
}
