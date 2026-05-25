//! Reproduce user-reported IDE crashes in Test mode so we can
//! see what's actually breaking.

#![cfg(target_os = "windows")]

use std::sync::Arc;
use std::sync::Mutex;

use newfactor::compiler::{compile_in_context, CompileContext};
use newfactor::session::{IoMode, Session, SessionOpts};

struct Repl {
    session: Session,
    ctx: CompileContext,
    out: Arc<Mutex<Vec<u8>>>,
}

impl Repl {
    fn new() -> Self {
        let out = Arc::new(Mutex::new(Vec::new()));
        let opts = SessionOpts::defaults_for_crate(IoMode::Test {
            input: vec![],
            output: out.clone(),
        });
        let session = Session::new(opts).expect("Session::new");
        Self { session, ctx: CompileContext::new(), out }
    }
    fn eval(&mut self, src: &str) -> Result<(), String> {
        let ir = compile_in_context(src, &mut self.ctx).map_err(|e| format!("compile: {e}"))?;
        eprintln!("--- IR for {src:?} ---\n{ir}\n---");
        self.session
            .eval(&ir)
            .map_err(|e| format!("eval: {e}"))?;
        Ok(())
    }
    fn captured(&self) -> String {
        let bytes = self.out.lock().unwrap().clone();
        String::from_utf8_lossy(&bytes).to_string()
    }
}

#[test]
#[ignore]
fn diag_user_test_definition_then_call() {
    let mut r = Repl::new();
    let r1 = r.eval(": test 10 10 + ;");
    eprintln!("eval 1 result: {r1:?}");
    let r2 = r.eval("test");
    eprintln!("eval 2 result: {r2:?}  captured: {:?}", r.captured());
    // Stack now has 20
    let r3 = r.eval(".");
    eprintln!("eval 3 result: {r3:?}  captured: {:?}", r.captured());
    assert!(r.captured().contains("20"), "expected 20 in output");
}

#[test]
#[ignore]
fn diag_bare_dot_on_empty_stack() {
    // Known limitation: stack underflow at the VM level fires a
    // kernel-error that walks the native C frames past our
    // alien-callback boundary.  The Factor listener architecture
    // (#54) catches Factor-level errors (tuple throws, no-method,
    // etc.) but kernel-level underflows still crash.  Tracked as
    // #47 / #48.  Marked #[ignore] so it doesn't block CI.
    let mut r = Repl::new();
    let res = r.eval(".");
    eprintln!("dot-on-empty result: {res:?}  captured: {:?}", r.captured());
    let r2 = r.eval("42 .");
    eprintln!("after err, 42 . result: {r2:?}  captured: {:?}", r.captured());
    assert!(r.captured().contains("42"), "session should be alive");
}

#[test]
#[ignore]
fn diag_bare_emit() {
    let mut r = Repl::new();
    let res = r.eval("65 emit");
    eprintln!("65 emit result: {res:?}  captured: {:?}", r.captured());
    assert!(r.captured().contains('A'), "expected 'A' in output");
}
