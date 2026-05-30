//! CoreProtocols — the standard class library (Layer 0 onward).
//!
//! The library source ships as `release/factorforth/lib/*.f`, written
//! in ordinary ANS Forth on the object system.  These tests load a
//! layer's source and exercise its protocol the way user code would.

#![cfg(target_os = "windows")]

use std::sync::{Arc, Mutex};
use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

/// Library source, embedded from the shipped files so the tests and
/// the release artifacts never drift.
const CORE: &str = include_str!("../release/factorforth/lib/core.f");
const COLLECTIONS: &str = include_str!("../release/factorforth/lib/collections.f");
const OTHELLO: &str = include_str!("../release/factorforth/lib/othello.f");

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

/// Compile + eval a source string, panicking on a compile error.
fn run(s: &Session, ctx: &mut CompileContext, src: &str) {
    let ir = compile_in_context(src, ctx).unwrap_or_else(|e| panic!("compile: {e}"));
    eprintln!("IR:\n{ir}");
    s.eval(&ir).expect("eval");
}

/// A class that implements `show` gets its own rendering; calling
/// `show` dispatches to it.
#[test]
#[ignore]
fn show_dispatches_to_class_method() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: point SLOT: x SLOT: y ;
        METHOD: show ( p:point -- )
            ." (" dup point>x . ." ," point>y . ." )" ;

        3 4 <point> show
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // `.` prints "n " (trailing space), so the rendering is "(3 ,4 )".
    assert!(cap.contains("(3 ,4 )"), "point show: {cap}");
}

/// A type with no `show` method falls back to the object catch-all,
/// so `show` is total — it never fails to dispatch.
#[test]
#[ignore]
fn show_object_default_is_total() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: widget ;
        <widget> show
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("<object>"), "object default: {cap}");
}

/// `show-ln` is defined once over the generic and works for any class
/// that implements `show` — protocol reuse, not per-class code.
#[test]
#[ignore]
fn show_ln_reuses_the_protocol() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: tag SLOT: n ;
        METHOD: show ( t:tag -- )  ." #" tag>n . ;

        5 <tag> show-ln
        9 <tag> show-ln
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // `show-ln` ran the per-class `show` twice, each on its own line.
    assert!(cap.contains("#5"), "tag1: {cap}");
    assert!(cap.contains("#9"), "tag2: {cap}");
    assert_eq!(cap.matches('#').count(), 2, "two shows: {cap}");
}

/// `equals?` is an open protocol: its default is structural/numeric
/// equality, but a class can override it — and `member?` (Layer 1)
/// dispatches through it, so value search honours the class's own rule.
#[test]
#[ignore]
fn equals_override_drives_member_search() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ default equality: by value
        ." n1=" 5 5 equals? .        \ -1
        ." n2=" 5 6 equals? .        \ 0

        \ a class whose equality is its id slot only (balance ignored)
        CLASS: account SLOT: id SLOT: balance ;
        METHOD: equals? ( a b:account -- ? )
            account>id swap account>id = ;

        7 100 <account> VALUE a1
        7 999 <account> VALUE a2      \ same id, different balance
        ." same=" a1 a2 equals? .     \ -1 (equal by id)

        \ member? rides equals?, so a same-id account counts as present
        new-darray VALUE accts
        a1 accts d-push
        8 50 <account> VALUE a3       \ probe: id 8, never pushed
        ." in1=" a2 accts member? .   \ -1 (a2 matches a1 by id)
        ." in2=" a3 accts member? .   \ 0  (no id-8 account present)
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n1=-1") && cap.contains("n2=0"), "default equals?: {cap}");
    assert!(cap.contains("same=-1"), "override compares by id: {cap}");
    assert!(cap.contains("in1=-1"), "member? honours equals? override: {cap}");
    assert!(cap.contains("in2=0"), "member? absent by id: {cap}");
}

