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
