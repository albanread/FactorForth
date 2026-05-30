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
        \ "a,bb,ccc" split on ',' -> 3 fields  (char-literal sugar)
        S" a,bb,ccc" >string ',' split VALUE parts
        ." n=" parts size .                          \ 3
        ." p0=" 0 parts at show                      \ a
        ." |p1=" 1 parts at show                     \ bb
        ." |p2=" 2 parts at show                     \ ccc
        \ join the same parts with '-'
        ." |joined=" parts '-' join show             \ a-bb-ccc
        \ round-trip: split then join on the same delim reproduces input
        ." |rt=" S" x:y:z" >string ':' split ':' join show   \ x:y:z
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

/// upcase-string / downcase-string apply ASCII case-flip to every
/// character of a string and return a fresh string of the SAME type.
/// They are written `' upcase-char map` — proof that strings now fit
/// the collection protocol (map's `new-like` gives a string back).
#[test]
#[ignore]
fn upcase_downcase_string() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." up=" S" Hello, World!" >string upcase-string show
        ." |down=" S" Hello, World!" >string downcase-string show
        ." |"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("up=HELLO, WORLD!"), "upcase: {cap}");
    assert!(cap.contains("down=hello, world!"), "downcase: {cap}");
}

/// trim variants drop leading / trailing / both ASCII whitespace.
/// An all-whitespace input trims to the empty string.
#[test]
#[ignore]
fn trim_variants() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." [" S"   hello  " >string trim-left  show ." ]"
        ." |[" S"   hello  " >string trim-right show ." ]"
        ." |[" S"   hello  " >string trim       show ." ]"
        ." |[" S"      " >string trim show ." ]"
        ." |[" S" notrim" >string trim show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[hello  ]"), "trim-left: {cap}");
    assert!(cap.contains("[  hello]"), "trim-right: {cap}");
    assert!(cap.contains("[hello]"), "trim: {cap}");
    assert!(cap.contains("[]"), "all-ws trim: {cap}");
    assert!(cap.contains("[notrim]"), "no-ws passthrough: {cap}");
}

/// starts-with? / ends-with? / contains? — three predicate searches
/// over the same substring-matching primitive.  Includes the edge
/// cases that bite: needle longer than haystack, needle at the very
/// last position, repeated overlapping matches.
#[test]
#[ignore]
fn substring_predicates() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        S" hello world" >string VALUE h
        ." sw1=" h S" hello" >string starts-with? .         \ -1
        ." sw0=" h S" world" >string starts-with? .         \ 0
        ." ew1=" h S" world" >string ends-with? .           \ -1
        ." ew0=" h S" hello" >string ends-with? .           \ 0
        ." c1="  h S" o w"   >string contains? .            \ -1
        ." c0="  h S" zzz"   >string contains? .            \ 0
        ." long=" h S" hello world!" >string contains? .    \ 0  (needle longer)
        ." last=" h S" world" >string contains? .           \ -1 (match at end)
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("sw1=-1"), "starts-with? true: {cap}");
    assert!(cap.contains("sw0=0"),  "starts-with? false: {cap}");
    assert!(cap.contains("ew1=-1"), "ends-with? true: {cap}");
    assert!(cap.contains("ew0=0"),  "ends-with? false: {cap}");
    assert!(cap.contains("c1=-1"),  "contains? mid: {cap}");
    assert!(cap.contains("c0=0"),   "contains? miss: {cap}");
    assert!(cap.contains("long=0"), "contains? needle too long: {cap}");
    assert!(cap.contains("last=-1"),"contains? at last position: {cap}");
}

/// pad-left / pad-right grow a string up to a width with a fill char.
/// Width less than the string's own size is a no-op (no truncation).
#[test]
#[ignore]
fn pad_variants() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." [" S" 42" >string 5 '0' pad-left show ." ]"
        ." |[" S" hi" >string 6 ' ' pad-right show ." ]"
        ." |[" S" already-long" >string 3 'x' pad-left show ." ]"
        ." |[" S" already-long" >string 3 'x' pad-right show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[00042]"), "pad-left zero-pad: {cap}");
    assert!(cap.contains("[hi    ]"), "pad-right space-pad: {cap}");
    assert!(cap.contains("[already-long]"), "pad-left no-op when wide enough: {cap}");
}

/// repeat-char / repeat-string build a string of n copies.  n <= 0
/// gives the empty string.
#[test]
#[ignore]
fn repeat_variants() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." [" '-' 5 repeat-char show ." ]"
        ." |[" S" ab" >string 3 repeat-string show ." ]"
        ." |[" '*' 0 repeat-char show ." ]"
        ." |[" S" x" >string -2 repeat-string show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[-----]"), "repeat-char: {cap}");
    assert!(cap.contains("[ababab]"), "repeat-string: {cap}");
    // Two empty-string brackets: zero count, and negative count.
    let empties = cap.matches("[]").count();
    assert!(empties >= 2, "empty cases: {cap}");
}

