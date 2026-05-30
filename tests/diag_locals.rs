//! Forth-2012 `{: name1 name2 :}` locals — end-to-end smoke against the
//! embedded VM.  Re-entrant safety is the whole point: a colon-def
//! that uses locals must work even when called from inside the xt
//! body of an `each` over a collection of those very kind of calls.

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

fn run(s: &Session, ctx: &mut CompileContext, src: &str) {
    let ir = compile_in_context(src, ctx).unwrap_or_else(|e| panic!("compile: {e}"));
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
}

/// The minimum: declare two locals, use them by name in the body.
#[test]
#[ignore]
fn two_locals_bind_inputs() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        : add2 ( a b -- c ) {: x y :} x y + ;
        ." r=" 10 20 add2 .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("r=30"), "locals didn't bind: {cap}");
}

/// Locals must be re-entrant: a recursive call must not corrupt the
/// outer activation's bindings.  Compute the sum 1+2+3 by recursion.
#[test]
#[ignore]
fn locals_survive_recursion() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        : sum-to ( n -- s ) {: n :}
            n 0 = if 0 else
                n 1 - sum-to    \ recurse with n-1; sum-to consumes n-1
                n +              \ then add the outer n
            then ;
        ." s=" 3 sum-to .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("s=6"), "recursive locals broken: {cap}");
}

/// Locals shadow user words: a local named `dup` shadows the kernel
/// `dup` inside the body, while the outer scope still sees `dup` as
/// the kernel word.
#[test]
#[ignore]
fn locals_shadow_outer_words() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        : double ( n -- m ) dup + ;          \ outer uses kernel dup
        : with-shadow ( n -- ? ) {: dup :}    \ here `dup` means the local
            dup 42 = ;
        ." d=" 5 double .                    \ 10  (outer dup still works)
        ." |s=" 42 with-shadow .             \ -1  (local `dup` is 42)
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("d=10"), "outer `dup` broken: {cap}");
    assert!(cap.contains("s=-1"), "local `dup` didn't shadow: {cap}");
}
