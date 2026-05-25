//! tests/session_file_access.rs — M2.x #32 ANS File Access (minimal).
//!
//! Only INCLUDED is shipped in this milestone — the critical word
//! for the Forth 2012 test runner.  Rest of the File Access Word
//! Set (OPEN-FILE etc.) is deferred until a real user wants it.
//!
//! INCLUDED reads an ANS source file and evaluates it as if its
//! contents were appended to the current eval.  The file is
//! compiled through NewFactor's Rust pipeline (via the
//! `rt_compile_ans` FFI extern), THEN handed to Factor's `(eval)`.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

/// Absolute path to a fixture file under tests/fixtures/.
fn fixture(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/tests/fixtures/{name}");
    // Forward slashes work on Windows for `std::fs::read_to_string`
    // and keep escaping out of the Forth-source literal.
    path.replace('\\', "/")
}

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

#[test]
#[ignore]
fn included_prints_fixture_output() {
    let path = fixture("included-hello.fs");
    // S" path" produces (c-addr u); INCLUDED consumes it.
    let src = format!(r#"s" {path}" included"#);
    let out = run_capturing(&src);
    assert!(out.contains("hello from included"),
        "fixture content should appear in output, got {out:?}");
}

// NOTE: a test like `included path; included-word .` won't compile
// because NewFactor's resolver runs at compile time and can't know
// about words defined by an INCLUDED file (which only happens at
// runtime).  The included file can call its own definitions
// internally — that's tested via the fixture printing the result
// of `included-word` from within the fixture itself.

#[test]
#[ignore]
fn included_with_managed_string_path_works() {
    // Same test but using `>$ $>addr` round-trip — proves
    // INCLUDED accepts any (c-addr u) regardless of how it was
    // produced.
    let path = fixture("included-hello.fs");
    let src = format!(r#"s$" {path}" $>addr included"#);
    let out = run_capturing(&src);
    assert!(out.contains("hello from included"),
        "managed-string path should work, got {out:?}");
}

#[test]
#[ignore]
fn included_missing_file_does_not_kill_session() {
    // Use a path that definitely doesn't exist.  Should produce
    // a diagnostic but not crash the session.
    let src = r#"s" E:/nonexistent/file/path.fs" included"#;
    let out = run_capturing(src);
    // We expect an error-y string but no panic.
    let lower = out.to_lowercase();
    assert!(lower.contains("included") || lower.contains("file") ||
            lower.contains("error") || lower.contains("cannot"),
        "expected file-access diagnostic, got {out:?}");
}