/// `clone` (Layer 0) gives an independent copy.  The default is shallow,
/// but grid/darray override it to copy their backing — so mutating a
/// clone never touches the original.
#[test]
#[ignore]
fn clone_is_independent_for_collections() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ grid: clone, then mutate the copy — original must not change
        2 2 new-grid VALUE g
        5  0 0 g at-xy!
        g clone VALUE g2
        99 0 0 g2 at-xy!            \ scribble on the copy
        ." g="  0 0 g  at-xy .      \ 5  (original untouched)
        ." g2=" 0 0 g2 at-xy .      \ 99 (copy changed)

        \ darray: clone, push to the copy — original size unchanged
        new-darray VALUE xs
        1 xs d-push  2 xs d-push
        xs clone VALUE ys
        3 ys d-push                 \ grow only the copy
        ." xn=" xs size .           \ 2
        ." yn=" ys size .           \ 3
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("g=5"), "original grid cell unchanged: {cap}");
    assert!(cap.contains("g2=99"), "cloned grid cell changed: {cap}");
    assert!(cap.contains("xn=2"), "original darray length unchanged: {cap}");
    assert!(cap.contains("yn=3"), "cloned darray grew independently: {cap}");
}

/// `clone`'s default copies a value-like class's slots, and numbers
/// clone to an equal value.
#[test]
#[ignore]
fn clone_default_copies_slots() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, r#"
        CLASS: point SLOT: x SLOT: y ;
        3 4 <point> VALUE p
        p clone VALUE q
        \ the copy reads the same slot values
        ." qx=" q point>x .         \ 3
        ." qy=" q point>y .         \ 4
        \ numbers clone to an equal value
        ." n=" 42 clone .           \ 42
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("qx=3") && cap.contains("qy=4"), "clone copies slots: {cap}");
    assert!(cap.contains("n=42"), "number clones to equal value: {cap}");
}

// ── Layer 1: grid ───────────────────────────────────────────────

/// A grid stores and retrieves cells by (x, y), 0-based.  Write a
/// few cells, read them back.
#[test]
#[ignore]
fn grid_stores_and_reads_by_xy() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ a 3-wide, 2-tall grid, held in a VALUE for clean access
        3 2 new-grid VALUE board

        \ set (0,0)=11, (2,0)=22, (1,1)=33
        11  0 0 board at-xy!
        22  2 0 board at-xy!
        33  1 1 board at-xy!

        \ read them back, in order
        0 0 board at-xy .
        2 0 board at-xy .
        1 1 board at-xy .
        \ an untouched cell reads 0
        1 0 board at-xy .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("11 ") && cap.contains("22 ") && cap.contains("33 "),
        "stored cells read back: {cap}");
    // the four `.` outputs, in order: 11 22 33 0
    assert!(cap.contains("11 22 33 0"), "in (x,y) order incl untouched=0: {cap}");
}

/// in-bounds? is 0-based and (x,y): valid columns are 0..w, rows
/// 0..h; negatives and over-edge are out.
#[test]
#[ignore]
fn grid_in_bounds_is_zero_based_xy() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        3 2 new-grid          \ x in 0..2, y in 0..1

        dup 0 0 rot in-bounds? .   \ -1  (origin, in)
        dup 2 1 rot in-bounds? .   \ -1  (far corner, in)
        dup 3 0 rot in-bounds? .   \  0  (x == w, out)
        dup 0 2 rot in-bounds? .   \  0  (y == h, out)
        -1 0 rot in-bounds? .      \  0  (negative x, out)
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // -1 -1 0 0 0  (ANS true is -1, false is 0)
    assert!(cap.contains("-1 -1 0 0 0"), "bounds flags: {cap}");
}

/// darray grows on push; size/at read it back in order.
#[test]
#[ignore]
fn darray_grows_and_reads() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        10 xs d-push
        20 xs d-push
        30 xs d-push
        xs size .            \ 3
        0 xs at .           \ 10
        2 xs at .           \ 30
        \ overwrite element 1
        99 1 xs at!
        1 xs at .           \ 99
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("3 10 30 99"), "size/at/at!: {cap}");
}

/// The collection protocol is polymorphic: `size` and `at` work on a
/// grid and a darray through the same generics — write an algorithm
/// once, run it on either backing.
#[test]
#[ignore]
fn collection_protocol_is_polymorphic() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ a generic word over ANY collection: print its size
        : .size ( c -- )  size . ;

        3 2 new-grid .size      \ grid: 6 cells
        new-darray
        dup 5 swap d-push
        dup 6 swap d-push
        .size                   \ darray: 2 elements
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // 6 from the grid, 2 from the darray — same `.size` word, two types
    assert!(cap.contains("6 2"), "polymorphic size over grid + darray: {cap}");
}

