// Release builds ship as a Windows GUI-subsystem app: no console
// window pops up when the user launches factorforth-ui.exe from
// Explorer.  Debug builds keep the console attached so eprintln!
// traces show up when running under `cargo run`.
#![cfg_attr(
    all(windows, not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! `factorforth-ui` — the FactorForth IDE.
//!
//! Wires WF64's iGui MDI front-end (Direct2D / DirectWrite) to
//! the in-process `newfactor::session::Session`.  Structure mirrors
//! WF64's own IDE (`wf64::bin::wf64_ui`) but swaps WF64's subprocess
//! `FactorSession` for our in-process `Session`.
//!
//! ## Architecture
//!
//! ```text
//! newfactor-ui.exe  (one Windows process)
//! ├── GUI thread        Direct2D MDI, Win32 message pump (wf64::igui)
//! │     ↕ IGuiEvent MPSC channel
//! ├── IDE worker        receives events, drives Session
//! │     ↕ Command/EvalResult channels
//! └── Session worker    owns Factor VM (newfactor::session::Session)
//!       eval-callback → rt_write_char → IoMode::Gui callback → fconsole
//! ```
//!
//! The supervisor wraps the IDE worker in `catch_unwind` and the
//! SEH crash handler, mirroring WF64's three-level recovery:
//!
//!   - SEH crash    → crash_handler dump → respawn IDE worker
//!   - Rust panic   → catch_unwind → report + respawn
//!   - Session dies → Session::drop + Session::new() → keep going
//!
//! For the in-flight-interrupt case (long-running Forth loop the
//! user wants to stop), the FFI hook is wired (`Session::interrupt`)
//! but currently inert — see #51 for the VM-side SEH-table patch.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_editor_checker();
    igui::crash_handler::install();
    // Warm-charcoal wallpaper for FactorForth.  Slightly distinct
    // from WF64's cool navy so running both side-by-side reads
    // at a glance — but close enough that the family resemblance
    // is preserved.  The amber brand colour in the icon pairs
    // with this warmer base.
    igui::window::set_frame_palette(igui::window::FramePalette {
        bg: 0x221E1A,  // warm charcoal
        fg: 0x4A3C2E,  // toasted-bronze dots, +~40/channel
    });
    // Register the Forth → Break / Ctrl+B hook so the GUI thread
    // can interrupt the running eval without going through the
    // IDE worker's event queue (which is blocked inside
    // session.eval at the moment the user wants to abort).
    igui::channels::set_interrupt_hook(
        Some(newfactor::session::interrupt_current_session));

    let worker = || {
        wait_for_frame();
        retitle_frame();
        auto_open_console();
        run_supervisor();
    };
    let exit_code = igui::run(Some(worker))?;
    std::process::exit(exit_code);
}

/// Shared editor-side snapshot of the live session's `CompileContext`.
///
/// The IDE worker is the sole writer: after every successful
/// `compile_in_context` call (and after a session restart), it
/// republishes its ctx into the `RwLock`.  The F7 editor checker
/// reads from this snapshot so the editor sees the same dictionary
/// the next eval will compile against — words defined in earlier
/// REPL evals stop showing up as "unknown word" in the editor.
///
/// `OnceLock` because the snapshot doesn't exist until the IDE
/// worker starts (after `main` has called `install_editor_checker`).
/// The checker treats a missing snapshot as "no prior context" and
/// falls back to a fresh `build_sema`.
#[cfg(windows)]
static EDITOR_SNAPSHOT: std::sync::OnceLock<
    std::sync::RwLock<newfactor::compiler::CompileContext>
> = std::sync::OnceLock::new();

