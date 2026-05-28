//! Programming-Tools word set — .S / WORDS / DUMP.
//!
//! DUMP is the headline: re-imagined for our value model.  ANS
//! `DUMP ( addr u -- )` hex-dumps raw memory; ours inspects the
//! VALUE on top of the stack (type tag + value, plus a hex/ASCII
//! dump of the backing bytes for strings and nf-addrs) and leaves
//! it in place.

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

/// .S prints the stack non-destructively in gforth style:
/// `<depth> a b c`, and the values survive for further use.
#[test]
#[ignore]
fn dot_s_is_nondestructive() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        1 2 3 .s
        + + .       \ if .s consumed nothing, this prints 6
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("<3>"), "depth marker expected: {cap}");
    assert!(cap.contains('1') && cap.contains('2') && cap.contains('3'),
        "stack contents expected: {cap}");
    assert!(cap.contains('6'), "values must survive .s for the sum: {cap}");
}

/// DUMP on an integer prints a type tag and the value (decimal +
/// hex), and leaves the integer on the stack.
#[test]
#[ignore]
fn dump_int_reports_type_and_value() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        255 dump
        .          \ dump left it; this prints 255
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("INT"), "type tag expected: {cap}");
    assert!(cap.contains("ff"), "hex of 255 expected: {cap}");
    // leftover integer printed by the trailing `.`
    assert!(cap.matches("255").count() >= 1, "255 should appear: {cap}");
}

/// DUMP on an nf-addr (the c-addr half of a `s" ..."` string pair)
/// prints ADDR + byte count + a classic hex/ASCII dump of the
/// backing bytes.  This is the headline DUMP behaviour: real bytes,
/// not a meaningless pointer value.
#[test]
#[ignore]
fn dump_addr_hex_ascii() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        s" Hi!" drop dump   \ s" leaves ( addr u ); drop the length,
                            \ dump the nf-addr to hex/ASCII its bytes
        drop                \ dump left the addr; clean up
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("ADDR"), "type tag expected: {cap}");
    // 'H'=0x48 'i'=0x69 '!'=0x21 should appear in the hex row
    assert!(cap.contains("48") && cap.contains("69") && cap.contains("21"),
        "hex bytes of 'Hi!' expected: {cap}");
    // ASCII gutter shows the text
    assert!(cap.contains("Hi!"), "ASCII gutter expected: {cap}");
}

/// DUMP on a float prints FLOAT and the value.
#[test]
#[ignore]
fn dump_float_reports_type() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        3.5e0 dump
        drop
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("FLOAT"), "type tag expected: {cap}");
    assert!(cap.contains("3.5"), "float value expected: {cap}");
}

/// WORDS lists the user's own definitions.  After defining `foo`
/// and `bar`, both names should appear.
#[test]
#[ignore]
fn words_lists_user_definitions() {
    let (s, out, mut ctx) = fresh();
    let src = r#"
        : foo ( -- ) ;
        : bar ( -- ) ;
        words
    "#;
    let ir = compile_in_context(src, &mut ctx).expect("compile");
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("foo"), "foo should be listed: {cap}");
    assert!(cap.contains("bar"), "bar should be listed: {cap}");
}