/// `each` runs an xt over every element — defined once over the
/// protocol, so it works on a darray and a grid alike.  Here the xt
/// is the builtin `.` (print), gathered with `'`.
#[test]
#[ignore]
fn each_iterates_any_collection() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        2 xs d-push  3 xs d-push  4 xs d-push
        ." darray: " xs ' . each cr

        \ each also walks a grid's cells in linear (row-major) order
        2 2 new-grid VALUE g
        7  0 0 g at-xy!          \ only (0,0) is non-zero
        ." grid: " g ' . each cr
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    // darray elements in push order
    assert!(cap.contains("darray: 2 3 4"), "each over darray: {cap}");
    // grid cells linear: (0,0)=7 then three zeros
    assert!(cap.contains("grid: 7 0 0 0"), "each over grid cells: {cap}");
}

/// `each` composes with user accumulators too (xt defined in an
/// earlier eval, so it's in the dictionary when ticked — see the
/// same-compile tick-ordering note).
#[test]
#[ignore]
fn each_with_user_accumulator() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, "0 VALUE acc  : add-acc ( n -- ) acc + TO acc ;");
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        2 xs d-push  3 xs d-push  4 xs d-push
        xs ' add-acc each
        ." sum=" acc .          \ 9
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("sum=9"), "each + accumulator: {cap}");
}

/// `map` transforms every element into a fresh darray.  Written once
/// over the protocol; the transform xt is ticked from an earlier
/// eval.
#[test]
#[ignore]
fn map_transforms_into_a_darray() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, ": dbl ( n -- n2 ) 2 * ;");
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        5 xs d-push  6 xs d-push  7 xs d-push
        xs ' dbl map VALUE ys     \ doubled into a new darray
        ." len=" ys size .         \ 3
        ." vals: " ys ' . each cr  \ 10 12 14
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("len=3"), "map result size: {cap}");
    assert!(cap.contains("vals: 10 12 14"), "map doubled each: {cap}");
}

/// `map` is type-preserving: mapping a grid yields a *grid* of the same
/// dimensions (not a flat darray).  The 2-D structure survives — the
/// result reads back through at-xy and has the source's width/height.
#[test]
#[ignore]
fn map_preserves_grid_type() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, ": dbl ( n -- n2 ) 2 * ;");
    run(&s, &mut ctx, r#"
        2 2 new-grid VALUE g
        1  0 0 g at-xy!
        2  1 0 g at-xy!
        3  0 1 g at-xy!
        4  1 1 g at-xy!
        g ' dbl map VALUE g2          \ a doubled grid, same shape

        \ g2 is a grid: read it 2-dimensionally
        ." c00=" 0 0 g2 at-xy .        \ 2
        ." c11=" 1 1 g2 at-xy .        \ 8
        \ and it kept the grid's dimensions
        ." dims=" g2 grid-w . g2 grid-h .   \ 2 2
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("c00=2"), "grid map (0,0): {cap}");
    assert!(cap.contains("c11=8"), "grid map (1,1): {cap}");
    assert!(cap.contains("dims=2 2"), "result is a 2x2 grid: {cap}");
}

/// `filter` keeps the elements that satisfy a predicate, into a fresh
/// darray.  Written once over the protocol; the predicate xt is ticked
/// from an earlier eval.
#[test]
#[ignore]
fn filter_keeps_matching_elements() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, ": even? ( n -- ? ) 2 mod 0= ;");
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        1 xs d-push  2 xs d-push  3 xs d-push
        4 xs d-push  5 xs d-push  6 xs d-push
        xs ' even? filter VALUE ys   \ keep the evens into a new darray
        ." len=" ys size .            \ 3
        ." vals: " ys ' . each cr     \ 2 4 6
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("len=3"), "filter result size: {cap}");
    assert!(cap.contains("vals: 2 4 6"), "filter kept the evens: {cap}");
}

/// `fold` threads an accumulator through every element.  It's the
/// general reducer: sum is `0 ' + fold`.  Ticked builtins (`+`) work
/// as the two-in/one-out xt via call2>.
#[test]
#[ignore]
fn fold_reduces_with_an_accumulator() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        1 xs d-push  2 xs d-push  3 xs d-push  4 xs d-push
        ." sum=" xs 0 ' + fold .       \ 1+2+3+4 = 10
        \ left-to-right order matters: start at 100, subtract each
        ." sub=" xs 100 ' - fold .     \ ((((100-1)-2)-3)-4) = 90
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("sum=10"), "fold sum: {cap}");
    assert!(cap.contains("sub=90"), "fold is left-to-right: {cap}");
}