/// Publish a fresh snapshot of the IDE worker's compile-context for
/// the F7 checker to read.  Called from the worker after every
/// successful compile and at session restart.  Clones the ctx — the
/// checker doesn't see future mutations until the next publish.
#[cfg(windows)]
fn publish_editor_snapshot(ctx: &newfactor::compiler::CompileContext) {
    let lock = EDITOR_SNAPSHOT.get_or_init(|| {
        std::sync::RwLock::new(newfactor::compiler::CompileContext::new())
    });
    *lock.write().unwrap() = ctx.clone();
}

#[cfg(windows)]
fn install_editor_checker() {
    use newfactor::compiler;
    use newfactor::compiler::effect::EffectError;
    use newfactor::compiler::error::Span;
    use newfactor::compiler::parse::ParseError;
    use newfactor::compiler::resolve::ResolveError;
    use igui::{install_checker, Diagnostic};

    fn diag_from_span(span: Span, message: String) -> Diagnostic {
        Diagnostic {
            line: span.start.line as usize,
            column: span.start.col as usize,
            message,
        }
    }

    fn parse_error_span(err: &ParseError) -> Span {
        match err {
            ParseError::ExpectedDefName { at }
            | ParseError::ExpectedDefiningName { at, .. }
            | ParseError::ConstantWithoutValue { at, .. }
            | ParseError::NonLiteralConstantValue { at, .. }
            | ParseError::StraySemicolon { at }
            | ParseError::MalformedStackEffect { at, .. }
            | ParseError::StrayControlWord { at, .. }
            | ParseError::LetSyntax { at, .. } => *at,
            ParseError::NestedColon { inner, .. } => *inner,
            ParseError::UnterminatedDefinition { opened_at } => *opened_at,
            ParseError::UnterminatedControl { opened_at, .. } => *opened_at,
        }
    }

    fn resolve_error_span(err: &ResolveError) -> Span {
        match err {
            ResolveError::UnknownWord { at, .. }
            | ResolveError::RedefinedWord { at, .. }
            | ResolveError::RecurseNeedsEffect { at, .. }
            | ResolveError::ToNotValue { at, .. } => *at,
        }
    }

    fn effect_error_span(err: &EffectError) -> Span {
        match err {
            EffectError::Mismatch { at, .. }
            | EffectError::CaseNeedsDefault { at, .. } => *at,
        }
    }

    install_checker(|source| {
        // Pull a read lock on the current editor snapshot, if any.
        // The IDE worker writes to this after each successful eval;
        // before the worker boots, the slot is empty and we fall
        // back to a context-free check.
        //
        // The read guard is held only for the duration of the sema
        // build, which is fast (microseconds for typical source).
        // Worker writes contend on this lock briefly per eval — also
        // negligible.
        let snapshot = EDITOR_SNAPSHOT.get().map(|l| l.read().unwrap());

        let sema_result = match compiler::lex(source) {
            Ok(tokens) => match compiler::parse(&tokens) {
                Ok(program) => Ok(match &snapshot {
                    Some(snap) => newfactor::compiler::sema::build_with_prior_state(
                        program,
                        &snap.user_words,
                        &snap.user_effects,
                        &snap.templates,
                        &snap.values,
                        &snap.classes,
                    ),
                    None => compiler::build_sema(program),
                }),
                Err(err) => Err(diag_from_span(parse_error_span(&err), err.to_string())),
            },
            Err(err) => Err(diag_from_span(match &err {
                compiler::CompileError::UnterminatedString { opened_at, .. } => *opened_at,
                compiler::CompileError::UnterminatedBlockComment { opened_at } => *opened_at,
                compiler::CompileError::MalformedNumber { at, .. } => *at,
            }, err.to_string())),
        };

        match sema_result {
            Ok(Ok(sema)) => sema.effect_errors.iter()
                .map(|err| diag_from_span(effect_error_span(err), err.to_string()))
                .collect(),
            Ok(Err(err)) => vec![diag_from_span(resolve_error_span(&err), err.to_string())],
            Err(diag) => vec![diag],
        }
    });
}

// ── Supervisor ────────────────────────────────────────────────────────────

