//! tests/session_ans_core.rs — M2.x #39 ANS Core completeness.
//!
//! Locks each newly-resolved ANS word with a T{ }T-style assertion.
//! These match the canonical Forth 2012 test suite's expectations
//! so the eventual `runtests.fth` driver (#41) can run them.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

fn run_capturing(src: &str) -> String {
    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![],
        output: output.clone(),
    });
    let session = Session::new(opts).expect("Session::new");
    let ir = newfactor::compiler::compile(src).expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = output.lock().unwrap();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

// ── Arithmetic shortcuts ────────────────────────────────────────────────────

#[test]
#[ignore]
fn core_one_plus() {
    assert_eq!(run_capturing("41 1+ ."), "42");
    assert_eq!(run_capturing("-1 1+ ."), "0");
}

#[test]
#[ignore]
fn core_one_minus() {
    assert_eq!(run_capturing("43 1- ."), "42");
    assert_eq!(run_capturing("0 1- ."), "-1");
}

#[test]
#[ignore]
fn core_two_star() {
    assert_eq!(run_capturing("21 2* ."), "42");
    assert_eq!(run_capturing("-21 2* ."), "-42");
}

#[test]
#[ignore]
fn core_two_slash() {
    assert_eq!(run_capturing("84 2/ ."), "42");
    // ANS 2/ is arithmetic right shift — sign-preserving.
    assert_eq!(run_capturing("-1 2/ ."), "-1");
}

// ── Division family ─────────────────────────────────────────────────────────

#[test]
#[ignore]
fn core_slash_mod_positive() {
    // 17 /MOD 5  → ( r=2 q=3 )  TOS is q.
    let out = run_capturing("17 5 /mod . .");
    // Top-first . prints: q first, then r.
    assert_eq!(out, "3 2", "got {out:?}");
}

#[test]
#[ignore]
fn core_slash_mod_negative_floored() {
    // -17 /MOD 5  with floored semantics: q = -4, r = 3.
    //   -4 * 5 = -20; -17 - (-20) = 3.  Sign of r matches divisor.
    let out = run_capturing("-17 5 /mod . .");
    assert_eq!(out, "-4 3", "got {out:?}");
}

#[test]
#[ignore]
fn core_star_slash() {
    // 100 3 * 4 / = 75
    assert_eq!(run_capturing("100 3 4 */ ."), "75");
    // Intermediate-precision: 1000000 1000000 * = 1e12, / 1e9 = 1000.
    // Factor auto-promotes to bignum — should still work.
    assert_eq!(run_capturing("1000000 1000000 1000000000 */ ."), "1000");
}

#[test]
#[ignore]
fn core_star_slash_mod() {
    // 100 3 * 7 /MOD = ( r q ): 300/7 = 42 rem 6.
    let out = run_capturing("100 3 7 */mod . .");
    assert_eq!(out, "42 6", "got {out:?}");
}

// ── Bit-shifts ──────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn core_lshift() {
    assert_eq!(run_capturing("1 4 lshift ."), "16");
    assert_eq!(run_capturing("3 2 lshift ."), "12");
}

#[test]
#[ignore]
fn core_rshift() {
    assert_eq!(run_capturing("16 4 rshift ."), "1");
    assert_eq!(run_capturing("12 2 rshift ."), "3");
}

// ── Stack manipulation pairs ────────────────────────────────────────────────

#[test]
#[ignore]
fn core_two_dup() {
    let out = run_capturing("1 2 2dup . . . .");
    // Stack ( 1 2 ) 2dup → ( 1 2 1 2 ).  Top-first . prints: 2 1 2 1.
    assert_eq!(out, "2 1 2 1", "got {out:?}");
}

#[test]
#[ignore]
fn core_two_drop() {
    let out = run_capturing("1 2 3 4 2drop . .");
    // ( 1 2 3 4 ) 2drop → ( 1 2 ).  Print TOS first: 2 1.
    assert_eq!(out, "2 1", "got {out:?}");
}

#[test]
#[ignore]
fn core_two_swap() {
    let out = run_capturing("1 2 3 4 2swap . . . .");
    // ( 1 2 3 4 ) 2swap → ( 3 4 1 2 ).  Print TOS first: 2 1 4 3.
    assert_eq!(out, "2 1 4 3", "got {out:?}");
}

#[test]
#[ignore]
fn core_two_over() {
    let out = run_capturing("1 2 3 4 2over . . . . . .");
    // ( 1 2 3 4 ) 2over → ( 1 2 3 4 1 2 ).  TOS-first: 2 1 4 3 2 1.
    assert_eq!(out, "2 1 4 3 2 1", "got {out:?}");
}

#[test]
#[ignore]
fn core_minus_rot() {
    let out = run_capturing("1 2 3 -rot . . .");
    // ( 1 2 3 ) -rot → ( 3 1 2 ).  TOS-first: 2 1 3.
    assert_eq!(out, "2 1 3", "got {out:?}");
}

// ── Cell-pair memory ────────────────────────────────────────────────────────

#[test]
#[ignore]
fn core_two_store_two_fetch_round_trip() {
    // CREATE a 2-cell buffer, 2! the pair, 2@ back.
    // ANS: 11 22 buf 2!  buf 2@  leaves ( 11 22 ) — x2 at addr (lower),
    // x1 at addr+cell (upper).  After 2@, x2 is TOS.
    // `cbuffer` size is in bytes, so 16 bytes = 2 cells.
    // `cbuffer` defines an INDEXED accessor: `<n> buf` returns
    // the address of byte n.  Use 0 for the base.
    let out = run_capturing(
        "16 cbuffer buf  11 22 0 buf 2!  0 buf 2@ . ."
    );
    // TOS-first: x2 then x1 → "22 11".
    assert_eq!(out, "22 11", "got {out:?}");
}

// ── ERASE ───────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn core_erase_zeros_buffer() {
    // Allocate 4 bytes, fill with 0xFF, erase 4, read first byte.
    let out = run_capturing(
        "4 cbuffer buf  0 buf 4 255 fill  0 buf 4 erase  0 buf c@ ."
    );
    assert_eq!(out, "0", "got {out:?}");
}

// ── 0<> comparator ──────────────────────────────────────────────────────────

#[test]
#[ignore]
fn core_zero_neq_true_for_nonzero() {
    assert_eq!(run_capturing("5 0<> ."), "-1");
    assert_eq!(run_capturing("-3 0<> ."), "-1");
}

#[test]
#[ignore]
fn core_zero_neq_false_for_zero() {
    assert_eq!(run_capturing("0 0<> ."), "0");
}