/// The predicate family — tally / any? / all? — all ride the protocol.
/// tally counts matches; any? is true if some match; all? is true if
/// every element matches (vacuously true when empty).
#[test]
#[ignore]
fn predicate_combinators() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, ": even? ( n -- ? ) 2 mod 0= ;  : big? ( n -- ? ) 3 > ;");
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        1 xs d-push  2 xs d-push  3 xs d-push
        4 xs d-push  5 xs d-push  6 xs d-push
        ." tally=" xs ' even? tally .     \ 3  (2,4,6)
        ." any="   xs ' even? any? .      \ -1 (some even)
        ." big="   xs ' big? any? .       \ -1 (4,5,6 > 3)
        ." allbig=" xs ' big? all? .      \ 0  (1,2,3 fail)

        \ a collection where every element matches
        new-darray VALUE evens
        2 evens d-push  4 evens d-push  8 evens d-push
        ." alleven=" evens ' even? all? . \ -1
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("tally=3"), "tally counts matches: {cap}");
    assert!(cap.contains("any=-1"), "any? true when some match: {cap}");
    assert!(cap.contains("big=-1"), "any? big: {cap}");
    assert!(cap.contains("allbig=0"), "all? false when one fails: {cap}");
    assert!(cap.contains("alleven=-1"), "all? true when all match: {cap}");
}

/// `find` returns the first matching element plus a found flag (so 0 is
/// a valid element, not a sentinel).  `sum`/`product` are the common
/// folds with their identity baked in.
#[test]
#[ignore]
fn find_and_numeric_reductions() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, ": even? ( n -- ? ) 2 mod 0= ;");
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        1 xs d-push  2 xs d-push  3 xs d-push
        4 xs d-push  5 xs d-push  6 xs d-push

        \ first even is 2, found
        xs ' even? find  ." found=" swap . .   \ "2 -1"

        \ nothing matches -> 0 and a false flag
        new-darray VALUE odds
        1 odds d-push  3 odds d-push  5 odds d-push
        odds ' even? find  ." none=" swap . .   \ "0 0"

        \ reductions over the same darray
        ." sum=" xs sum .          \ 21
        ." prod=" xs product .     \ 720
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("found=2 -1"), "find first match + flag: {cap}");
    assert!(cap.contains("none=0 0"), "find miss -> 0 and false: {cap}");
    assert!(cap.contains("sum=21"), "sum reduces with + : {cap}");
    assert!(cap.contains("prod=720"), "product reduces with * : {cap}");
}

/// `member?` and `index-of` search by value, comparing through Layer 0's
/// `equals?` (so they respect a class's own equality).  member? answers
/// presence; index-of gives the first position plus a found flag.
#[test]
#[ignore]
fn member_and_index_of_search_by_value() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        10 xs d-push  20 xs d-push  30 xs d-push  20 xs d-push

        ." has20=" 20 xs member? .       \ -1 (present)
        ." has99=" 99 xs member? .       \ 0  (absent)

        \ first index of 20 is 1; the duplicate at 3 is ignored
        20 xs index-of  ." at=" swap . .  \ "1 -1"
        \ a miss gives index 0 and a false flag
        99 xs index-of  ." miss=" swap . . \ "0 0"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("has20=-1"), "member? present: {cap}");
    assert!(cap.contains("has99=0"), "member? absent: {cap}");
    assert!(cap.contains("at=1 -1"), "index-of first match + flag: {cap}");
    assert!(cap.contains("miss=0 0"), "index-of miss -> 0 and false: {cap}");
}

/// `dict` — a key→value map.  set/get/overwrite/has?/del, `size`, and
/// `dict-keys` (which hands back a darray the algorithms work on).
#[test]
#[ignore]
fn dict_maps_keys_to_values() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-dict VALUE d
        111 1 d dict-set
        222 2 d dict-set
        333 1 d dict-set          \ overwrite key 1
        ." n=" d size .            \ 2  (still two keys)
        ." has1=" 1 d dict-has? .  \ -1
        ." has9=" 9 d dict-has? .  \ 0
        1 d dict-at ." v1=" swap . .   \ 333 -1 (latest value, found)
        \ keys feed the sequence algorithms; sum is order-independent
        ." ksum=" d dict-keys 0 ' + fold .   \ 1+2 = 3
        1 d dict-del
        ." after=" d size .        \ 1
        ." has1b=" 1 d dict-has? .  \ 0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n=2"), "two distinct keys: {cap}");
    assert!(cap.contains("has1=-1") && cap.contains("has9=0"), "has?: {cap}");
    assert!(cap.contains("v1=333 -1"), "overwrite + found flag: {cap}");
    assert!(cap.contains("ksum=3"), "dict-keys feeds fold: {cap}");
    assert!(cap.contains("after=1") && cap.contains("has1b=0"), "delete: {cap}");
}

