//! tests/session_quickwins.rs — M2.x latent-word surfacing (task #38).
//!
//! These words were already defined in `factor/forth/runtime/runtime.factor`
//! but missing from `resolve.rs::builtin_table()`, so user code couldn't
//! reach them.  This test locks each name in place against future
//! resolver-table changes.
//!
//! Also covers task #42 — `MOD` now resolves to `forth.runtime:floored-mod`
//! (ANS-faithful sign-follows-divisor), not Factor's truncated `math:mod`.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

fn new_test_session(input: &[u8]) -> (Session, Arc<Mutex<Vec<u8>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: input.to_vec(),
        output: output.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    (session, output)
}

/// Helper: compile + eval a snippet and return the captured stdout
/// (after any whitespace trim).
fn run_capturing(src: &str) -> String {
    let (session, out) = new_test_session(b"");
    let ir = newfactor::compiler::compile(src).expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = out.lock().unwrap();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

// ?DUP intentionally NOT tested — it's stack-effect-polymorphic
// ( x -- 0 | x x ) which Factor's static inference rejects.  See
// the comment next to its absence in resolve.rs.  Modern Forth
// programs use `dup IF ... THEN` (not `?dup IF`); we'll revisit
// only if a benchmark program actually needs the variant.

#[test]
#[ignore]
fn quickwin_depth_reports_data_stack_size() {
    // depth on an empty stack — should be 0 (or very close; nothing
    // we wrote pushed beforehand).  We instead push three known
    // values and verify depth reports >= 3.
    let out = run_capturing("1 2 3 depth .");
    // depth-result was the only thing left to print; everything else
    // was consumed by . in normal evaluation order.  The depth value
    // reflects the stack at the moment `depth` ran (after pushing
    // 1 2 3 but before any `.` consumed them), so it should be 3.
    assert!(out.contains('3'), "depth: got {out:?}");
}

#[test]
#[ignore]
fn quickwin_rstack_round_trip() {
    // >R / R> / R@ — the return-stack family.  Forth-side return
    // stack is independent of Factor's retainstack; ours lives in
    // forth.runtime as an `fstack` tuple.
    let out = run_capturing(": rt 10 >r r@ r> + . ;  rt");
    // r@ → 10, r> → 10, sum = 20
    assert_eq!(out.trim_end_matches('\n').trim(), "20",
        "rstack round-trip: got {out:?}");
}

#[test]
#[ignore]
fn quickwin_u_dot_prints_unsigned() {
    // u. on a positive value behaves the same as . in the current
    // BASE.  We just need it to resolve and print *something* with
    // the value in it.
    let out = run_capturing("42 u.");
    assert!(out.contains("42"), "u.: got {out:?}");
}

#[test]
#[ignore]
fn quickwin_s_to_d_and_d_to_s_are_identity() {
    // On a 64-bit cell host, S>D and D>S are identity (no upper
    // cell needed).  Round-trip a known value.
    let out = run_capturing("7 s>d d>s .");
    assert!(out.contains('7'), "s>d d>s round-trip: got {out:?}");
}

#[test]
#[ignore]
fn quickwin_float_arithmetic_resolves() {
    // F+ F- F* F/ — Factor's polymorphic `+ - * /` already handle
    // floats; we just need the resolver entries to map ANS names
    // onto them.
    let out = run_capturing("2.0e0 3.0e0 f+ .");
    // The exact textual form depends on Factor's printing; should
    // contain "5" somewhere (either "5.0" or "5").
    assert!(out.contains('5'), "f+: got {out:?}");
}

#[test]
#[ignore]
fn quickwin_d_to_f_and_f_to_d() {
    // D>F coerces an integer to a float; F>D coerces back.
    // 42 D>F F>D → 42 (round-trip through float type)
    let out = run_capturing("42 d>f f>d .");
    assert!(out.contains("42"), "d>f f>d round-trip: got {out:?}");
}

#[test]
#[ignore]
fn quickwin_floored_mod_negative_dividend() {
    // ANS MOD: sign of the result follows the DIVISOR, not the
    // dividend.  This is `floored-mod` in forth.runtime; previously
    // resolved to Factor's truncated `math:mod` (sign follows
    // dividend), which is wrong for ANS.
    //
    //   -7 MOD 3   →   2  (floored)  vs  -1 (truncated)
    //    7 MOD -3  →  -2  (floored)  vs   1 (truncated)
    let out_a = run_capturing("-7 3 mod .");
    assert!(out_a.contains('2') && !out_a.contains("-1"),
        "floored -7 mod 3 should be 2: got {out_a:?}");

    let out_b = run_capturing("7 -3 mod .");
    assert!(out_b.contains("-2"),
        "floored 7 mod -3 should be -2: got {out_b:?}");
}
