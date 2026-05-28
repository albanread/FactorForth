//! NEEDS — the Rust-driven, compile-time include-once directive.
//!
//! `NEEDS path` splices a file's parsed AST into the current module the
//! first time the file is seen in a session, and expands to nothing on
//! repeats (dedup keyed on the canonical path, held in CompileContext).
//!
//! The compile-level tests need no VM — they assert on the emitted IR,
//! which is where the splice/dedup is observable.  The eval-level tests
//! prove the spliced IR actually runs (gated to Windows + the Session).

use newfactor::compiler::{compile_in_context, CompileContext};

/// A repeated NEEDS in one compile splices the file once: the included
/// file's top-level marker appears a single time, and its definition is
/// present.
#[test]
fn needs_splices_once_in_one_compile() {
    let mut ctx = CompileContext::new();
    let ir = compile_in_context(
        "NEEDS tests/fixtures/needs_probe.f\n\
         NEEDS tests/fixtures/needs_probe.f\n\
         probe-word",
        &mut ctx,
    ).expect("compile");
    // The file's top-level marker is emitted exactly once despite two NEEDS.
    assert_eq!(ir.matches("[probe-loaded]").count(), 1,
        "included once in a single compile:\n{ir}");
    // And the included word is referenced (callable) after the NEEDS.
    assert!(ir.contains("probe-word"), "included def present:\n{ir}");
}

/// Dedup persists across compiles in a session: a second eval's NEEDS of
/// an already-loaded file emits nothing, but the word stays resolvable
/// (it's in the context from the first eval).
#[test]
fn needs_dedups_across_compiles() {
    let mut ctx = CompileContext::new();
    let ir1 = compile_in_context(
        "NEEDS tests/fixtures/needs_probe.f", &mut ctx).expect("compile 1");
    assert_eq!(ir1.matches("[probe-loaded]").count(), 1, "first load emits:\n{ir1}");

    let ir2 = compile_in_context(
        "NEEDS tests/fixtures/needs_probe.f\nprobe-word", &mut ctx).expect("compile 2");
    // Second NEEDS of the same file: no re-emit of the file body.
    assert_eq!(ir2.matches("[probe-loaded]").count(), 0,
        "second NEEDS expands to nothing:\n{ir2}");
    // But probe-word still resolves (defined in the first eval).
    assert!(ir2.contains("probe-word"), "word resolvable in later eval:\n{ir2}");
}

/// Nested NEEDS resolve relative to the *including file's* directory, and
/// the shared dedup set means a diamond pulls the leaf in once.
#[test]
fn needs_resolves_relative_and_dedups_diamond() {
    let mut ctx = CompileContext::new();
    // outer.f lives in tests/fixtures and does `NEEDS needs_probe.f`
    // (a bare sibling name) — only resolvable if NEEDS joins it to
    // outer.f's own directory, not the process CWD.
    let ir = compile_in_context(
        "NEEDS tests/fixtures/needs_outer.f\n\
         NEEDS tests/fixtures/needs_probe.f\n\
         outer-word",
        &mut ctx,
    ).expect("compile");
    // probe loaded exactly once even though outer pulls it in AND the
    // top level NEEDS it again (diamond → one load).
    assert_eq!(ir.matches("[probe-loaded]").count(), 1,
        "leaf included once across the diamond:\n{ir}");
    assert_eq!(ir.matches("[outer-loaded]").count(), 1, "outer included once:\n{ir}");
    assert!(ir.contains("outer-word"), "outer def present:\n{ir}");
}

/// A missing file is a clean compile error (ANS-style message), not a
/// panic or a Factor frame.
#[test]
fn needs_missing_file_is_a_clean_error() {
    let mut ctx = CompileContext::new();
    let err = compile_in_context("NEEDS tests/fixtures/does-not-exist.f", &mut ctx)
        .expect_err("should fail");
    assert!(err.contains("NEEDS"), "error mentions NEEDS: {err}");
    assert!(err.to_lowercase().contains("does-not-exist"), "names the file: {err}");
}

// ── End-to-end: the spliced IR actually runs ───────────────────────

#[cfg(target_os = "windows")]
mod eval {
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

    fn run(s: &Session, ctx: &mut CompileContext, src: &str) {
        let ir = compile_in_context(src, ctx).unwrap_or_else(|e| panic!("compile: {e}"));
        s.eval(&ir).expect("eval");
    }

    /// The spliced file runs once and its word is callable in the same
    /// eval that NEEDS it.
    #[test]
    #[ignore]
    fn needs_runs_once_and_word_is_live() {
        let (s, out, mut ctx) = fresh();
        run(&s, &mut ctx, "NEEDS tests/fixtures/needs_probe.f\n\
                           NEEDS tests/fixtures/needs_probe.f\n\
                           probe-word .");
        let cap = String::from_utf8_lossy(&out.lock().unwrap()).to_string();
        eprintln!("captured: {cap:?}");
        assert_eq!(cap.matches("[probe-loaded]").count(), 1, "ran once: {cap}");
        assert!(cap.contains("42"), "probe-word returned 42: {cap}");
    }
}