/// `set` — a collection of unique values.  add (dup-safe) / has? (O(1))
/// / del, `size`, and `set-members` for iteration.
#[test]
#[ignore]
fn set_holds_unique_values() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-set VALUE st
        10 st set-add
        20 st set-add
        10 st set-add             \ duplicate — no-op
        ." n=" st size .           \ 2
        ." has10=" 10 st set-has? .   \ -1
        ." has99=" 99 st set-has? .   \ 0
        ." msum=" st set-members 0 ' + fold .   \ 10+20 = 30
        10 st set-del
        ." after=" st size .       \ 1
        ." has10b=" 10 st set-has? .  \ 0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n=2"), "duplicate add is a no-op: {cap}");
    assert!(cap.contains("has10=-1") && cap.contains("has99=0"), "membership: {cap}");
    assert!(cap.contains("msum=30"), "set-members feeds fold: {cap}");
    assert!(cap.contains("after=1") && cap.contains("has10b=0"), "delete: {cap}");
}

// ── Phase 1 capstone: text Othello ──────────────────────────────

/// The opening position renders as the standard Othello board — the
/// central four squares, everything else empty.  Proves Layer 0 +
/// Layer 1 compose into a real program.
#[test]
#[ignore]
fn othello_opening_board_renders() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, OTHELLO);
    run(&s, &mut ctx, "othello-new show-board");
    let cap = captured(&out);
    eprintln!("captured:\n{cap}");
    // rows 3 and 4 carry the centre; the rest are all dots.
    assert!(cap.contains("...OX..."), "row 3 (O X): {cap}");
    assert!(cap.contains("...XO..."), "row 4 (X O): {cap}");
    assert!(cap.contains("........"), "an empty row: {cap}");
    // 64 cells total: 60 empty dots + 2 X + 2 O
    assert_eq!(cap.matches('.').count(), 60, "60 empty cells: {cap}");
    assert_eq!(cap.matches('X').count(), 2, "2 black: {cap}");
    assert_eq!(cap.matches('O').count(), 2, "2 white: {cap}");
}

/// Playing a legal opening move flips the bracketed disc.  Black at
/// (2,3) brackets the white at (3,3) against the black at (4,3),
/// flipping it: row 3 becomes `..XXX...`, and the board goes 4 black
/// / 1 white.
#[test]
#[ignore]
fn othello_play_flips_a_disc() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, OTHELLO);
    run(&s, &mut ctx, "othello-new  2 3 black play  show-board");
    let cap = captured(&out);
    eprintln!("captured:\n{cap}");
    assert!(cap.contains("..XXX..."), "row 3 after flip: {cap}");
    assert!(cap.contains("...XO..."), "row 4 unchanged: {cap}");
    assert_eq!(cap.matches('X').count(), 4, "4 black after flip: {cap}");
    assert_eq!(cap.matches('O').count(), 1, "1 white after flip: {cap}");
}

// ── Round 1: empty? / first / last / reverse ─────────────────────

#[test]
#[ignore]
fn empty_first_last_conveniences() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE empty
        ." e=" empty empty? .
        new-darray VALUE xs
        10 xs d-push  20 xs d-push  30 xs d-push
        ." |ne=" xs empty? .
        ." |f=" xs first .
        ." |l=" xs last .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("e=-1"), "empty? on empty darray: {cap}");
    assert!(cap.contains("ne=0"), "empty? on non-empty darray: {cap}");
    assert!(cap.contains("f=10"), "first of [10,20,30]: {cap}");
    assert!(cap.contains("l=30"), "last of [10,20,30]: {cap}");
}

#[test]
#[ignore]
fn each_index_passes_position_and_element() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        100 xs d-push  200 xs d-push  300 xs d-push
        \ accumulate sum of (i * x) into acc
        0 VALUE acc
        : weigh ( i x -- )  * acc + TO acc ;
        xs ' weigh each-index
        ." acc=" acc .                            \ 0*100 + 1*200 + 2*300 = 800
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("acc=800"), "each-index weighted sum: {cap}");
}

