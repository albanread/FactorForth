//! tests/session_managed_strings.rs — M2.x #43 managed strings.
//!
//! Mirrors WF64's `demos/strings.f` exercising the `$` vocab.
//! Strings are Factor's native immutable `string` type — GC-tracked,
//! Unicode-aware, no PAD, no counted-string footguns.
//!
//! Each test compiles a short Forth program that exercises one or
//! two `$` words, captures the host output, and asserts on the
//! exact rendered text.  Equivalent to `T{ … -> … }T` assertions
//! but checking via output rather than stack inspection.

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

// ── S$" literal — pushes a single Factor string handle ─────────────────────

#[test]
#[ignore]
fn ms_literal_prints() {
    let out = run_capturing(r#"S$" hello, world" $."#);
    assert_eq!(out, "hello, world");
}

// ── $len byte length ────────────────────────────────────────────────────────

#[test]
#[ignore]
fn ms_len() {
    let out = run_capturing(r#"S$" hello" $len ."#);
    assert_eq!(out, "5");
}

#[test]
#[ignore]
fn ms_len_empty() {
    let out = run_capturing(r#"S$" " $len ."#);
    assert_eq!(out, "0");
}

// ── $+ concatenation ────────────────────────────────────────────────────────

#[test]
#[ignore]
fn ms_concat() {
    let out = run_capturing(r#"S$" hello"  S$" , world!" $+  $."#);
    assert_eq!(out, "hello, world!");
}

#[test]
#[ignore]
fn ms_concat_three() {
    // Chain three concats: "a" + "b" + "c" = "abc"
    let out = run_capturing(r#"S$" a"  S$" b"  $+  S$" c" $+  $."#);
    assert_eq!(out, "abc");
}

// ── $upper / $lower ────────────────────────────────────────────────────────

#[test]
#[ignore]
fn ms_upper() {
    let out = run_capturing(r#"S$" FoRtH" $upper $."#);
    assert_eq!(out, "FORTH");
}

#[test]
#[ignore]
fn ms_lower() {
    let out = run_capturing(r#"S$" FoRtH" $lower $."#);
    assert_eq!(out, "forth");
}

// ── $find — index or -1 ────────────────────────────────────────────────────

#[test]
#[ignore]
fn ms_find_present() {
    // "ll" in "hello" starts at index 2.
    let out = run_capturing(r#"S$" hello"  S$" ll"  $find ."#);
    assert_eq!(out, "2");
}

#[test]
#[ignore]
fn ms_find_absent() {
    let out = run_capturing(r#"S$" hello"  S$" xyz"  $find ."#);
    assert_eq!(out, "-1");
}

#[test]
#[ignore]
fn ms_find_at_zero() {
    // Prefix match: needle at index 0.
    let out = run_capturing(r#"S$" hello"  S$" he"  $find ."#);
    assert_eq!(out, "0");
}

// ── $slice — substring extraction ───────────────────────────────────────────

#[test]
#[ignore]
fn ms_slice() {
    // "hello"[1, len=3] → "ell"
    let out = run_capturing(r#"S$" hello"  1 3 $slice  $."#);
    assert_eq!(out, "ell");
}

#[test]
#[ignore]
fn ms_slice_to_end() {
    // "hello"[2, len=3] → "llo"
    let out = run_capturing(r#"S$" hello"  2 3 $slice  $."#);
    assert_eq!(out, "llo");
}

// ── $contains? / $starts? / $ends? — predicates ─────────────────────────────

#[test]
#[ignore]
fn ms_contains_true() {
    let out = run_capturing(r#"S$" hello"  S$" ell"  $contains? ."#);
    assert_eq!(out, "-1");
}

#[test]
#[ignore]
fn ms_contains_false() {
    let out = run_capturing(r#"S$" hello"  S$" xyz"  $contains? ."#);
    assert_eq!(out, "0");
}

#[test]
#[ignore]
fn ms_starts_true() {
    let out = run_capturing(r#"S$" hello"  S$" he"  $starts? ."#);
    assert_eq!(out, "-1");
}

#[test]
#[ignore]
fn ms_starts_false() {
    let out = run_capturing(r#"S$" hello"  S$" lo"  $starts? ."#);
    assert_eq!(out, "0");
}

#[test]
#[ignore]
fn ms_ends_true() {
    let out = run_capturing(r#"S$" hello"  S$" lo"  $ends? ."#);
    assert_eq!(out, "-1");
}

#[test]
#[ignore]
fn ms_ends_false() {
    let out = run_capturing(r#"S$" hello"  S$" he"  $ends? ."#);
    assert_eq!(out, "0");
}

// ── $cmp — lex compare returning -1/0/1 ─────────────────────────────────────

#[test]
#[ignore]
fn ms_cmp_equal() {
    let out = run_capturing(r#"S$" abc"  S$" abc"  $cmp ."#);
    assert_eq!(out, "0");
}

#[test]
#[ignore]
fn ms_cmp_lt() {
    let out = run_capturing(r#"S$" abc"  S$" abd"  $cmp ."#);
    assert_eq!(out, "-1");
}

#[test]
#[ignore]
fn ms_cmp_gt() {
    let out = run_capturing(r#"S$" abd"  S$" abc"  $cmp ."#);
    assert_eq!(out, "1");
}

// ── $hash — same string hashes to same value ────────────────────────────────

#[test]
#[ignore]
fn ms_hash_stable_within_run() {
    // Compute hash of "alpha" twice, subtract — should be 0.
    let out = run_capturing(r#"S$" alpha" $hash  S$" alpha" $hash  - ."#);
    assert_eq!(out, "0");
}

#[test]
#[ignore]
fn ms_hash_differs_for_different_strings() {
    // hash("a") - hash("b") should be nonzero (probabilistically certain).
    // Test for nonzero by abs > 0.
    let out = run_capturing(r#"S$" alpha" $hash  S$" beta" $hash  - 0= ."#);
    assert_eq!(out, "0", "Two distinct strings unexpectedly had the same hash");
}

// ── Number ↔ string ─────────────────────────────────────────────────────────

#[test]
#[ignore]
fn ms_int_to_string() {
    let out = run_capturing(r#"1234 int>$ $."#);
    assert_eq!(out, "1234");
}

#[test]
#[ignore]
fn ms_string_to_int() {
    let out = run_capturing(r#"S$" 1234" $>int ."#);
    assert_eq!(out, "1234");
}

#[test]
#[ignore]
fn ms_string_to_int_negative() {
    let out = run_capturing(r#"S$" -42" $>int ."#);
    assert_eq!(out, "-42");
}

// ── >$ and $>addr round-trip with legacy (c-addr u) ─────────────────────────

#[test]
#[ignore]
fn ms_to_dollar_from_legacy() {
    // S" "hello" produces (c-addr u); >$ wraps into a managed string;
    // $. prints.  Same observable output as bypassing the wrap.
    let out = run_capturing(r#"S" hello" >$ $."#);
    assert_eq!(out, "hello");
}

#[test]
#[ignore]
fn ms_to_addr_to_legacy_type() {
    // Round-trip: S$" hello"  $>addr → (c-addr u) → TYPE prints it.
    let out = run_capturing(r#"S$" hello" $>addr type"#);
    assert_eq!(out, "hello");
}

// ── Pipeline demo (mirrors WF64 strings.f) ─────────────────────────────────

#[test]
#[ignore]
fn ms_pipeline_upper_concat() {
    let out = run_capturing(
        r#"S$" hello, " S$" world" $upper $+ $."#
    );
    // hello,  + WORLD = "hello, WORLD"
    assert_eq!(out, "hello, WORLD");
}

#[test]
#[ignore]
fn ms_inside_colon_def() {
    // Managed strings should work inside : ... ; just like primitive
    // values do.  Define a word that greets by name.
    let out = run_capturing(
        r#": greet ( name -- )  S$" Hello, " swap $+  S$" !" $+  $. ;
           S$" world" greet"#
    );
    assert_eq!(out, "Hello, world!");
}
