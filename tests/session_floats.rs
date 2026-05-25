//! tests/session_floats.rs — Phase 3.1d float-FFI proof of life.
//!
//! Real-time graphics, audio, anything past printf-style I/O needs
//! the host and Factor to ship `double`s in both directions through
//! XMM0 (Win64 ABI for the first FP arg / FP return).  This file
//! proves that without precision loss.
//!
//!   Factor side               Rust side
//!   ──────────                ──────────
//!   1.5 rt_check_double  →    fn rt_check_double(x: f64) -> f64
//!         ↑ XMM0                  ↓ x * 2.0 + 1.0
//!                              XMM0 → 4.0 → Factor stack
//!
//!   1.234e0 rt_emit_double →  fn rt_emit_double(x: f64)
//!         ↑ XMM0                  ↓ x.to_le_bytes() → output sink
//!                              8 bytes land in captured Vec<u8>
//!                              Rust test reconstructs f64::from_le_bytes
//!                              and asserts u64-bit equality.
//!
//! If any of this is broken (calling convention, marshaling,
//! precision), the bit-compare catches it.

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

/// rt_check_double receives 1.5 in XMM0, returns 4.0 in XMM0.
/// We bounce the result back out as 8 bytes via rt_emit_double
/// and compare bit-exact — anything other than 4.0_f64 fails.
#[test]
#[ignore]
fn float_ffi_round_trip_xmm0() {
    let (session, out) = new_test_session(b"");
    let src = "USING: forth.runtime ; 1.5 rt_check_double rt_emit_double";
    session.eval(src).expect("eval");

    let bytes = out.lock().unwrap();
    assert_eq!(bytes.len(), 8,
        "expected exactly 8 bytes of IEEE-754 payload, got {} bytes: {:?}",
        bytes.len(), &*bytes);
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    let got = f64::from_le_bytes(arr);
    let expected = 1.5_f64 * 2.0 + 1.0;
    assert_eq!(got.to_bits(), expected.to_bits(),
        "bit-exact mismatch: expected {expected} ({:#x}), got {got} ({:#x})",
        expected.to_bits(), got.to_bits());
}

/// Same shape with a value whose IEEE-754 encoding has bits set
/// across the whole mantissa — catches any silent demotion to
/// single-precision or rounding-mode mistakes.
#[test]
#[ignore]
fn float_ffi_round_trip_full_mantissa() {
    let (session, out) = new_test_session(b"");
    // π has every-mantissa-bit-set-ish; a precision-losing path
    // would round it to something visibly different.
    let src = "USING: forth.runtime ; 3.141592653589793 rt_emit_double";
    session.eval(src).expect("eval");

    let bytes = out.lock().unwrap();
    assert_eq!(bytes.len(), 8);
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    let got = f64::from_le_bytes(arr);
    let expected = std::f64::consts::PI;
    assert_eq!(got.to_bits(), expected.to_bits(),
        "π round-trip mismatch: expected {:#x}, got {:#x}",
        expected.to_bits(), got.to_bits());
}

/// Negative + denormal-ish range, just to shake out the sign bit
/// and the exponent path.
#[test]
#[ignore]
fn float_ffi_round_trip_negative() {
    let (session, out) = new_test_session(b"");
    let src = "USING: forth.runtime ; -1.0e-200 rt_emit_double";
    session.eval(src).expect("eval");

    let bytes = out.lock().unwrap();
    assert_eq!(bytes.len(), 8);
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    let got = f64::from_le_bytes(arr);
    let expected = -1.0e-200_f64;
    assert_eq!(got.to_bits(), expected.to_bits(),
        "negative-exponent round-trip mismatch: expected {:#x}, got {:#x}",
        expected.to_bits(), got.to_bits());
}
