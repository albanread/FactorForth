//! CoreProtocols Layer 3 — text & streams.
//!
//! The library ships as release/factorforth/lib/streams.f, written in
//! ANS Forth on the object system.  Its signature idea: end-of-file is
//! an OBJECT (<eof>), not a flag — `read-char` returns a char code or
//! the marker, and the read loop dispatches on that.  These load it the
//! way user code would (after core.f + collections.f) and exercise the
//! stream protocol + the derived `copy-stream` / `read-all`.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

const CORE: &str = include_str!("../release/factorforth/lib/core.f");
const COLLECTIONS: &str = include_str!("../release/factorforth/lib/collections.f");
const STREAMS: &str = include_str!("../release/factorforth/lib/streams.f");

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

fn run(s: &Session, ctx: &mut CompileContext, src: &str) {
    let ir = compile_in_context(src, ctx).unwrap_or_else(|e| panic!("compile: {e}"));
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
}

fn load_layers(s: &Session, ctx: &mut CompileContext) {
    run(s, ctx, CORE);
    run(s, ctx, COLLECTIONS);
    run(s, ctx, STREAMS);
}

/// The `string` value type: build from a literal, show it, measure it,
/// index it, compare it (Layer 0 equals?), and concatenate.
#[test]
#[ignore]
fn string_value_type() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." show=" S" abc" >string show           \ abc
        ." len=" S" abcde" >string size .         \ 5
        ." at=" 1 S" abc" >string at .             \ 98 (b)
        ." eq=" S" ab" >string S" ab" >string equals? .    \ -1
        ." ne=" S" ab" >string S" ax" >string equals? .    \ 0
        ." cat=" S" foo" >string S" bar" >string string-append show  \ foobar
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("show=abc"), "show: {cap}");
    assert!(cap.contains("len=5"), "size: {cap}");
    assert!(cap.contains("at=98"), "at: {cap}");
    assert!(cap.contains("eq=-1"), "equals? true: {cap}");
    assert!(cap.contains("ne=0"), "equals? false: {cap}");
    assert!(cap.contains("cat=foobar"), "string-append: {cap}");
}

/// split breaks a string on a delimiter char into a darray of strings;
/// join glues them back with a (possibly different) delimiter.  They
/// round-trip.
#[test]
#[ignore]
fn split_and_join() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        \ "a,bb,ccc" split on ',' -> 3 fields
        S" a,bb,ccc" >string 44 split VALUE parts   \ 44 = ','
        ." n=" parts size .                          \ 3
        ." p0=" 0 parts at show                      \ a
        ." |p1=" 1 parts at show                     \ bb
        ." |p2=" 2 parts at show                     \ ccc
        \ join the same parts with '-' (45)
        ." |joined=" parts 45 join show              \ a-bb-ccc
        \ round-trip: split then join on the same delim reproduces input
        ." |rt=" S" x:y:z" >string 58 split 58 join show   \ x:y:z
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n=3"), "field count: {cap}");
    assert!(cap.contains("p0=a") && cap.contains("p1=bb") && cap.contains("p2=ccc"), "fields: {cap}");
    assert!(cap.contains("joined=a-bb-ccc"), "join: {cap}");
    assert!(cap.contains("rt=x:y:z"), "split/join round-trip: {cap}");
}

/// read-line splits an input stream on newlines, returning a string
/// per line (newline consumed, not included).
#[test]
#[ignore]
fn read_line_splits_on_newline() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, "S\" line1\nline2\" str>reader VALUE r\n        .\" L1=\" r read-line show\n        .\" |L2=\" r read-line show\n        .\" |\"");
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("L1=line1"), "first line: {cap}");
    assert!(cap.contains("L2=line2"), "second line: {cap}");
}

/// Roundtrip: a string-reader, drained into a writer via `read-all`
/// (which uses the derived `copy-stream`), reproduces the input.
#[test]
#[ignore]
fn reader_to_writer_roundtrip() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        S" Hello, streams!" str>reader read-all writer-emit
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("Hello, streams!"), "roundtrip: {cap}");
}

/// EOF is an object: read each char, then `read-char` yields <eof>.
#[test]
#[ignore]
fn eof_is_an_object() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        S" Hi" str>reader VALUE r
        ." c1=" r read-char emit              \ H
        ." c2=" r read-char emit              \ i
        ." end=" r read-char eof? .           \ -1 (true) — drained
        ." more=" S" x" str>reader read-char eof? .   \ 0 (false) — a real char
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("c1=H") && cap.contains("c2=i"), "chars: {cap}");
    assert!(cap.contains("end=-1"), "drained reader returns <eof>: {cap}");
    assert!(cap.contains("more=0"), "non-empty reader is not <eof>: {cap}");
}

/// The polymorphic-loop payoff: `copy-stream` is written ONCE over the
/// protocol; drop a transforming output stream under it and the same
/// loop transforms.  Here we copy through a writer, then re-read and
/// upper-case as we go — proving read-char/write-char compose.
#[test]
#[ignore]
fn copy_stream_composes() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        \ upcase: copy a reader to a writer, upper-casing a..z on the way.
        : lower? ( ch -- ? )  dup 97 >= swap 122 <= and ;
        : up ( ch -- CH )  dup lower? IF 32 - THEN ;
        : ucopy ( in out -- )
            BEGIN
                over read-char
                dup eof? IF  drop -1  ELSE  up over write-char 0  THEN
            UNTIL 2drop ;
        S" abcXYz!" str>reader <writer> dup >r ucopy r> writer-emit
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("ABCXYZ!"), "uppercasing copy: {cap}");
}
