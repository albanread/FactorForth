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

/// Regression for #80 — the real root cause: a LET `x^2` must square by
/// multiplication, NOT float exponentiation.
///
/// `math.functions:^` with a FLOAT exponent computes `exp(y*log x)`, and
/// `log` of a negative number is complex — so `(-3.0) 2.0 ^` returns a
/// COMPLEX number, not `9.0`.  Magnitude / distance code squares negative
/// differences all the time (`vmag = sqrt(x^2+y^2)`, `vdist = |a-b|`), so
/// before the fix those silently went complex and then surfaced as a
/// bogus "no method" downstream.  The codegen now emits `x dup *` for
/// small integer powers, which is correct for any sign (and faster).
///
/// This test would have FAILED before the fix: the vmag of a vector with
/// negative components must be the real magnitude.
#[test]
#[ignore]
fn negative_components_square_to_real_magnitude() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        \ |(-3,-4)| = 5.0 (real).  Pre-fix this was complex -> "no method".
        ." negmag=" -3.0e -4.0e <vec2> vmag .          \ 5.0
        \ Distance squares the negative difference (1,2)-(4,6) = (-3,-4).
        ." dist=" 1.0e 2.0e <vec2> 4.0e 6.0e <vec2> vdist .   \ 5.0
        \ Same for complex modulus.
        ." cmod=" -3.0e -4.0e <complex> vmag .          \ 5.0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("negmag=5."), "negative vec2 magnitude must be real 5.0: {cap}");
    assert!(cap.contains("dist=5."), "vdist over negative difference: {cap}");
    assert!(cap.contains("cmod=5."), "negative complex modulus must be real 5.0: {cap}");
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

/// vec2-specific enrichments: normalize (unit vector) and perp (rotate
/// 90° left).
#[test]
#[ignore]
fn vec2_extras() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        \ normalize (3,4) -> (0.6, 0.8); its magnitude is 1.
        3.0e 4.0e <vec2> normalize VALUE u
        ." nx=" u vec2>x .          \ 0.6
        ." ny=" u vec2>y .          \ 0.8
        ." nm=" u vmag .            \ 1.0

        \ perp (3,4) -> (-4,3)
        3.0e 4.0e <vec2> perp VALUE p
        ." px=" p vec2>x .          \ -4.0
        ." py=" p vec2>y .          \ 3.0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("nx=0.6") && cap.contains("ny=0.8"), "normalize: {cap}");
    assert!(cap.contains("nm=1."), "normalized magnitude: {cap}");
    assert!(cap.contains("px=-4.") && cap.contains("py=3."), "perp: {cap}");
}

/// complex-specific enrichments: phase (argument), recip (1/z), c/ (full
/// division).
#[test]
#[ignore]
fn complex_extras() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        \ phase(0+1i) = atan2(1,0) = pi/2 ~ 1.5707963
        0.0e 1.0e <complex> phase ." ph=" .

        \ recip(1+1i) = conj/|z|^2 = (1-1i)/2 = 0.5 - 0.5i
        1.0e 1.0e <complex> recip VALUE r
        ." rr=" r complex>re .      \ 0.5
        ." ri=" r complex>im .      \ -0.5

        \ (4+2i)/(1+1i) = (6-2i)/2 = 3 - 1i
        4.0e 2.0e <complex>  1.0e 1.0e <complex>  c/  VALUE q
        ." qr=" q complex>re .      \ 3.0
        ." qi=" q complex>im .      \ -1.0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("ph=1.5707"), "phase: {cap}");
    assert!(cap.contains("rr=0.5") && cap.contains("ri=-0.5"), "recip: {cap}");
    assert!(cap.contains("qr=3.") && cap.contains("qi=-1."), "c/: {cap}");
}

/// The derived protocol word `vneg` is written ONCE over the `vscale`
/// generic — so the SAME definition serves vec2 AND complex.  This is
/// the protocol payoff: a new type joins for free, no per-type code.
///
/// (Derived words over the 2-class generics v+/v- — vdist, vlerp, vmid
/// — are held back pending a multi-dispatch-from-colon finalization
/// fix; the direct v+/v- calls are covered by the tests above.)
#[test]
#[ignore]
fn derived_protocol_polymorphic() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, NUMERICS);
    run(&s, &mut ctx, r#"
        \ vneg on a vec2 -> (-3,-4)
        3.0e 4.0e <vec2> vneg VALUE n
        ." vnx=" n vec2>x .                          \ -3.0
        ." vny=" n vec2>y .                          \ -4.0
        ." vdist=" 1.0e 2.0e <vec2> 4.0e 6.0e <vec2> vdist .   \ |(-3,-4)| = 5.0
        0.0e 0.0e <vec2> 10.0e 20.0e <vec2> 0.5e vlerp VALUE m
        ." vlx=" m vec2>x .                          \ 5.0
        ." vly=" m vec2>y .                          \ 10.0

        \ the SAME words on complex — one definition, every protocol type
        1.0e 2.0e <complex> vneg VALUE c
        ." cnr=" c complex>re .                      \ -1.0
        ." cdist=" 1.0e 2.0e <complex> 4.0e 6.0e <complex> vdist .  \ 5.0
        0.0e 0.0e <complex> 10.0e 20.0e <complex> vmid VALUE cm
        ." cmr=" cm complex>re .                     \ 5.0
        ." cmi=" cm complex>im .                     \ 10.0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("vnx=-3.") && cap.contains("vny=-4."), "vneg vec2: {cap}");
    assert!(cap.contains("vdist=5."), "vdist vec2: {cap}");
    assert!(cap.contains("vlx=5.") && cap.contains("vly=10."), "vlerp vec2: {cap}");
    assert!(cap.contains("cnr=-1."), "vneg complex: {cap}");
    assert!(cap.contains("cdist=5."), "vdist complex (same word): {cap}");
    assert!(cap.contains("cmr=5.") && cap.contains("cmi=10."), "vmid complex: {cap}");
}
