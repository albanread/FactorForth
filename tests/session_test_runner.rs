//! tests/session_test_runner.rs — M3.0.3 (#41) Forth-2012-style test runner.
//!
//! Treats `.fs` / `.fr` / `.fth` files as DATA, not as Forth-side
//! programs that we evaluate as one big chunk.  The Forth 2012
//! canonical tester (Hayes / Gerry Jackson) depends on `SOURCE`,
//! `>IN`, `?DUP`, `[CHAR]`, `IMMEDIATE` — Forth's interactive
//! parser-state introspection.  NewFactor's compile-then-eval
//! pipeline can't emulate that cleanly.
//!
//! Instead:
//!
//!   1. Rust reads the test file and extracts every
//!      `T{ <code> -> <expected> }T` block.
//!   2. For each assertion, Rust drives TWO evals against a
//!      shared Session:
//!        a) `<code>` followed by a stack-dumping tail that
//!           prints all data-stack values.
//!        b) `<expected>` with the same tail.
//!      Both outputs (trimmed) are compared as text.
//!   3. The 20s per-eval watchdog fires `std::process::abort()`
//!      if any assertion hangs.
//!   4. Pass / fail / nyimp counts are tallied in Rust and
//!      asserted by the harness.
//!
//! `nyimp` (not yet implemented): when an assertion can't compile
//! because NewFactor's resolver doesn't know one of the words it
//! uses.  Distinguishes "we know the answer is wrong" from "we
//! haven't built this part yet" — the latter is FINE for a
//! work-in-progress conformance run.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::session::{IoMode, Session, SessionOpts};

// ── Assertion extraction ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Assertion {
    code:     String,
    expected: String,
    line:     usize,    // 1-based source line where T{ appears
}