/// Outer loop: respawns the IDE worker if its thread crashes via
/// SEH (caught by wf64's vectored-exception handler).
#[cfg(windows)]
fn run_supervisor() {
    use igui::{crash_handler, crash_view};

    loop {
        let join = std::thread::Builder::new()
            .name("nf-ide-worker".into())
            .spawn(|| {
                crash_handler::register_worker_thread();
                run_ide_worker();
                crash_handler::unregister_worker_thread();
            });
        let join = match join {
            Ok(j) => j,
            Err(e) => {
                eprintln!("[supervisor] could not spawn worker: {e}");
                return;
            }
        };
        // VEH-redirected SEH exits cause the std-lib to panic on
        // join; swallow that so we don't propagate.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = join.join();
        }));
        match crash_handler::take_dump() {
            Some(dump) => {
                let text = crash_handler::format_dump(&dump);
                crash_view::push(text);
                igui::fconsole::append("∿ FactorForth thread crashed (SEH) — rebooting.");
                igui::fconsole::append("");
                // loop: respawn
            }
            None => return,
        }
    }
}

// ── IDE worker ────────────────────────────────────────────────────────────

#[cfg(windows)]
fn run_ide_worker() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use igui::fconsole;

    loop {
        let session = match boot_session(true) {
            Some(s) => s,
            None => {
                eprintln!("[factorforth-ui] session boot failed; IDE worker exiting");
                return;
            }
        };

        let result = catch_unwind(AssertUnwindSafe(move || run_drain_loop(session)));
        match result {
            Ok(()) => return,
            Err(payload) => {
                report_panic(payload);
                fconsole::reset_for_restart();
                fconsole::append("∿ FactorForth session crashed — rebooting.");
                fconsole::append("");
            }
        }
    }
}

#[cfg(windows)]
fn run_drain_loop(mut session: newfactor::session::Session) {
    use igui::channels::{self, IGuiEvent};
    use igui::fconsole;

    // Persistent compile context: remembers every name defined
    // across this session's evals (`: square ... ;`, `VARIABLE x`,
    // `42 CONSTANT max`, `8 CBUFFER buf`, etc.).  Without this,
    // each eval starts with an empty user-word set and references
    // to previously-defined names fail at compile time.  Reset
    // when a fresh Session is booted (Factor's dictionary resets
    // too, so they must stay in lockstep).
    let mut compile_ctx = newfactor::compiler::CompileContext::new();
    // Seed the editor snapshot so F7 has something to read even
    // before the first eval lands.  Subsequent compiles refresh it.
    publish_editor_snapshot(&compile_ctx);

    loop {
        let Some(ev) = channels::next_event(200) else { continue };

        match ev {
            IGuiEvent::EvalBuffer { source } => {
                handle_eval(&mut session, &mut compile_ctx, &source);
            }
            IGuiEvent::ForthInterrupt => {
                // Doesn't tear down the session — just nudges the VM.
                // The listener's recover catches ERROR_INTERRUPT and
                // prints the error message; session stays alive,
                // dictionary preserved, next prompt comes up.
                newfactor::session::trace("ide.event",
                    "ForthInterrupt → session.interrupt()");
                session.interrupt();
            }
            IGuiEvent::ForthRestart => {
                fconsole::reset_for_restart();
                drop(session);
                compile_ctx = newfactor::compiler::CompileContext::new();
                // Flush the editor snapshot too — the next F7 should
                // see an empty dictionary, matching the freshly-booted
                // session's state.
                publish_editor_snapshot(&compile_ctx);
                fconsole::append("∿ restart requested — fresh FactorForth session below.");
                fconsole::append("");
                match boot_session(false) {
                    Some(s) => session = s,
                    None => return,
                }
            }
            IGuiEvent::ReplSubmit { child_id } => {
                use igui::repl_pane;
                let Some(source) = repl_pane::pop_input(child_id) else { continue };
                handle_eval_repl(&mut session, &mut compile_ctx, &source, child_id);
            }
            IGuiEvent::FrameClose => {
                fconsole::append("∿ frame closing");
                return;
            }
            _ => {}
        }

        // If the session died during this eval, rebuild it.  The
        // Factor VM persists across worker restarts, so previously-
        // defined words remain in the image's dictionary — but
        // our resolver's context also persists, keeping them in
        // sync.
        if session.is_dead() {
            let cause = session.death_cause()
                .map(|c| format!("{:?}", c))
                .unwrap_or_else(|| "<unknown>".into());
            fconsole::append(&format!("∿ session ended: {cause}"));
            fconsole::append("  spawning a fresh worker (your dictionary persists)…");
            drop(session);
            match boot_session(false) {
                Some(s) => session = s,
                None => return,
            }
        }
    }
}