/// A string is a fully-fledged collection now: map gives back a
/// string, reverse gives back a string, and the predicate-search
/// algorithms from collections.f work directly on chars.
#[test]
#[ignore]
fn string_is_a_collection() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." rev=" S" abcde" >string reverse show              \ edcba
        ." |tally=" S" Hello" >string ' char-upper? tally .   \ 1
        ." |any=" S" abc1" >string ' digit-char? any? .       \ -1
        ." |all=" S" abcd" >string ' letter-char? all? .      \ -1
        ." |"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("rev=edcba"), "reverse on string: {cap}");
    assert!(cap.contains("tally=1"),   "tally on string: {cap}");
    assert!(cap.contains("any=-1"),    "any? on string: {cap}");
    assert!(cap.contains("all=-1"),    "all? on string: {cap}");
}

/// `n>string` renders any number — int (in current base) or float —
/// as a fresh string of our `string` class.  The round-trip with
/// `s>n` is the canonical test.
#[test]
#[ignore]
fn n_to_string_and_back() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." [" 42 n>string show ." ]"
        ." |[" -17 n>string show ." ]"
        ." |[" 0 n>string show ." ]"
        \ Round-trip: s>n . n>string . show
        ." |rt=" S" 12345" >string s>n . n>string show
        \ Failure: leading garbage
        ." |fail=" S" abc" >string s>n . .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[42]"), "positive: {cap}");
    assert!(cap.contains("[-17]"), "negative: {cap}");
    assert!(cap.contains("[0]"), "zero: {cap}");
    // s>n leaves ( n -1 ); first `.` prints flag, second `n>string`
    // re-renders the number.
    assert!(cap.contains("rt=-1 12345"), "round-trip: {cap}");
    // failure leaves ( 0 0 ); two `.`s print "0 " each.
    assert!(cap.contains("fail=0 0"), "parse failure: {cap}");
}

/// `s>n`'s flag distinguishes a successfully parsed zero from
/// "couldn't parse" — the whole point of the two-return shape.
#[test]
#[ignore]
fn s_to_n_zero_vs_failure() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        \ "0" parses to 0 with flag -1.
        ." z=" S" 0" >string s>n .   .   \ flag then n
        \ "" fails; both returns are zero.
        ." e=" S" " >string s>n .  .
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("z=-1 0"), "zero parse: {cap}");
    assert!(cap.contains("e=0 0"), "empty parse fails: {cap}");
}

/// `to-string` captures any `show` output into a fresh string —
/// the same `show` that prints to stdout, just with the bytes
/// caught instead of released.  Composes with every text utility.
#[test]
#[ignore]
fn to_string_captures_show_output() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        \ A string captured back as a string is the same string.
        ." [" S" hello" >string to-string show ." ]"
        \ A user class with a custom show.
        CLASS: point SLOT: x SLOT: y ;
        METHOD: show ( p:point -- )
            ." (" dup point>x . ." ," point>y . ." )" ;
        ." |[" 3 4 <point> to-string show ." ]"
        \ Capture composes with text utilities — pad the rendered form.
        ." |[" 42 to-string 6 '0' pad-left show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[hello]"), "string capture: {cap}");
    assert!(cap.contains("[(3 ,4 )]"), "point capture: {cap}");
    assert!(cap.contains("[000042]"), "captured + padded: {cap}");
}

/// `capture-with` lets the caller pick the rendering word — `.`
/// for raw print, `dump` for debug detail, anything that writes to
/// the output stream and consumes one value.
#[test]
#[ignore]
fn capture_with_custom_renderer() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        \ `.` prints "42 " (with trailing space) — capture confirms.
        ." [" 42 ' . capture-with show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[42 ]"), "capture-with .: {cap}");
}

/// `format1` substitutes one `{}` marker with a value rendered via
/// `to-string`.  The same `to-string` that routes ints through
/// `n>string` and user classes through `show`.
#[test]
#[ignore]
fn format1_substitutes_one_value() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." [" S" Hello, {}!" >string  S" world" >string  format1  show ." ]"
        ." |[" S" answer = {}" >string  42  format1  show ." ]"
        ." |[" S" pi ≈ {}" >string  3.14e  format1  show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[Hello, world!]"), "string sub: {cap}");
    assert!(cap.contains("[answer = 42]"), "int sub: {cap}");
    assert!(cap.contains("[pi ≈ 3.14]"), "float sub: {cap}");
}

/// `format2` / `format3` for the two- and three-value common cases.
/// Values are consumed in left-to-right order across `{}` markers.
#[test]
#[ignore]
fn format2_and_format3() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        ." [" S" {} + {} = {}" >string  2  3  5  format3  show ." ]"
        ." |[" S" {}-{}" >string  S" alpha" >string  S" beta" >string  format2  show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[2 + 3 = 5]"), "format3 arithmetic: {cap}");
    assert!(cap.contains("[alpha-beta]"), "format2 strings: {cap}");
}

