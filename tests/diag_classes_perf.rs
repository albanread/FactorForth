//! Empirical dispatch-cost measurement for generic methods.  Pits
//! a generic-function call in a tight loop against a regular `:`
//! definition doing the same work, to ground the "is it slow?"
//! question in actual wall-clock numbers.
//!
//! Run with `cargo test --test diag_classes_perf -- --include-ignored
//! --nocapture` and read the eprintln output — assertions are loose
//! (ratio bound) to avoid flakiness across CI environments.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use std::time::Instant;
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

/// Run a Forth fragment N times in a tight loop and report wall-clock.
fn time_loop(s: &Session, ctx: &mut CompileContext, n: u32, body: &str) -> std::time::Duration {
    let src = format!(
        "{n} 0 do  {body}  loop",
        n = n, body = body,
    );
    let ir = compile_in_context(&src, ctx).expect("compile");
    let t0 = Instant::now();
    s.eval(&ir).expect("eval");
    t0.elapsed()
}

/// Define + benchmark in a SINGLE compile because cross-eval class
/// persistence isn't shipped yet (#64 sprint 2).  Wraps the whole
/// test sequence into one eval so `<circle>` is visible in the loop.
fn run_single(s: &Session, ctx: &mut CompileContext, src: &str) -> std::time::Duration {
    let ir = compile_in_context(src, ctx).expect("compile");
    let t0 = Instant::now();
    s.eval(&ir).expect("eval");
    t0.elapsed()
}

/// Generic dispatch vs direct call.  All in one eval so the
/// `<circle>` constructor is visible during the loops (cross-eval
/// class persistence is task #64 — not shipped yet).  The single
/// source contains BOTH loops back-to-back; we record elapsed time
/// by eyeballing the eval-side Vec captures rather than per-loop
/// wall clock.  Crude but workable.
///
/// For a precise per-loop timing we'd need a Forth-side `utime` /
/// `nano-count` and report from inside the eval; that's a separate
/// addition.  Today we just confirm the generic path is ROUGHLY in
/// the same ballpark as direct by running both N times and looking
/// at the IR.
#[test]
#[ignore]
fn dispatch_vs_direct_call() {
    let (s, out, mut ctx) = fresh();
    let _ = out;

    let n: u32 = 200_000;
    // Generic version
    let src_g = format!(r#"
        CLASS: circle  SLOT: r  ;
        GENERIC: area-g ( c -- a )
        METHOD: area-g ( c:circle -- a )
            circle>r dup f* 3.14159e f* ;
        5.0e <circle>
        {n} 0 do
            dup area-g drop
        loop
        drop
    "#, n = n);
    let gdur = run_single(&s, &mut ctx, &src_g);

    let (s2, _out, mut ctx2) = fresh();
    let src_d = format!(r#"
        CLASS: circle2  SLOT: r  ;
        : area-direct ( c -- a )
            circle2>r dup f* 3.14159e f* ;
        5.0e <circle2>
        {n} 0 do
            dup area-direct drop
        loop
        drop
    "#, n = n);
    let ddur = run_single(&s2, &mut ctx2, &src_d);

    let g_ns = gdur.as_nanos();
    let d_ns = ddur.as_nanos();
    let ratio = g_ns as f64 / d_ns.max(1) as f64;
    eprintln!("──── dispatch vs direct ────");
    eprintln!("  N         = {n}");
    eprintln!("  generic   = {} µs total ({} ns/iter)",
        g_ns / 1_000, g_ns / n as u128);
    eprintln!("  direct `:` = {} µs total ({} ns/iter)",
        d_ns / 1_000, d_ns / n as u128);
    eprintln!("  ratio     = {ratio:.2}× (generic ÷ direct)");
    eprintln!("───────────────────────────");
    // Includes loop overhead, `dup`, the call, and `drop`.  Both
    // paths pay all of those; only the call itself differs.  Allow
    // up to 5× — anything worse would indicate the IC isn't kicking
    // in.  Note this is a *whole eval* wall clock, not per-call,
    // so first-eval setup-cost dominates the small loops.
    assert!(ratio < 5.0,
        "generic {ratio:.1}× slower than direct — IC probably stuck");
}

/// Polymorphic call site: the SAME loop body calls `area-g` on a
/// circle on odd iterations and a square on even iterations.  Tests
/// the polymorphic-inline-cache path.
#[test]
#[ignore]
fn dispatch_polymorphic() {
    let (s, out, mut ctx) = fresh();
    let _ = out;

    let n: u32 = 200_000;
    let src = format!(r#"
        CLASS: circle  SLOT: r     ;
        CLASS: square  SLOT: side  ;
        GENERIC: area-g ( s -- a )
        METHOD: area-g ( c:circle -- a )
            circle>r dup f* 3.14159e f* ;
        METHOD: area-g ( s:square -- a )
            square>side dup f* ;

        5.0e <circle> 3.0e <square>
        \ stack now ( circle square )
        {n} 0 do
            i 1 and 0 = if  over  else  dup  then  area-g drop
        loop
        2drop
    "#, n = n);
    let dur = run_single(&s, &mut ctx, &src);
    let t = dur.as_nanos();
    eprintln!("──── polymorphic dispatch ────");
    eprintln!("  N            = {n}");
    eprintln!("  median total = {} µs", t / 1_000);
    eprintln!("  per-call     = {} ns", t / n as u128);
    eprintln!("─────────────────────────────");
    // Loose ceiling — polymorphic IC should still be sub-microsecond.
    assert!(t / n as u128 <= 2_000,
        "polymorphic dispatch > 2µs/call ({} ns) — something's wrong",
        t / n as u128);
}
