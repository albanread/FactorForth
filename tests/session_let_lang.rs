//! tests/session_let_lang.rs — M2.x+ #44 step A: LET DSL.
//!
//! The infix-algebraic sub-language `LET (in) -> (out) = expr END`
//! lowered to Factor's `[| in | body ] call( in -- out )` form.
//! This file covers step A: straight-line LET with WHERE clauses
//! and intrinsics; step C (libm dispatch) lands in a follow-up.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

/// Compile + eval one snippet, return trimmed captured output.
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

/// Compile + eval, then push the resulting stack (floats) as space-
/// separated `.`-printed numbers.  Same pattern the test_runner
/// uses for capturing T{ }T results.
fn run_dump_stack(src: &str) -> String {
    let full = format!("{src}\nbegin depth 0> while . repeat");
    run_capturing(&full)
}

// ── Smoke ───────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn let_identity() {
    let out = run_dump_stack("42.0 LET (x) -> (y) = x END");
    assert_eq!(out, "42.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_arithmetic() {
    // (5*5) + 1 = 26
    let out = run_dump_stack("5.0 LET (x) -> (y) = x*x + 1 END");
    assert_eq!(out, "26.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_unary_minus() {
    let out = run_dump_stack("7.5 LET (x) -> (y) = -x END");
    assert_eq!(out, "-7.5", "got {out:?}");
}

#[test]
#[ignore]
fn let_multi_input() {
    // 10 / 3 = 3.333...
    let out = run_dump_stack("100.0 3.0 4.0 LET (a, b, c) -> (d) = a*b/c END");
    // 100*3/4 = 75
    assert_eq!(out, "75.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_multi_output() {
    // diff & sum.  Stack convention: outputs in declared order,
    // result[0] deepest, result[last] on top.
    // (a b) -> (diff, sum) = a-b, a+b
    // Stack at entry: ( a=10  b=3 )  (b on top)
    // After LET: ( diff=7  sum=13 )  (sum on top)
    // Dump tail prints TOS first: "13.0 7.0"
    let out = run_dump_stack(
        "10.0 3.0 LET (a, b) -> (diff, sum) = a - b, a + b END"
    );
    assert_eq!(out, "13.0 7.0", "got {out:?}");
}

// ── WHERE clauses ───────────────────────────────────────────────────────────

#[test]
#[ignore]
fn let_with_where() {
    let out = run_dump_stack(
        "3.0 LET (r) -> (a) = b * r WHERE b = r + 1 END"
    );
    // b = 4, a = 4*3 = 12
    assert_eq!(out, "12.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_with_multiple_where() {
    // Mandelbrot iteration shape.
    let out = run_dump_stack(
        "1.0 1.0 1.0 1.0 \
         LET (z_re, z_im, x, y) -> (z_next_re, z_next_im, mag) = \
            re, im, rmag \
            WHERE re   = z_re * z_re - z_im * z_im + x \
            WHERE im   = 2 * z_re * z_im + y \
            WHERE rmag = re * re + im * im \
         END"
    );
    // re  = 1*1 - 1*1 + 1 = 1
    // im  = 2*1*1 + 1     = 3
    // rmag = 1*1 + 3*3    = 10
    // Stack (TOS first): mag=10, im=3, re=1
    assert_eq!(out, "10.0 3.0 1.0", "got {out:?}");
}

// ── Intrinsics ──────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn let_sqrt() {
    let out = run_dump_stack("16.0 LET (x) -> (y) = sqrt(x) END");
    assert_eq!(out, "4.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_abs() {
    let out = run_dump_stack("-3.5 LET (x) -> (y) = abs(x) END");
    assert_eq!(out, "3.5", "got {out:?}");
}

#[test]
#[ignore]
fn let_min_max() {
    // 7 3 min = 3
    let out_min = run_dump_stack("7.0 3.0 LET (a, b) -> (m) = min(a, b) END");
    assert_eq!(out_min, "3.0", "got {out_min:?}");

    let out_max = run_dump_stack("7.0 3.0 LET (a, b) -> (m) = max(a, b) END");
    assert_eq!(out_max, "7.0", "got {out_max:?}");
}

#[test]
#[ignore]
fn let_floor_ceil_trunc() {
    let f = run_dump_stack("2.7 LET (x) -> (y) = floor(x) END");
    assert_eq!(f, "2.0", "floor: got {f:?}");

    let c = run_dump_stack("2.3 LET (x) -> (y) = ceil(x) END");
    assert_eq!(c, "3.0", "ceil: got {c:?}");

    let t = run_dump_stack("-2.7 LET (x) -> (y) = trunc(x) END");
    assert_eq!(t, "-2.0", "trunc: got {t:?}");
}

// ── Composed expressions ────────────────────────────────────────────────────

#[test]
#[ignore]
fn let_precedence() {
    // 1 + 2*3 = 7 (not 9 — multiplication binds tighter)
    let out = run_dump_stack("LET () -> (y) = 1 + 2 * 3 END");
    assert_eq!(out, "7.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_parens_override_precedence() {
    // (1+2)*3 = 9
    let out = run_dump_stack("LET () -> (y) = (1 + 2) * 3 END");
    assert_eq!(out, "9.0", "got {out:?}");
}

#[test]
#[ignore]
fn let_nested_call() {
    // sqrt(sin(x)*sin(x) + cos(x)*cos(x)) = 1.0  (the Pythagorean
    // identity).  Tests function calls + arithmetic together.
    let out = run_dump_stack(
        "1.5 LET (x) -> (y) = sqrt(sin(x)*sin(x) + cos(x)*cos(x)) END"
    );
    // Floats won't be exactly 1.0; check that it's close.
    // Our test infrastructure prints with Factor's default — likely
    // "1.0" or close.  Assert the leading "1.0" substring rather
    // than exact equality.
    assert!(out.starts_with("1.0") || out.starts_with("0.99"),
        "expected near 1.0, got {out:?}");
}