/// Strip Forth comments from source so the T{ }T scanner doesn't
/// trip over `T{` appearing inside a `\` line comment or `(...)`
/// stack-effect comment.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_paren = false;
    while let Some(c) = chars.next() {
        if in_paren {
            // ANS `( ... )` block comment — consume to closing `)`.
            // Comments don't nest in ANS, so first ')' ends it.
            if c == ')' {
                in_paren = false;
            }
            // Preserve newlines for line-number tracking.
            if c == '\n' { out.push('\n'); }
            continue;
        }
        if c == '\\' {
            // Line comment if next is whitespace OR end-of-line OR EOF.
            // ANS rule: `\` must be space-delimited.
            let space_after = chars.peek().map(|n| n.is_whitespace()).unwrap_or(true);
            if space_after {
                // Skip to end of line.
                while let Some(&n) = chars.peek() {
                    if n == '\n' { break; }
                    chars.next();
                }
                continue;
            } else {
                out.push(c);
                continue;
            }
        }
        if c == '(' {
            // Check for `( ` (space after) to confirm comment.
            let space_after = chars.peek().map(|n| n.is_whitespace()).unwrap_or(true);
            if space_after {
                in_paren = true;
                continue;
            } else {
                out.push(c);
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Pull every `T{ <code> -> <expected> }T` assertion out of the
/// (already comment-stripped) source.  Space-delimited tokens
/// for delimiters, case-insensitive on `t{` / `}t` / `->`.
fn extract_assertions(src: &str) -> Vec<Assertion> {
    let stripped = strip_comments(src);
    let mut assertions = Vec::new();

    // Walk token-by-token via simple whitespace split, tracking
    // line numbers as we go for diagnostics.  Multi-line T{ }T
    // blocks are common in test files.
    let mut tokens: Vec<(usize, String)> = Vec::new();
    let mut line = 1usize;
    let mut tok = String::new();
    for c in stripped.chars() {
        if c == '\n' {
            if !tok.is_empty() {
                tokens.push((line, std::mem::take(&mut tok)));
            }
            line += 1;
        } else if c.is_whitespace() {
            if !tok.is_empty() {
                tokens.push((line, std::mem::take(&mut tok)));
            }
        } else {
            tok.push(c);
        }
    }
    if !tok.is_empty() {
        tokens.push((line, tok));
    }

    let lc = |s: &str| s.to_ascii_lowercase();
    let mut i = 0;
    while i < tokens.len() {
        if lc(&tokens[i].1) == "t{" {
            let start_line = tokens[i].0;
            // Find matching `->` and `}t`.
            let mut arrow: Option<usize> = None;
            let mut end:   Option<usize> = None;
            let mut j = i + 1;
            while j < tokens.len() {
                let tname = lc(&tokens[j].1);
                if tname == "}t" { end = Some(j); break; }
                if tname == "->" && arrow.is_none() { arrow = Some(j); }
                j += 1;
            }
            if let (Some(a), Some(e)) = (arrow, end) {
                let code: String = tokens[i + 1 .. a].iter()
                    .map(|t| t.1.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                let expected: String = tokens[a + 1 .. e].iter()
                    .map(|t| t.1.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                assertions.push(Assertion { code, expected, line: start_line });
                i = e + 1;
                continue;
            }
            // Unmatched T{ — skip past it to avoid an infinite loop.
            i += 1;
        } else {
            i += 1;
        }
    }
    assertions
}

// ── Per-assertion execution ─────────────────────────────────────────────────

#[derive(Debug)]
enum Outcome {
    Pass,
    Fail {
        actual:   String,
        expected: String,
    },
    Nyimp {
        which:    Side,
        message:  String,
    },
}

#[derive(Debug, Clone, Copy)]
enum Side { Code, Expected }

/// Tail appended to both `<code>` and `<expected>` to dump the
/// resulting data stack as space-separated `.`-formatted numbers.
/// Both sides run through the same tail so the comparison is
/// apples-to-apples.
const DUMP_TAIL: &str = "\nbegin depth 0> while . repeat\n";

/// Compile + eval a snippet on the given session.  On compile
/// failure (which most often means an unknown word), returns
/// Err with the message.  On eval success, returns the captured
/// output (which the caller will compare).
fn try_eval(
    session: &Session,
    output:  &Arc<Mutex<Vec<u8>>>,
    src:     &str,
) -> Result<String, String> {
    output.lock().unwrap().clear();
    let ir = newfactor::compiler::compile(src).map_err(|e| format!("{e:?}"))?;
    session.eval(&ir).map_err(|e| format!("eval: {e}"))?;
    let captured = String::from_utf8_lossy(&output.lock().unwrap())
        .into_owned();
    Ok(captured.trim().to_string())
}

fn run_assertion(
    session: &Session,
    output:  &Arc<Mutex<Vec<u8>>>,
    a:       &Assertion,
) -> Outcome {
    let code_src = format!("{}{}", a.code, DUMP_TAIL);
    let actual = match try_eval(session, output, &code_src) {
        Ok(s) => s,
        Err(m) => return Outcome::Nyimp { which: Side::Code, message: m },
    };

    let exp_src = format!("{}{}", a.expected, DUMP_TAIL);
    let expected = match try_eval(session, output, &exp_src) {
        Ok(s) => s,
        Err(m) => return Outcome::Nyimp { which: Side::Expected, message: m },
    };

    if actual == expected {
        Outcome::Pass
    } else {
        Outcome::Fail { actual, expected }
    }
}

// ── Driver ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Summary {
    pass:  usize,
    fail:  usize,
    nyimp: usize,
}

fn run_test_file_data(file_label: &str, contents: &str) -> Summary {
    let assertions = extract_assertions(contents);
    eprintln!("─── {file_label}: {} assertions extracted ───",
              assertions.len());

    let output = Arc::new(Mutex::new(Vec::new()));
    let opts = SessionOpts::defaults_for_crate(IoMode::Test {
        input: vec![],
        output: output.clone(),
    });
    let session = Session::new(opts).expect("Session::new");

    let mut s = Summary::default();
    for a in &assertions {
        match run_assertion(&session, &output, a) {
            Outcome::Pass => {
                s.pass += 1;
            }
            Outcome::Fail { actual, expected } => {
                s.fail += 1;
                eprintln!(
                    "  FAIL  line {}: T{{ {} -> {} }}T\n         expected stack: {expected:?}\n         actual stack:   {actual:?}",
                    a.line, a.code, a.expected,
                );
            }
            Outcome::Nyimp { which, message } => {
                s.nyimp += 1;
                eprintln!(
                    "  NYIMP line {}: T{{ {} -> {} }}T  ({:?} side: {})",
                    a.line, a.code, a.expected, which,
                    message.lines().next().unwrap_or(""),
                );
            }
        }
    }

    eprintln!(
        "─── {file_label}: PASS={} FAIL={} NYIMP={} of {} ───",
        s.pass, s.fail, s.nyimp, assertions.len(),
    );
    s
}

fn run_test_file(file_label: &str, path: &str) -> Summary {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"));
    run_test_file_data(file_label, &contents)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn smoke_one_assertion_passes() {
    // Smoke: a single inline T{}T against the runner machinery.
    let summary = run_test_file_data(
        "smoke",
        "T{ 2 3 + -> 5 }T",
    );
    assert_eq!(summary.pass, 1, "expected 1 pass, got {summary:?}");
    assert_eq!(summary.fail, 0);
    assert_eq!(summary.nyimp, 0);
}

#[test]
#[ignore]
fn smoke_one_assertion_fails() {
    // Deliberately wrong expected value.
    let summary = run_test_file_data(
        "smoke-fail",
        "T{ 2 3 + -> 99 }T",
    );
    assert_eq!(summary.pass, 0);
    assert_eq!(summary.fail, 1, "expected 1 fail, got {summary:?}");
}

#[test]
#[ignore]
fn smoke_nyimp_unknown_word() {
    // `foobar-undefined` won't resolve → nyimp.
    let summary = run_test_file_data(
        "smoke-nyimp",
        "T{ foobar-undefined -> 0 }T",
    );
    assert_eq!(summary.pass, 0);
    assert_eq!(summary.fail, 0);
    assert_eq!(summary.nyimp, 1, "expected 1 nyimp, got {summary:?}");
}

#[test]
#[ignore]
fn comment_stripping_does_not_eat_T_brackets() {
    let summary = run_test_file_data(
        "comments",
        r#"\ a line comment with T{ inside that shouldn't trigger
( a paren comment with T{ also ignored )
T{ 1 2 + -> 3 }T
\ another T{ in a comment
T{ 4 5 + -> 9 }T"#,
    );
    assert_eq!(summary.pass, 2, "expected 2 passes, got {summary:?}");
    assert_eq!(summary.fail, 0);
    assert_eq!(summary.nyimp, 0);
}

#[test]
#[ignore]
fn multi_line_assertion() {
    let summary = run_test_file_data(
        "multiline",
        r#"
T{
    1 2 3
    swap
->
    1 3 2
}T
"#,
    );
    assert_eq!(summary.pass, 1, "expected 1 pass, got {summary:?}");
}

#[test]
#[ignore]
fn corpus_ans_core_assertions() {
    // The big-picture goal: run a batch of canonical ANS Core
    // T{}T tests against NewFactor and watch how many pass.
    // This corpus is hand-curated (a subset of what core.fr
    // covers, restricted to words NewFactor currently ships).
    let summary = run_test_file(
        "ans-core-corpus",
        &format!(
            "{}/tests/fixtures/ans-core-corpus.fs",
            env!("CARGO_MANIFEST_DIR"),
        ).replace('\\', "/"),
    );
    eprintln!("=== ANS core corpus final: {summary:?} ===");
    // Expectation: zero failures and zero nyimp on words we
    // already ship.  NYIMP > 0 means we forgot to add a word.
    assert_eq!(summary.fail, 0,
        "ANS core corpus had {} failures — see eprintln above",
        summary.fail);
    assert_eq!(summary.nyimp, 0,
        "ANS core corpus had {} nyimps — missing resolver entries?",
        summary.nyimp);
    assert!(summary.pass > 30,
        "ANS core corpus expected >30 passes, got {}", summary.pass);
}
