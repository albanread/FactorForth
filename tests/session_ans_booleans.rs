//! tests/session_ans_booleans.rs — M3.0.2 (#40).
//!
//! ANS Forth specifies that comparison operators leave **-1 for true,
//! 0 for false** on the data stack — not Factor's `t` / `f`.  The
//! Forth 2012 test suite (ttester.fs) checks raw stack values, so
//! `T{ 5 5 = -> -1 }T` and `T{ 5 6 = -> 0 }T` are explicit literal
//! comparisons.  This file mirrors the canonical assertions.
//!
//! Also covers the dual-side of the convention: IF / WHILE / UNTIL
//! must treat ANS 0 as false (Factor natively treats 0 as truthy).
//! NewFactor's emit prepends `math:zero?` before `kernel:if` and
//! swaps branches; this test makes sure the user sees ANS semantics.

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

// ── Comparator values must be ANS -1 / 0 ────────────────────────────────────

#[test]
#[ignore]
fn ans_eq_true_yields_minus_one() {
    // T{ 5 5 = -> -1 }T
    let out = run_capturing("5 5 = .");
    assert_eq!(out, "-1", "5 5 = should be -1, got {out:?}");
}

#[test]
#[ignore]
fn ans_eq_false_yields_zero() {
    // T{ 5 6 = -> 0 }T
    let out = run_capturing("5 6 = .");
    assert_eq!(out, "0", "5 6 = should be 0, got {out:?}");
}

#[test]
#[ignore]
fn ans_neq_true_yields_minus_one() {
    let out = run_capturing("5 6 <> .");
    assert_eq!(out, "-1", "5 6 <> should be -1, got {out:?}");
}

#[test]
#[ignore]
fn ans_lt_true_yields_minus_one() {
    let out = run_capturing("3 5 < .");
    assert_eq!(out, "-1", "3 5 < should be -1, got {out:?}");
}

#[test]
#[ignore]
fn ans_lt_false_yields_zero() {
    let out = run_capturing("5 3 < .");
    assert_eq!(out, "0", "5 3 < should be 0, got {out:?}");
}

#[test]
#[ignore]
fn ans_gt_true_yields_minus_one() {
    let out = run_capturing("5 3 > .");
    assert_eq!(out, "-1", "5 3 > should be -1, got {out:?}");
}

#[test]
#[ignore]
fn ans_zero_eq_true_yields_minus_one() {
    let out = run_capturing("0 0= .");
    assert_eq!(out, "-1", "0 0= should be -1, got {out:?}");
}

#[test]
#[ignore]
fn ans_zero_eq_false_yields_zero() {
    let out = run_capturing("5 0= .");
    assert_eq!(out, "0", "5 0= should be 0, got {out:?}");
}

#[test]
#[ignore]
fn ans_zero_lt_true_yields_minus_one() {
    let out = run_capturing("-3 0< .");
    assert_eq!(out, "-1", "-3 0< should be -1, got {out:?}");
}

#[test]
#[ignore]
fn ans_zero_gt_true_yields_minus_one() {
    let out = run_capturing("3 0> .");
    assert_eq!(out, "-1", "3 0> should be -1, got {out:?}");
}

// ── IF must treat 0 as false, anything else as true ─────────────────────────

#[test]
#[ignore]
fn ans_if_runs_then_branch_for_minus_one() {
    // -1 IF "yes" ELSE "no" THEN — ANS true → "yes"
    let out = run_capturing(r#"-1 if ." yes" else ." no" then"#);
    assert_eq!(out, "yes", "got {out:?}");
}

#[test]
#[ignore]
fn ans_if_runs_else_branch_for_zero() {
    // 0 IF "yes" ELSE "no" THEN — ANS false → "no"
    let out = run_capturing(r#"0 if ." yes" else ." no" then"#);
    assert_eq!(out, "no", "got {out:?}");
}

#[test]
#[ignore]
fn ans_if_treats_nonzero_as_true() {
    // ANS spec: anything non-zero is true.  5 IF should run THEN.
    let out = run_capturing(r#"5 if ." yes" else ." no" then"#);
    assert_eq!(out, "yes", "got {out:?}");
}

#[test]
#[ignore]
fn ans_if_treats_negative_nonzero_as_true() {
    let out = run_capturing(r#"-42 if ." yes" else ." no" then"#);
    assert_eq!(out, "yes", "got {out:?}");
}

#[test]
#[ignore]
fn ans_if_chains_comparators_correctly() {
    // : compare ( a b -- ) = if ." eq" else ." neq" then ;
    // 5 5 compare → "eq"
    // 5 6 compare → "neq"
    let out_eq = run_capturing(r#": cmp = if ." eq" else ." neq" then ;  5 5 cmp"#);
    assert_eq!(out_eq, "eq", "got {out_eq:?}");

    let out_neq = run_capturing(r#": cmp = if ." eq" else ." neq" then ;  5 6 cmp"#);
    assert_eq!(out_neq, "neq", "got {out_neq:?}");
}

#[test]
#[ignore]
fn ans_bare_when_works_for_ans_flag() {
    // -1 IF "yes" THEN  (no ELSE) — should run THEN
    let out = run_capturing(r#"-1 if ." yes" then"#);
    assert_eq!(out, "yes", "got {out:?}");
}

#[test]
#[ignore]
fn ans_bare_when_skips_for_zero() {
    // 0 IF "yes" THEN  — should NOT run THEN
    let out = run_capturing(r#"0 if ." yes" then"#);
    assert_eq!(out, "", "got {out:?}");
}

// ── Bitwise AND/OR/INVERT on ANS flags preserve semantics ───────────────────

#[test]
#[ignore]
fn ans_and_of_flags() {
    // -1 AND -1 == -1  (true AND true = true)
    let out1 = run_capturing("-1 -1 and .");
    assert_eq!(out1, "-1", "got {out1:?}");

    // -1 AND 0  == 0   (true AND false = false)
    let out2 = run_capturing("-1 0 and .");
    assert_eq!(out2, "0", "got {out2:?}");
}

#[test]
#[ignore]
fn ans_invert_of_flag() {
    // ANS INVERT is bitwise NOT.  ~(-1) = 0, ~0 = -1.
    let out1 = run_capturing("-1 invert .");
    assert_eq!(out1, "0", "invert -1 should be 0, got {out1:?}");

    let out2 = run_capturing("0 invert .");
    assert_eq!(out2, "-1", "invert 0 should be -1, got {out2:?}");
}
