//! tests/session_io.rs — Phase 3.1b end-to-end I/O redirection.
//!
//! Verifies that Factor calls `rt_write_char` (and friends) when
//! user code does I/O, and that the bytes land in the IoMode's
//! sink.  This is the proof-of-life that the whole FFI-callback
//! mechanism works:
//!
//!   Rust binary exports rt_*  →
//!   Factor's add-library + FUNCTION: resolve them via GetProcAddress  →
//!   forth.runtime's nf-host-output-stream calls rt_write_char  →
//!   Session::IoState's writer closure captures the byte  →
//!   Test's Arc<Mutex<Vec<u8>>> sees the bytes.
//!
//! If any link in that chain breaks, the captured output is wrong.

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

/// The headline test: a program that prints `hello, world` and a
/// newline should land those bytes in the Test-mode output buffer
/// — proving Factor → rt_write_char → IoState → captured Vec<u8>
/// all the way through.
#[test]
#[ignore]
fn host_callback_captures_dot_quote() {
    let (session, out) = new_test_session(b"");
    let ir = newfactor::compiler::compile(": greet  .\" hello, world\" cr ;  greet")
        .expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = out.lock().unwrap();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("hello, world"),
            "expected 'hello, world' in captured output, got {s:?}");
}

/// EMIT one character at a time.  Verifies the per-byte write
/// path (stream-write1) rather than the bulk write path.
#[test]
#[ignore]
fn host_callback_captures_emit() {
    let (session, out) = new_test_session(b"");
    // ASCII 'A' = 65.
    let ir = newfactor::compiler::compile("65 emit 66 emit 67 emit cr").expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = out.lock().unwrap();
    assert_eq!(bytes.as_slice(), b"ABC\n",
               "expected 'ABC\\n', got {bytes:?}");
}

/// `.` (the ANS print-number word) ultimately goes through
/// Factor's `write`.  If our output-stream is bound to the host
/// stream, the bytes flow through rt_write_char.
#[test]
#[ignore]
fn host_callback_captures_number_dot() {
    let (session, out) = new_test_session(b"");
    let ir = newfactor::compiler::compile("42 .").expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = out.lock().unwrap();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("42"), "expected '42' in captured output, got {s:?}");
}

/// The pictured-numeric-output DSL plus TYPE.  Build a string,
/// type it, verify the bytes show up.
#[test]
#[ignore]
fn host_callback_captures_typed_string() {
    let (session, out) = new_test_session(b"");
    let ir = newfactor::compiler::compile("1234 n>$ type cr").expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = out.lock().unwrap();
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("1234"), "expected '1234', got {s:?}");
}

/// KEY reads one byte from the host's input queue.  Pre-feed a
/// byte; the program reads it back and types it.
#[test]
#[ignore]
fn host_callback_routes_key_input() {
    // Pre-feed ASCII 'X' = 88.
    let (session, out) = new_test_session(b"X");
    let ir = newfactor::compiler::compile("key emit cr").expect("compile");
    session.eval(&ir).expect("eval");
    let bytes = out.lock().unwrap();
    assert_eq!(bytes.as_slice(), b"X\n",
               "expected 'X\\n' (KEY round-trip), got {bytes:?}");
}