#[test]
#[ignore]
fn map_index_uses_position_in_transform() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        10 xs d-push  20 xs d-push  30 xs d-push  40 xs d-push
        : plus ( i x -- y )  + ;                  \ y = i + x
        xs ' plus map-index VALUE ys
        ." e0=" 0 ys at .                         \ 0 + 10 = 10
        ." |e1=" 1 ys at .                        \ 1 + 20 = 21
        ." |e2=" 2 ys at .                        \ 2 + 30 = 32
        ." |e3=" 3 ys at .                        \ 3 + 40 = 43
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("e0=10") && cap.contains("e1=21")
            && cap.contains("e2=32") && cap.contains("e3=43"),
        "map-index should add index to each element: {cap}");
}

#[test]
#[ignore]
fn reduce_folds_without_explicit_seed() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        4 xs d-push  1 xs d-push  3 xs d-push  2 xs d-push  5 xs d-push
        \ sum the elements without a seed
        ." sum=" xs ' + reduce .                  \ 4+1+3+2+5 = 15
        \ minimum via `lesser` (Layer 0 ordering protocol)
        ." |min=" xs ' lesser reduce .            \ 1
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("sum=15"), "reduce + over [4,1,3,2,5]: {cap}");
    assert!(cap.contains("min=1"), "reduce lesser over [4,1,3,2,5]: {cap}");
}

#[test]
#[ignore]
fn partition_splits_into_yes_and_no() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        \ Two-output convention: pre-declare VALUE slots, then assign
        \ with TO.  (Cleaner than VARIABLE / ! and matches the
        \ accumulator pattern other tests use.)
        0 VALUE myyes
        0 VALUE myno
        new-darray VALUE xs
        1 xs d-push  2 xs d-push  3 xs d-push  4 xs d-push  5 xs d-push  6 xs d-push
        : even? ( n -- ? ) 2 mod 0= ;
        xs ' even? partition          \ stack: yes no  (no on top)
        TO myno  TO myyes
        ." y=" myyes size .                       \ 3 evens
        ." |n=" myno  size .                      \ 3 odds
        \ matching order: yes = [2,4,6], no = [1,3,5]
        ." |y0=" 0 myyes at .                     \ 2
        ." |y2=" 2 myyes at .                     \ 6
        ." |n0=" 0 myno  at .                     \ 1
        ." |n2=" 2 myno  at .                     \ 5
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("y=3") && cap.contains("n=3"), "partition sizes: {cap}");
    assert!(cap.contains("y0=2") && cap.contains("y2=6"), "yes order: {cap}");
    assert!(cap.contains("n0=1") && cap.contains("n2=5"), "no order: {cap}");
}

#[test]
#[ignore]
fn take_and_skip_slice() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        10 xs d-push  20 xs d-push  30 xs d-push  40 xs d-push  50 xs d-push
        xs 2 take VALUE first2
        xs 2 skip VALUE rest
        ." t=" first2 size .                      \ 2
        ." |t0=" 0 first2 at .                    \ 10
        ." |t1=" 1 first2 at .                    \ 20
        ." |s=" rest size .                       \ 3
        ." |s0=" 0 rest at .                      \ 30
        ." |s2=" 2 rest at .                      \ 50
        \ clamping: take more than exists, skip past end
        ." |big=" xs 99 take size .               \ 5 (all)
        ." |over=" xs 99 skip size .              \ 0
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("t=2") && cap.contains("t0=10") && cap.contains("t1=20"),
        "take first 2: {cap}");
    assert!(cap.contains("s=3") && cap.contains("s0=30") && cap.contains("s2=50"),
        "skip first 2: {cap}");
    assert!(cap.contains("big=5") && cap.contains("over=0"),
        "take/skip clamping: {cap}");
}

#[test]
#[ignore]
fn concat_appends_into_a_fresh_darray() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE a
        new-darray VALUE b
        1 a d-push  2 a d-push
        10 b d-push  20 b d-push  30 b d-push
        a b concat VALUE c
        ." n=" c size .                           \ 5
        ." |e0=" 0 c at .                         \ 1
        ." |e1=" 1 c at .                         \ 2
        ." |e2=" 2 c at .                         \ 10
        ." |e4=" 4 c at .                         \ 30
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n=5"), "concat length: {cap}");
    assert!(cap.contains("e0=1") && cap.contains("e1=2")
            && cap.contains("e2=10") && cap.contains("e4=30"),
        "concat preserves order: {cap}");
}

