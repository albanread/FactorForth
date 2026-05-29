//! CoreProtocols Layer 2 — numeric value types (vec2, complex).
//!
//! The library ships as release/factorforth/lib/numerics.f, written in
//! ANS Forth on the object system, method bodies in the LET infix DSL.
//! These load it the way user code would and exercise the arithmetic
//! protocol.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

const CORE: &str = include_str!("../release/factorforth/lib/core.f");
const NUMERICS: &str = include_str!("../release/factorforth/lib/numerics.f");

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

/// vec2 addition — the keystone case: a multi-OUTPUT LET (`-> ( sx sy )`)
/// inside a multi-DISPATCH method (`a:vec2 b:vec2`), result rebuilt with
/// the boa constructor.  If this works, the rest follows.
#[test]
#[ignore]
fn vec2_add_via_multi_output_let() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        1.0e 2.0e <vec2>  3.0e 4.0e <vec2>  v+  VALUE r
        ." rx=" r vec2>x .       \ 4.0
        ." ry=" r vec2>y .       \ 6.0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("rx=4."), "vec2 v+ x: {cap}");
    assert!(cap.contains("ry=6."), "vec2 v+ y: {cap}");
}

/// The rest of the vec2 protocol: subtract, scalar multiply, magnitude,
/// dot product, and show.
#[test]
#[ignore]
fn vec2_protocol() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        5.0e 7.0e <vec2>  1.0e 2.0e <vec2>  v-  VALUE d
        ." dx=" d vec2>x .          \ 4.0
        ." dy=" d vec2>y .          \ 5.0

        3.0e 4.0e <vec2>  2.0e vscale VALUE s2
        ." sx=" s2 vec2>x .         \ 6.0
        ." sy=" s2 vec2>y .         \ 8.0

        ." mag=" 3.0e 4.0e <vec2> vmag .         \ 5.0
        ." dot=" 1.0e 2.0e <vec2> 3.0e 4.0e <vec2> dot .   \ 1*3+2*4 = 11.0
        ." show=" 3.0e 4.0e <vec2> show           \ (3.0 , 4.0 )
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("dx=4.") && cap.contains("dy=5."), "v-: {cap}");
    assert!(cap.contains("sx=6.") && cap.contains("sy=8."), "vscale: {cap}");
    assert!(cap.contains("mag=5."), "vmag: {cap}");
    assert!(cap.contains("dot=11."), "dot: {cap}");
    assert!(cap.contains("show=(3."), "show: {cap}");
}

/// complex arithmetic: add (shared generic), full product, conjugate,
/// modulus.
#[test]
#[ignore]
fn complex_protocol() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        \ (1+2i) + (3+4i) = 4+6i  — v+ shared with vec2, dispatched here
        1.0e 2.0e <complex>  3.0e 4.0e <complex>  v+  VALUE a
        ." are=" a complex>re .      \ 4.0
        ." aim=" a complex>im .      \ 6.0

        \ (1+2i)(3+4i) = (3-8) + (4+6)i = -5+10i
        1.0e 2.0e <complex>  3.0e 4.0e <complex>  c*  VALUE p
        ." pre=" p complex>re .      \ -5.0
        ." pim=" p complex>im .      \ 10.0

        \ conj(3+4i) = 3-4i ; |3+4i| = 5
        3.0e 4.0e <complex> conj VALUE k
        ." kim=" k complex>im .      \ -4.0
        ." mod=" 3.0e 4.0e <complex> vmag .   \ 5.0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("are=4.") && cap.contains("aim=6."), "complex v+: {cap}");
    assert!(cap.contains("pre=-5.") && cap.contains("pim=10."), "c*: {cap}");
    assert!(cap.contains("kim=-4."), "conj: {cap}");
    assert!(cap.contains("mod=5."), "vmag/modulus: {cap}");
}

/// The multiple-dispatch payoff: `v+` is ONE generic, dispatched on the
/// classes of BOTH arguments — vec2+vec2 and complex+complex hit
/// different methods, and there's no privileged receiver.
#[test]
#[ignore]
fn v_plus_is_one_generic_two_types() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        \ same verb, two backings — dispatch picks the right method
        2.0e 3.0e <vec2>     4.0e 5.0e <vec2>     v+  vmag  ." vm=" .  \ |(6,8)|=10
        2.0e 3.0e <complex>  4.0e 5.0e <complex>  v+  vmag  ." cm=" .  \ |6+8i|=10
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("vm=10."), "v+ then vmag on vec2: {cap}");
    assert!(cap.contains("cm=10."), "v+ then vmag on complex: {cap}");
}
