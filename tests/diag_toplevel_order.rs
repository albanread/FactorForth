//! Top-level execution order — a regression guard.
//!
//! A defining word like `VALUE` greedily captures the pending run of
//! top-level expressions as its initializer (ANS: it binds whatever is
//! on the stack).  The bug this guards against: emitting that
//! initializer's EXECUTION in the up-front definitions phase, which
//! hoisted any side effects (a swallowed `." ..."`) ahead of earlier
//! top-level code.  Forth is sequential — load-time side effects must
//! run in source order.

use newfactor::compiler::{compile_in_context, CompileContext};

/// The three markers `m1` / `m2` / `m3` are written in that source
/// order.  `m2` is swallowed into `x`'s VALUE initializer; `m1` is a
/// flushed top-level run before the VALUE, `m3` one after.  In the
/// emitted IR they must still appear m1 → m2 → m3.
///
/// Pre-fix, `m2` (in the VALUE seed) emitted in the definitions phase,
/// landing BEFORE `m1`/`m3` in the IR — this assertion would fail.
#[test]
fn value_initializer_runs_in_source_order() {
    let mut ctx = CompileContext::new();
    let ir = compile_in_context(
        // VARIABLE flushes the pending `." m1 "` into its own top-level
        // run; VALUE then swallows `." m2 "`; `." m3 "` trails after.
        r#"." m1 " VARIABLE v ." m2 " 0 VALUE x ." m3 ""#,
        &mut ctx,
    ).expect("compile");
    let p1 = ir.find("m1").expect("m1 present");
    let p2 = ir.find("m2").expect("m2 present");
    let p3 = ir.find("m3").expect("m3 present");
    assert!(
        p1 < p2 && p2 < p3,
        "top-level markers must keep source order in IR (m1<m2<m3), \
         got positions {p1} {p2} {p3}:\n{ir}",
    );
}

// ── End-to-end: the program actually prints in order ───────────────

#[cfg(target_os = "windows")]
mod eval {
    use std::sync::{Arc, Mutex};
    use newfactor::compiler::{compile_in_context, CompileContext};
    use newfactor::session::{IoMode, Session, SessionOpts};

    #[test]
    #[ignore]
    fn prints_run_in_source_order_around_a_value() {
        let out = Arc::new(Mutex::new(Vec::<u8>::new()));
        let opts = SessionOpts::defaults_for_crate(IoMode::Test {
            input: vec![], output: out.clone(),
        });
        let s = Session::new(opts).expect("Session::new");
        let mut ctx = CompileContext::new();
        let ir = compile_in_context(
            r#"." [m1] " VARIABLE v ." [m2] " 0 VALUE x ." [m3] " x ."  x=" ."#,
            &mut ctx,
        ).expect("compile");
        s.eval(&ir).expect("eval");
        let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        eprintln!("captured: {cap:?}");
        let p1 = cap.find("[m1]").expect("m1 printed");
        let p2 = cap.find("[m2]").expect("m2 printed");
        let p3 = cap.find("[m3]").expect("m3 printed");
        assert!(p1 < p2 && p2 < p3, "printed in source order: {cap}");
        assert!(cap.contains("x=0"), "x bound to its initializer: {cap}");
    }
}