// ── Round 4: set algebra + dict iteration ────────────────────────

#[test]
#[ignore]
fn set_union_intersect_difference() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-set VALUE a
        new-set VALUE b
        1 a set-add  2 a set-add  3 a set-add
        2 b set-add  3 b set-add  4 b set-add
        \ union {1,2,3,4} — 4 elements
        a b set-union VALUE u
        ." u=" u size .
        \ intersect {2,3} — 2 elements
        a b set-intersect VALUE i
        ." |i=" i size .
        \ membership in intersect: 2 yes, 1 no, 4 no
        ." |i2=" 2 i set-has? .
        ." |i1=" 1 i set-has? .
        \ a\b = {1} — 1 element
        a b set-difference VALUE d
        ." |d=" d size .
        ." |d1=" 1 d set-has? .
        ." |d2=" 2 d set-has? .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("u=4"), "union size: {cap}");
    assert!(cap.contains("i=2"), "intersect size: {cap}");
    assert!(cap.contains("i2=-1") && cap.contains("i1=0"),
        "intersect membership: {cap}");
    assert!(cap.contains("d=1") && cap.contains("d1=-1") && cap.contains("d2=0"),
        "difference membership: {cap}");
}

#[test]
#[ignore]
fn subset_predicate() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-set VALUE big
        1 big set-add  2 big set-add  3 big set-add  4 big set-add
        new-set VALUE small
        2 small set-add  3 small set-add
        new-set VALUE mixed
        2 mixed set-add  9 mixed set-add
        new-set VALUE empty
        ." s=" small big subset? .                  \ -1   small ⊆ big
        ." |b=" big small subset? .                 \ 0    big NOT ⊆ small
        ." |o=" mixed big subset? .                 \ 0    mixed has 9
        ." |e=" empty big subset? .                 \ -1   empty ⊆ any
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("s=-1"), "small ⊆ big: {cap}");
    assert!(cap.contains("b=0"), "big NOT ⊆ small: {cap}");
    assert!(cap.contains("o=0"), "other (with 9) NOT ⊆ big: {cap}");
    assert!(cap.contains("e=-1"), "empty ⊆ anything (vacuous): {cap}");
}

#[test]
#[ignore]
fn set_each_walks_members() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-set VALUE s
        10 s set-add  20 s set-add  30 s set-add  40 s set-add
        0 VALUE total
        : add-total ( x -- )  total + TO total ;
        s ' add-total set-each
        ." sum=" total .                            \ 100
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("sum=100"), "set-each sum of members: {cap}");
}

#[test]
#[ignore]
fn dict_each_visits_every_pair() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-dict VALUE d
        100 1 d dict-set
        200 2 d dict-set
        300 3 d dict-set
        \ Sum keys and values separately to verify the xt sees BOTH.
        0 VALUE k-sum
        0 VALUE v-sum
        : tally-pair ( k v -- )
            swap k-sum + TO k-sum                    \ ( v ) — added k
            v-sum + TO v-sum ;                       \ added v
        d ' tally-pair dict-each
        ." k=" k-sum .                              \ 1+2+3 = 6
        ." |v=" v-sum .                             \ 100+200+300 = 600
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("k=6"), "dict-each key sum: {cap}");
    assert!(cap.contains("v=600"), "dict-each value sum: {cap}");
}

#[test]
#[ignore]
fn reverse_returns_new_collection_in_reverse() {
    let (s, out, mut ctx) = fresh();
    run(&s, &mut ctx, CORE);
    run(&s, &mut ctx, COLLECTIONS);
    run(&s, &mut ctx, r#"
        new-darray VALUE xs
        1 xs d-push  2 xs d-push  3 xs d-push  4 xs d-push
        xs reverse VALUE ys
        ." n=" ys size .
        ." |e0=" 0 ys at .
        ." |e1=" 1 ys at .
        ." |e2=" 2 ys at .
        ." |e3=" 3 ys at .
        ." |orig=" 0 xs at .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n=4"), "reverse preserves length: {cap}");
    assert!(cap.contains("e0=4") && cap.contains("e1=3")
            && cap.contains("e2=2") && cap.contains("e3=1"),
        "reverse should yield [4,3,2,1]: {cap}");
    assert!(cap.contains("orig=1"),
        "original collection must be untouched: {cap}");
}
