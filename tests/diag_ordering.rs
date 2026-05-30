//! CoreProtocols ordering protocol — `cmp` (Layer 0) and the ordered
//! algorithms `min-of` / `max-of` / `sorted?` / `sort` (Layer 1).
//!
//! Loads the libraries the way user code would (core.f, then
//! collections.f) and exercises three-way comparison, the derived
//! ordering words, and the algorithms written once over the
//! collection + ordering protocols.  VM-backed, so `#[ignore]`d like
//! the other diag suites — run with `--ignored` against the embedded
//! Factor build.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

const CORE: &str = include_str!("../release/factorforth/lib/core.f");
const COLLECTIONS: &str = include_str!("../release/factorforth/lib/collections.f");

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
}

/// `cmp` is three-way: negative / zero / positive, with the derived
/// `before?` / `after?` / `lesser` / `greater` reading off it.
#[test]
#[ignore]
fn cmp_and_derived() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." gt="  5 3 cmp .            \ 1   (5 sorts after 3)
        ." |lt="  3 5 cmp .           \ -1
        ." |eq="  3 3 cmp .           \ 0
        ." |bT="  3 5 before? .       \ -1  (3 before 5)
        ." |bF="  5 3 before? .       \ 0
        ." |aT="  5 3 after? .        \ -1
        ." |les=" 5 3 lesser .        \ 3
        ." |grt=" 5 3 greater .       \ 5
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("gt=1"), "5>3: {cap}");
    assert!(cap.contains("lt=-1"), "3<5: {cap}");
    assert!(cap.contains("eq=0"), "3==3: {cap}");
    assert!(cap.contains("bT=-1") && cap.contains("bF=0"), "before?: {cap}");
    assert!(cap.contains("aT=-1"), "after?: {cap}");
    assert!(cap.contains("les=3"), "lesser: {cap}");
    assert!(cap.contains("grt=5"), "greater: {cap}");
}

/// min-of / max-of fold `lesser` / `greater` over a collection.
#[test]
#[ignore]
fn min_max_of_collection() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        3 xs d-push  1 xs d-push  4 xs d-push  1 xs d-push  5 xs d-push
        ." min=" xs min-of .          \ 1
        ." |max=" xs max-of .         \ 5
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("min=1"), "min-of: {cap}");
    assert!(cap.contains("max=5"), "max-of: {cap}");
}

/// `sorted?` detects order; `sort` establishes it in place.
#[test]
#[ignore]
fn sort_orders_in_place() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        3 xs d-push  1 xs d-push  2 xs d-push
        ." before=" xs sorted? .      \ 0    (3 1 2 is not sorted)
        xs sort
        ." |after=" xs sorted? .      \ -1   (now it is)
        ." |e0=" 0 xs at .            \ 1
        ." |e1=" 1 xs at .            \ 2
        ." |e2=" 2 xs at .            \ 3
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("before=0"), "unsorted detected: {cap}");
    assert!(cap.contains("after=-1"), "sorted after sort: {cap}");
    assert!(cap.contains("e0=1") && cap.contains("e1=2") && cap.contains("e2=3"),
        "elements in order: {cap}");
}

/// A single-element / empty collection is vacuously sorted.
#[test]
#[ignore]
fn trivially_sorted() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        new-darray VALUE one
        42 one d-push
        ." one=" one sorted? .        \ -1
        one sort                       \ no-op, must not crash
        ." |still=" one sorted? .     \ -1
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("one=-1") && cap.contains("still=-1"),
        "single-element collection is sorted: {cap}");
}