// ── Output routing ────────────────────────────────────────────────────────
//
// Where Factor's stdout-equivalent goes depends on which pane
// initiated the eval.  An eval from the console pane should
// stream output back into the console; an eval from a REPL
// child pane should land in that REPL.  The IDE worker mutates
// CURRENT_SINK before each session.eval call; the on_write /
// on_flush closures consult it to decide where each line goes.

#[cfg(windows)]
#[derive(Clone, Copy)]
enum OutputSink {
    Console,
    Repl { child_id: i64 },
}

#[cfg(windows)]
static CURRENT_SINK: std::sync::OnceLock<std::sync::Mutex<OutputSink>> =
    std::sync::OnceLock::new();

#[cfg(windows)]
fn set_sink(sink: OutputSink) {
    let slot = CURRENT_SINK.get_or_init(|| std::sync::Mutex::new(OutputSink::Console));
    *slot.lock().unwrap() = sink;
}

#[cfg(windows)]
fn deliver_line(line: String) {
    use igui::fconsole;
    use igui::repl_pane::{self, AppendKind};
    let sink = CURRENT_SINK
        .get_or_init(|| std::sync::Mutex::new(OutputSink::Console));
    let target = *sink.lock().unwrap();
    match target {
        OutputSink::Console => fconsole::append(&line),
        OutputSink::Repl { child_id } => {
            repl_pane::append(child_id, line, AppendKind::Output);
        }
    }
}

// ── Boot / eval helpers ───────────────────────────────────────────────────

#[cfg(windows)]
fn boot_session(intro: bool) -> Option<newfactor::session::Session> {
    use newfactor::session::{IoMode, Session, SessionOpts};
    use igui::fconsole;

    if intro {
        fconsole::append("∿ FactorForth IDE");
        fconsole::append("");
        fconsole::append("ANS Forth front-end on Factor's VM (in-process).");
        fconsole::append("");
        fconsole::append(
            "Type ANS Forth in the prompt, press Enter.  \
             Control structures (IF/ELSE/THEN, BEGIN/UNTIL…) work.",
        );
        fconsole::append(
            "LET (...) -> (...) = expr END — infix algebra for math-heavy code.",
        );
        fconsole::append(
            "Editor: Ctrl+Shift+E   Console: Ctrl+Shift+R   Restart: Ctrl+Shift+F5",
        );
        fconsole::append("");
    }

    // Line-buffered IoMode::Gui callback: buffer bytes until a
    // newline, then append the accumulated string to fconsole.
    // After each eval, the flusher pushes any partial-line buffer
    // so output without a trailing newline (e.g. `42 .` -> "42 ")
    // is still visible.
    let line_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let line_buf_for_writer = line_buf.clone();
    let on_write: Box<dyn FnMut(u8) + Send> = Box::new(move |b: u8| {
        let mut buf = line_buf_for_writer.lock().unwrap();
        if b == b'\n' {
            let line = std::mem::take(&mut *buf);
            drop(buf);
            deliver_line(line);
        } else {
            // Most output is ASCII; non-ASCII bytes are appended as
            // their latin-1 char (will be wrong for multi-byte UTF-8
            // but tolerable for the first cut).
            buf.push(b as char);
        }
    });
    let line_buf_for_flush = line_buf.clone();
    let on_flush: Box<dyn FnMut() + Send> = Box::new(move || {
        let mut buf = line_buf_for_flush.lock().unwrap();
        if !buf.is_empty() {
            let line = std::mem::take(&mut *buf);
            drop(buf);
            deliver_line(line);
        }
    });

    let mut opts = SessionOpts::defaults_for_crate(IoMode::Gui {
        on_write, on_flush,
    });
    // No automatic watchdog timeout in the IDE - the user has an
    // explicit "Forth → Break" menu item (Ctrl+B) for runaway
    // loops, and intermediate slow computations (large compiles,
    // long renders) shouldn't be guillotined.  An effectively-
    // infinite ceiling keeps Session::eval's two-stage timeout
    // structure intact (in case we ever want it for tests) while
    // making the IDE feel unsupervised.
    opts.eval_timeout = std::time::Duration::from_secs(60 * 60 * 24);

    match Session::new(opts) {
        Ok(s) => {
            fconsole::append("∿ FactorForth session ready.");
            fconsole::append("");
            Some(s)
        }
        Err(e) => {
            fconsole::append(&format!("∿ session boot failed: {e}"));
            None
        }
    }
}

