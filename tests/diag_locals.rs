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

/// `_` is the anonymous-discard marker — it consumes a stack slot
/// but binds no name, so the body has no way to reference it.  This
/// is the clean idiom for a method whose effect dictates an arg the
/// implementation ignores (the object catch-all in a generic, for
/// instance).
#[test]
#[ignore]
fn underscore_discards_a_local() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        \ Head locals: discard the middle of three args.
        : take-outer ( a b c -- d ) {: x _ z :} x z + ;
        ." r=" 100 999 5 take-outer .            \ 105

        \ Two `_`s in one block; both consumed, neither bound.
        : sum-edges ( a b c d -- s ) {: a _ _ d :} a d + ;
        ." s=" 7 88 77 3 sum-edges .              \ 10
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("r=105"), "single _ discard: {cap}");
    assert!(cap.contains("s=10"), "multiple _ discards: {cap}");
}

/// Mid-body `_` works the same way: a `{: a _ c :}` block in the
/// middle of a body consumes three stack values, naming only two.
#[test]
#[ignore]
fn underscore_in_mid_body_block() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        : pick-around ( a b c -- ac ) {: a _ c :} a c + ;
        ." mid=" 4 999 6 pick-around .            \ 10
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("mid=10"), "mid-body _ discard: {cap}");
}

/// Referring to `_` from user code is an error — it isn't a real
/// binding.  The resolver doesn't add it to the locals scope, so a
/// reference falls through to the normal "undefined word" path.
#[test]
#[ignore]
fn underscore_is_not_referenceable() {
    let (_s, _out, mut ctx) = fresh();
    let src = ": bogus ( a b -- c ) {: _ y :} _ y + ;";
    let result = compile_in_context(src, &mut ctx);
    assert!(result.is_err(), "expected compile error; got {result:?}");
}

/// METHOD: bodies accept the same `{: ... :}` head-locals form as
/// `:` bodies.  Because `multi-methods:METHOD:` is itself a parsing
/// word and doesn't open a `::` locals scope, the emitter routes
/// these methods through a generated helper word — invisible to the
/// user.  This is the cleanest expression of the catch-all idiom:
///
///     METHOD: show ( x:object -- ) {: _ :} ." <object>" ;
#[test]
#[ignore]
fn method_head_locals_work() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        GENERIC: show ( x -- )
        METHOD: show ( x:object -- )  {: _ :}
            ." <object>" ;
        CLASS: widget ;
        ." r=" <widget> show
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("r=<object>"), "method head locals: {cap}");
}

/// A METHOD: with multiple head locals binds them in declaration
/// order.  Discard markers and real names mix freely.
#[test]
#[ignore]
fn method_head_locals_mix_discard_and_named() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, r#"
        GENERIC: combine ( a b c -- d )
        CLASS: bag ;
        METHOD: combine ( a:bag b c -- d )  {: _ b c :}
            b c + ;
        ." r=" <bag> 10 32 combine .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("r=42"), "mixed _ and named: {cap}");
}