/// `format` (the N-ary form) takes a darray of values; perfect for
/// programmatically built argument lists.
#[test]
#[ignore]
fn format_with_explicit_darray() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        new-darray VALUE args
        S" red" >string args d-push
        S" green" >string args d-push
        S" blue" >string args d-push
        ." [" S" rgb({},{},{})" >string args format show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[rgb(red,green,blue)]"), "format N-ary: {cap}");
}

/// A user class with a custom `show` method renders through
/// `to-string`, so `format` Just Works on it — protocol all the
/// way through.
#[test]
#[ignore]
fn format_renders_user_classes() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        CLASS: point SLOT: x SLOT: y ;
        METHOD: show ( p:point -- )
            ." (" dup point>x . ." ," point>y . ." )" ;
        ." [" S" pos={}" >string  3 4 <point>  format1  show ." ]"
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[pos=(3 ,4 )]"), "format user class: {cap}");
}

/// Build a fresh path under the system temp directory.  Returns
/// the path as a string Forth can `S" ... "` into, with backslashes
/// escaped for the Forth literal.  Each test gets its own unique
/// path so they don't fight over the same file.
fn temp_path(stem: &str) -> String {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    p.push(format!("nf-{}-{}-{}.tmp", stem, pid, nanos));
    let s = p.to_string_lossy().to_string();
    // Forth `S" ... "` accepts UTF-8 bytes literally; backslashes in
    // Windows paths aren't escape chars in S" — they pass through.
    s
}

/// `spit-file` then `slurp-file` round-trips the contents byte-for-
/// byte.  Uses a unique temp path so tests don't collide.
#[test]
#[ignore]
fn spit_then_slurp_roundtrips() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    let path = temp_path("roundtrip");
    let src = format!(r#"
        S" hello, world!" >string  S" {path}" >string  spit-file
        ." [" S" {path}" >string slurp-file show ." ]"
    "#);
    run(&s, &mut ctx, &src);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("[hello, world!]"), "round-trip: {cap}");
    // Clean up — tests share the temp dir so we don't want to litter.
    let _ = std::fs::remove_file(&path);
}

/// `file-exists?` distinguishes a missing path from one that's been
/// written.  Test against a path we know doesn't exist, then write,
/// then ask again.
#[test]
#[ignore]
fn file_exists_before_and_after_write() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    let path = temp_path("exists");
    // Make sure it doesn't exist (defensive — previous run cleanup
    // might have failed).
    let _ = std::fs::remove_file(&path);
    let src = format!(r#"
        ." pre=" S" {path}" >string file-exists? .
        S" data" >string  S" {path}" >string  spit-file
        ." |post=" S" {path}" >string file-exists? .
    "#);
    run(&s, &mut ctx, &src);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("pre=0"), "missing should be 0: {cap}");
    assert!(cap.contains("post=-1"), "present should be -1: {cap}");
    let _ = std::fs::remove_file(&path);
}

/// `file-lines` reads a file as a darray of strings, one per line.
/// `write-lines` is its inverse; the round-trip preserves content
/// (with the caveat that an empty trailing field appears if the
/// file ends with a newline — same convention as `split`).
#[test]
#[ignore]
fn file_lines_round_trip() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    let path = temp_path("lines");
    let src = format!(r#"
        \ Build three lines, write, read back, show.
        new-darray VALUE ls
        S" alpha" >string ls d-push
        S" beta"  >string ls d-push
        S" gamma" >string ls d-push
        ls S" {path}" >string write-lines
        \ Now read it back and verify each line.
        S" {path}" >string file-lines VALUE got
        ." n=" got size .
        ." |0=" 0 got at show
        ." |1=" 1 got at show
        ." |2=" 2 got at show
    "#);
    run(&s, &mut ctx, &src);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("n=3"), "three lines: {cap}");
    assert!(cap.contains("0=alpha"), "line 0: {cap}");
    assert!(cap.contains("1=beta"), "line 1: {cap}");
    assert!(cap.contains("2=gamma"), "line 2: {cap}");
    let _ = std::fs::remove_file(&path);
}

/// Floats round-trip too: n>string handles them via Factor's float
/// formatting, s>n handles standard float syntax (decimal point,
/// exponent).
#[test]
#[ignore]
fn n_to_string_floats() {
    let (s, out, mut ctx) = fresh();
    load_layers(&s, &mut ctx);
    run(&s, &mut ctx, r#"
        \ Render then parse: the parsed value should equal the original.
        ." pi=" 3.14e n>string show
        ." |back=" S" 3.14" >string s>n . .   \ flag then value
    "#);
    let cap = captured(&out);
    eprintln!("captured: {cap:?}");
    assert!(cap.contains("pi=3.14"), "float render: {cap}");
    // s>n flag then value: "back=-1 3.14"
    assert!(cap.contains("back=-1 3.14"), "float parse: {cap}");
}