#[cfg(windows)]
fn handle_eval(
    session: &mut newfactor::session::Session,
    ctx: &mut newfactor::compiler::CompileContext,
    source: &str,
) {
    use igui::fconsole;
    newfactor::session::trace("ide.handle_eval",
        &format!("entry, source.len={}", source.len()));
    // Output from this eval lands in the console pane.
    set_sink(OutputSink::Console);

    let multiline = source.lines().count() > 1;
    if multiline {
        fconsole::append("─── eval ───");
        for line in source.lines().take(8) {
            fconsole::append(line);
        }
        let extra = source.lines().count().saturating_sub(8);
        if extra > 0 {
            fconsole::append(&format!("    … {extra} more line(s) elided"));
        }
        fconsole::append("─── result ───");
    }

    // Compile through FactorForth's pipeline, threading the session-
    // wide context so previous defs (`: square ...`, `VARIABLE x`)
    // resolve in subsequent evals.  Compile errors surface as a ⚠
    // line; eval errors as a WorkerDied(cause) which we render via
    // Display.
    let ir = match newfactor::compiler::compile_in_context(source, ctx) {
        Ok(ir) => ir,
        Err(e) => {
            newfactor::session::trace("ide.handle_eval",
                &format!("compile error: {e}"));
            fconsole::append(&format!("⚠ compile: {e}"));
            return;
        }
    };
    // Compile succeeded — refresh the F7 editor's snapshot so the
    // next syntax check sees any names this eval introduced.
    publish_editor_snapshot(ctx);
    newfactor::session::trace("ide.handle_eval",
        &format!("compiled OK ({} bytes IR); calling session.eval",
            ir.len()));
    if let Err(e) = session.eval(&ir) {
        newfactor::session::trace("ide.handle_eval",
            &format!("session.eval Err: {e}"));
        fconsole::append(&format!("⚠ {e}"));
    } else {
        newfactor::session::trace("ide.handle_eval", "session.eval Ok");
    }
    // Successful output already flowed through the host-stream
    // callback (line-buffered into fconsole) while eval was running.
}

#[cfg(windows)]
fn handle_eval_repl(
    session: &mut newfactor::session::Session,
    ctx: &mut newfactor::compiler::CompileContext,
    source: &str,
    child_id: i64,
) {
    use igui::repl_pane::{self, AppendKind};
    newfactor::session::trace("ide.handle_eval_repl",
        &format!("entry, source.len={}, child_id={}",
            source.len(), child_id));
    // Output from this eval streams back into THIS REPL pane.
    set_sink(OutputSink::Repl { child_id });

    let ir = match newfactor::compiler::compile_in_context(source, ctx) {
        Ok(ir) => ir,
        Err(e) => {
            newfactor::session::trace("ide.handle_eval_repl",
                &format!("compile error: {e}"));
            repl_pane::append(child_id, format!("compile: {e}"), AppendKind::Error);
            return;
        }
    };
    // Compile succeeded — refresh the F7 editor's snapshot so the
    // next syntax check sees any names this eval introduced.
    publish_editor_snapshot(ctx);
    newfactor::session::trace("ide.handle_eval_repl",
        &format!("compiled OK ({} bytes IR); calling session.eval",
            ir.len()));
    // Output flows through the IoMode::Gui callback into fconsole
    // (the shared host console) rather than per-REPL.  Per-REPL
    // routing is a refinement once we have multiple REPLs in flight.
    match session.eval(&ir) {
        Ok(_) => {
            newfactor::session::trace("ide.handle_eval_repl",
                "session.eval Ok");
            repl_pane::append(child_id, "ok".into(), AppendKind::Output);
        }
        Err(e) => {
            newfactor::session::trace("ide.handle_eval_repl",
                &format!("session.eval Err: {e}"));
            repl_pane::append(child_id, e.to_string(), AppendKind::Error);
        }
    }
}

// ── Startup helpers ───────────────────────────────────────────────────────

#[cfg(windows)]
fn wait_for_frame() {
    use std::time::Duration;
    for _ in 0..200 {
        if igui::cp_exports::FRAME_HWND.get().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!("[factorforth-ui] FRAME_HWND not published after 4 s; continuing anyway");
}

/// Override iGui's default frame title ("WF64 - Forth IDE") with
/// ours.  iGui hardcodes its title in WF64's window.rs; we resilve
/// after the frame is up via SetWindowTextW.
#[cfg(windows)]
fn retitle_frame() {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetWindowTextW;

    let Some(&hwnd_isize) = igui::cp_exports::FRAME_HWND.get() else {
        return;
    };
    let hwnd = HWND(hwnd_isize as *mut _);
    // ∴ glyph + em-dash to match the iGui visual house style.
    let title: Vec<u16> = "\u{2234} FactorForth \u{2014} Forth IDE"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(title.as_ptr())) };
}

#[cfg(windows)]
fn auto_open_console() {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_COMMAND};
    let Some(&hwnd_isize) = igui::cp_exports::FRAME_HWND.get() else {
        return;
    };
    let hwnd = HWND(hwnd_isize as *mut _);
    let cmd_id = igui::fconsole::MENU_CMD_ID;
    let _ = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(cmd_id as usize),
            LPARAM(0),
        )
    };
}

// ── Panic reporting ───────────────────────────────────────────────────────

#[cfg(windows)]
fn report_panic(payload: Box<dyn std::any::Any + Send>) {
    use igui::crash_view;

    let msg: String = if let Some(s) = payload.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<panic payload not a string>".to_string()
    };

    let thread = std::thread::current()
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{:?}", std::thread::current().id()));

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}.{:03}", d.as_secs(), d.subsec_millis()))
        .unwrap_or_else(|_| "<no time>".into());

    let mut dump = String::new();
    dump.push_str(&format!("when:    {ts}\n"));
    dump.push_str(&format!("thread:  {thread}\n"));
    dump.push_str("kind:    Rust panic\n");
    dump.push_str(&format!("message: {msg}\n"));
    dump.push('\n');
    dump.push_str("The FactorForth session has been dropped.\n");
    dump.push_str("A fresh session will be booted below.\n");
    crash_view::push(dump);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("factorforth-ui is Windows-only (iGui depends on Direct2D / DirectWrite).");
    std::process::exit(1);
}
