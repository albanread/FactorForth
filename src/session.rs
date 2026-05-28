//! Session — the embedded VM on a dedicated worker thread.
//!
//! Why this exists:
//!
//! Factor's VM is single-threaded and TLS-resident: every call to
//! `nf_eval_string` must happen on the thread that called
//! `nf_init_factor`.  KEY blocks waiting for input; if Factor and
//! the GUI/test runner share a thread, KEY deadlocks the host.
//! Solution: own the VM on a worker thread, communicate via
//! channels, let the host's main thread service GUI events / test
//! orchestration without ever directly touching the VM.
//!
//! The same machinery lets us redirect I/O.  Three modes:
//!   - `IoMode::Test`     pre-fed input, captured output (for tests)
//!   - `IoMode::Terminal` stdin / stdout (for headless CLI use)
//!   - `IoMode::Gui`      callbacks the host wires to a UI pane
//!
//! Factor's KEY / EMIT / TYPE eventually call three extern C
//! functions — `nf_rt_read_char`, `nf_rt_write_char`, `nf_rt_read_line` —
//! which dispatch on the current session's mode.  Worker thread
//! blocks on the input queue when KEY runs and no input is queued;
//! main thread fills the queue via `feed_input` (GUI keystrokes,
//! test fixtures, terminal byte read, etc.).
//!
//! ## Lifecycle
//!
//! ```text
//! Session::new(opts)
//!   ├── installs `CURRENT` (global, so extern fns can find it)
//!   ├── spawns worker thread
//!   ├── worker: LoadLibrary factor.dll, nf_init_factor,
//!   │           nf_run_startup, install host streams in Factor
//!   └── returns Session
//!
//! session.eval("..")
//!   ├── send Command::Eval to worker
//!   ├── worker calls nf_eval_string
//!   ├── while running, Factor may call nf_rt_read_char/nf_rt_write_char
//!   │   (those find CURRENT, route to session.io_state)
//!   └── receive EvalResult on reply channel
//!
//! session.feed_input(b"hello\n")
//!   └── main thread: push to io_state.input_q, notify_all
//!
//! session.shutdown() / drop
//!   ├── send Command::Shutdown
//!   ├── worker exits cleanly
//!   └── clear CURRENT
//! ```

#![cfg(target_os = "windows")]

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use libloading::{Library, Symbol};

// ─── Trace logging ──────────────────────────────────────────────────────────
//
// Writes to `factorforth.log` next to the exe.  Every key event
// in the session lifecycle gets a timestamped line.  Lets us
// diagnose hangs and crashes after the fact without needing a
// debugger attached.  Cheap (one mutex-locked file append per
// event); the file gets recreated on each Session::new.

use std::fs::OpenOptions;
use std::io::Write as _;

static TRACE_FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();
static TRACE_START: OnceLock<Instant> = OnceLock::new();

fn trace_init() {
    let _ = TRACE_START.get_or_init(Instant::now);
    let slot = TRACE_FILE.get_or_init(|| Mutex::new(None));
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("factorforth.log")))
        .unwrap_or_else(|| PathBuf::from("factorforth.log"));
    let mut guard = slot.lock().unwrap();
    *guard = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .ok();
    if let Some(ref mut f) = *guard {
        let _ = writeln!(f, "=== factorforth log opened at {path:?} ===");
        let _ = f.flush();
    }
}

pub fn trace(category: &str, msg: &str) {
    let t = TRACE_START.get().map(|s| s.elapsed().as_millis()).unwrap_or(0);
    let tid = format!("{:?}", std::thread::current().id());
    let line = format!("[{t:>6}ms {tid:>10} {category:<14}] {msg}\n");
    if let Some(slot) = TRACE_FILE.get() {
        if let Ok(mut guard) = slot.lock() {
            if let Some(ref mut f) = *guard {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
        }
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Decides where bytes flow.  Set at session construction; stable
/// for the session's lifetime.
pub enum IoMode {
    /// Pre-fed input, captured output.  The shape tests want.
    ///
    /// `input` is consumed as Factor reads it; once empty, KEY
    /// returns -1 (EOF).  `output` is appended to as Factor writes;
    /// the host pulls it out via `collected_output`.
    Test {
        input: Vec<u8>,
        output: Arc<Mutex<Vec<u8>>>,
    },
    /// Reads from stdin, writes to stdout.  Line-buffered the way
    /// the OS wires it.  Useful for `newfactor demo.f` headless
    /// use.
    Terminal,
    /// Custom host I/O — input via `feed_input`, output via a
    /// caller-provided closure.  This is what the GUI binary
    /// uses (or will when Phase 3.4 lands).
    ///
    /// `on_write` receives each output byte individually.
    /// `on_flush` is called after each eval completes — gives
    /// the host a chance to push any partial-line buffer to the
    /// console.  Forth output frequently doesn't end with a
    /// newline (e.g. `42 .` emits `42 ` with no newline), so
    /// without a flush hook the IDE would never show that text.
    Gui {
        on_write: Box<dyn FnMut(u8) + Send>,
        on_flush: Box<dyn FnMut() + Send>,
    },
}

/// Construction-time options.
pub struct SessionOpts {
    /// Path to the patched factor.dll (the VM).
    pub dll_path: PathBuf,
    /// Path to the image to load (contains forth.runtime,
    /// forth.wf64-gfx, plus whatever else was baked in).
    pub image_path: PathBuf,
    pub mode: IoMode,
    /// Max wall-clock per `eval` call.  After this elapses the
    /// whole process aborts (we can't safely unwind a stuck
    /// Factor call).  Matches today's test watchdog.
    pub eval_timeout: Duration,
}

impl SessionOpts {
    /// Resolve to whichever of `factor.dll` / `factorforth.image`
    /// exists first:
    ///   1. next to the running .exe   — release / installed layout
    ///   2. under the crate manifest    — dev / cargo-test layout
    ///
    /// The release folder ships factor.dll + factorforth.image
    /// next to factorforth-ui.exe (see release/factorforth/).  In
    /// dev builds those files live in vm-build/ + images/ under
    /// the crate root, where cargo test naturally finds them.
    pub fn defaults_for_crate(mode: IoMode) -> Self {
        let (dll_path, image_path) = resolve_default_paths();
        Self {
            dll_path,
            image_path,
            mode,
            eval_timeout: Duration::from_secs(20),
        }
    }
}

/// Two-tier lookup: exe-adjacent first, then the crate manifest.
/// Returns whichever pair exists; if neither, returns the exe-
/// adjacent paths (Session::new will then surface DllNotFound).
fn resolve_default_paths() -> (PathBuf, PathBuf) {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let manifest_dll = PathBuf::from(manifest).join("vm-build").join("factor.dll");
    let manifest_img = PathBuf::from(manifest).join("images").join("factorforth.image");

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let exe_dll = dir.join("factor.dll");
            let exe_img = dir.join("factorforth.image");
            if exe_dll.exists() && exe_img.exists() {
                return (exe_dll, exe_img);
            }
        }
    }
    (manifest_dll, manifest_img)
}

/// Inflate `src` (a zstd archive) to `dst` atomically.  Writes through
/// a `.tmp` sidecar then renames into place, so a crash partway through
/// inflation never leaves a half-written image that the loader would
/// then try to consume.  Returns the final file size on success.
fn inflate_image(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u64> {
    let tmp = {
        let mut t = dst.to_path_buf();
        let stem = t.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        t.set_file_name(format!("{stem}.tmp"));
        t
    };

    let in_file  = std::fs::File::open(src)?;
    let out_file = std::fs::File::create(&tmp)?;
    let mut dec = zstd::Decoder::new(in_file)?;
    let mut bw  = std::io::BufWriter::with_capacity(1 << 20, out_file);
    let written = std::io::copy(&mut dec, &mut bw)?;
    // Flush BufWriter explicitly; std::io::copy doesn't.
    use std::io::Write;
    bw.flush()?;
    drop(bw);

    // Atomic-ish rename.  On Windows, std::fs::rename across the same
    // directory is atomic for the destination's appearance to other
    // processes.
    std::fs::rename(&tmp, dst)?;
    Ok(written)
}

/// Result of one `eval` call.
#[derive(Debug)]
pub struct EvalResult {
    /// Whatever `nf_eval_string` returned — typically diagnostics
    /// for error paths, empty string for clean evaluations.  This
    /// is NOT the user-visible output (that goes through the I/O
    /// callbacks); it's Factor's interpreter feedback.
    pub interpreter_output: String,
}

/// A live Factor session.
pub struct Session {
    cmd_tx: Sender<Command>,
    io_state: Arc<IoState>,
    worker: Option<JoinHandle<()>>,
    eval_timeout: Duration,
    /// Set to Some(reason) when the worker has died, panicked, or
    /// timed out.  Once set, subsequent `eval` calls fail fast
    /// without trying to send on the channel.  The host can call
    /// `Session::new()` to spawn a fresh worker; the underlying
    /// Factor VM persists across worker restarts so previously-
    /// defined words remain in the image.
    dead: Arc<Mutex<Option<DeathCause>>>,
    /// Cross-thread handle to interrupt the running VM.  Set once
    /// at worker startup; remains `Some` for the session's life.
    /// Called by `eval` on timeout to stop the worker politely
    /// rather than letting it run forever.
    interrupter: Arc<Mutex<Option<Interrupter>>>,
}

/// Why a session worker is no longer healthy.  Conveyed to the
/// host so the UI can show "Session crashed: division by zero"
/// rather than just disappearing.  The `last_source` field captures
/// what was being evaluated at the time of death, when known.
#[derive(Debug, Clone)]
pub enum DeathCause {
    /// The per-eval timeout fired.  The worker thread is likely
    /// still stuck inside a Factor call (we can't safely interrupt
    /// it).  A new worker can be spawned via `Session::new()`, but
    /// the old worker thread leaks until process exit.
    Timeout { last_source: String, after: Duration },
    /// The worker thread panicked (Rust panic, caught by
    /// `catch_unwind`).  Factor's VM should still be sane — Rust
    /// panics don't reach into Factor's C state.
    WorkerPanicked { last_source: String, message: String },
    /// The worker thread terminated cleanly but unexpectedly
    /// (channel disconnect, etc.).
    WorkerGone { last_source: String },
}

impl Session {
    pub fn new(opts: SessionOpts) -> Result<Self, SessionError> {
        let SessionOpts { dll_path, image_path, mode, eval_timeout } = opts;

        // Validate paths up front so we fail clearly rather than
        // in worker-thread land.
        if !dll_path.exists() {
            return Err(SessionError::DllNotFound(dll_path));
        }
        // Image-on-demand inflation.  We ship `factorforth.image.zst`
        // (~30 MB) instead of `factorforth.image` (~134 MB) to keep
        // download size small.  On first run the raw image is absent
        // but the .zst lives next to it; decompress once and from then
        // on the loader sees the raw image like nothing changed.
        if !image_path.exists() {
            let mut zst_path = image_path.clone();
            let stem = zst_path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            zst_path.set_file_name(format!("{stem}.zst"));
            if zst_path.exists() {
                inflate_image(&zst_path, &image_path)
                    .map_err(SessionError::ImageInflateFailed)?;
            } else {
                return Err(SessionError::ImageNotFound(image_path));
            }
        }

        // Build the shared I/O state from the mode.
        let io_state = Arc::new(IoState::from_mode(mode));

        // Install as the process-wide current session.  Extern
        // functions look here when Factor calls them.  This will
        // succeed only if no other Session is currently alive —
        // if the previous Session's worker is stuck (Timeout
        // death cause), the host must drop the dead Session first.
        install_current(io_state.clone())?;

        // Spawn the worker.
        let (cmd_tx, cmd_rx) = channel();
        let state_for_worker = io_state.clone();
        let dead = Arc::new(Mutex::new(None));
        let dead_for_worker = dead.clone();
        let interrupter: Arc<Mutex<Option<Interrupter>>> = Arc::new(Mutex::new(None));
        let interrupter_for_worker = interrupter.clone();
        // Publish for cross-thread access (used by the IDE's
        // Ctrl+B / Forth → Break hook on the GUI thread).
        {
            let slot = CURRENT_INTERRUPTER.get_or_init(|| Mutex::new(None));
            *slot.lock().unwrap() = Some(interrupter.clone());
        }
        let worker = std::thread::Builder::new()
            .name("nf-session-worker".into())
            .spawn(move || {
                // Catch Rust panics so they become DeathCause
                // signals rather than process kills.  Factor-side
                // exceptions are already caught by Factor's own
                // recover machinery (see runtime.factor §13);
                // this only fires when our Rust code (e.g. an
                // extern callback) panics.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_main(
                        dll_path, image_path,
                        state_for_worker, cmd_rx,
                        interrupter_for_worker,
                    );
                }));
                if let Err(panic_payload) = result {
                    let message = panic_message(&panic_payload);
                    let mut d = dead_for_worker.lock().unwrap();
                    if d.is_none() {
                        *d = Some(DeathCause::WorkerPanicked {
                            last_source: String::new(),
                            message,
                        });
                    }
                }
            })
            .map_err(|e| SessionError::WorkerSpawn(e.to_string()))?;

        Ok(Session {
            cmd_tx,
            io_state,
            worker: Some(worker),
            eval_timeout,
            dead,
            interrupter,
        })
    }

    /// Send source to the worker for evaluation.  Blocks until the
    /// worker returns a result OR the per-eval timeout fires.  On
    /// timeout the Session is marked dead — subsequent calls
    /// fail-fast and the host should `drop` this Session and call
    /// `Session::new()` to recover.
    ///
    /// If the worker has already died (panic or prior timeout),
    /// returns `SessionError::WorkerDied` immediately with the
    /// cause carried through.
    pub fn eval(&self, source: &str) -> Result<EvalResult, SessionError> {
        trace("Session.eval",
            &format!("entry: {} bytes", source.len()));
        // Fail fast if a prior eval marked us dead.
        if let Some(cause) = self.dead.lock().unwrap().clone() {
            trace("Session.eval", &format!("already dead: {cause:?}"));
            return Err(SessionError::WorkerDied(cause));
        }
        let (reply_tx, reply_rx) = channel();
        trace("Session.eval", "sending Command::Eval to cmd_tx");
        if self.cmd_tx.send(Command::Eval {
            source: source.to_string(),
            reply: reply_tx,
        }).is_err() {
            // Worker thread terminated.  Record cause and report.
            let cause = DeathCause::WorkerGone {
                last_source: source.to_string(),
            };
            *self.dead.lock().unwrap() = Some(cause.clone());
            return Err(SessionError::WorkerDied(cause));
        }

        // First-stage wait: up to eval_timeout for the worker to
        // reply on its own.  Most evals finish well within this.
        trace("Session.eval", "waiting on reply_rx");
        match reply_rx.recv_timeout(self.eval_timeout) {
            Ok(result) => { trace("Session.eval", "reply received OK"); Ok(result) },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The worker is taking longer than allowed.  Ask
                // Factor's safepoint machinery to interrupt at the
                // next opportunity (raises ERROR_INTERRUPT, which
                // the eval-callback's `recover` catches).  Then
                // wait a short grace period for the worker to
                // return with the interrupt error.
                if let Some(intr) = self.interrupter.lock().unwrap().as_ref() {
                    intr.trigger();
                }
                // Grace period: the worker should respond promptly
                // once its next safepoint fires.  If it's stuck in
                // code that never reaches a safepoint (foreign C
                // calls in flight, etc.) we still need to bail.
                match reply_rx.recv_timeout(Duration::from_secs(3)) {
                    Ok(result) => Ok(result),
                    Err(_) => {
                        // Worker didn't honour the interrupt.  Now
                        // it really is dead-to-us.  Mark and
                        // surface — the host can spawn a new
                        // Session, but the old worker is detached.
                        let cause = DeathCause::Timeout {
                            last_source: source.to_string(),
                            after: self.eval_timeout,
                        };
                        *self.dead.lock().unwrap() = Some(cause.clone());
                        Err(SessionError::WorkerDied(cause))
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Worker died between send and recv (panic mid-eval).
                let panic_cause = self.dead.lock().unwrap().clone()
                    .unwrap_or_else(|| DeathCause::WorkerGone {
                        last_source: source.to_string(),
                    });
                let recorded = match &panic_cause {
                    DeathCause::WorkerPanicked { message, .. } =>
                        DeathCause::WorkerPanicked {
                            last_source: source.to_string(),
                            message: message.clone(),
                        },
                    other => other.clone(),
                };
                *self.dead.lock().unwrap() = Some(recorded.clone());
                Err(SessionError::WorkerDied(recorded))
            }
        }
    }

    /// Force the worker to interrupt at its next safepoint.  Doesn't
    /// wait for the result — the host can poll `is_dead()` or call
    /// `eval` afterwards (the in-flight eval will return with an
    /// interrupt-shaped diagnostic).  Useful for a UI "Stop" button.
    pub fn interrupt(&self) {
        if let Some(intr) = self.interrupter.lock().unwrap().as_ref() {
            intr.trigger();
        }
    }

    /// `true` if a prior `eval` left the session in an unrecoverable
    /// state (timeout, panic, channel disconnect).  Host code can
    /// poll this between user actions to decide whether to offer a
    /// "restart session" affordance.
    pub fn is_dead(&self) -> bool {
        self.dead.lock().unwrap().is_some()
    }

    /// Return the recorded cause of death, if any.  Useful for
    /// surfacing a meaningful error in a UI when `eval` returns
    /// `WorkerDied`.
    pub fn death_cause(&self) -> Option<DeathCause> {
        self.dead.lock().unwrap().clone()
    }

    /// Push bytes into the input queue.  KEY blocking on an empty
    /// queue wakes up when this is called.  Used by GUI keystrokes
    /// or test-mode pre-feeding.
    pub fn feed_input(&self, bytes: &[u8]) {
        let mut q = self.io_state.input_q.lock().unwrap();
        q.extend(bytes.iter().copied());
        self.io_state.input_cv.notify_all();
    }

    /// Pull whatever the session has written so far (Test mode
    /// only).  Returns an empty Vec for other modes.
    pub fn collected_output(&self) -> Vec<u8> {
        match &self.io_state.captured_output {
            Some(buf) => buf.lock().unwrap().clone(),
            None => Vec::new(),
        }
    }

    /// Signal end-of-input.  Subsequent KEY calls return -1 (EOF)
    /// instead of blocking.  Useful for tests that want to verify
    /// EOF handling.
    pub fn close_input(&self) {
        self.io_state.input_closed.store(true, Ordering::Release);
        self.io_state.input_cv.notify_all();
    }
}

/// Best-effort extraction of a panic message from `catch_unwind`'s
/// `Box<dyn Any>` payload.  Most Rust panics carry a `&'static str`
/// or `String`; fall back to a generic marker otherwise.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<unknown panic payload>".to_string()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Tell the worker to stop, join it, and clear the global.
        let _ = self.cmd_tx.send(Command::Shutdown);
        if let Some(h) = self.worker.take() {
            // If the worker is stuck inside a Factor call (the
            // `Timeout` death cause), `join` would block forever.
            // Detach in that case rather than wait — the OS reaps
            // the stuck thread on process exit.  Otherwise wait
            // briefly for clean shutdown.
            let stuck = matches!(
                self.dead.lock().unwrap().as_ref(),
                Some(DeathCause::Timeout { .. }),
            );
            if !stuck {
                let _ = h.join();
            }
            // For `stuck = true`, JoinHandle is dropped → thread
            // is detached.  Acceptable leak; documented limitation.
        }
        clear_current();
        // Clear the global interrupter handle so a stale pointer
        // can't fire into a freed VM after Drop returns.
        if let Some(slot) = CURRENT_INTERRUPTER.get() {
            if let Ok(mut g) = slot.lock() {
                *g = None;
            }
        }
    }
}

// ─── Errors ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SessionError {
    DllNotFound(PathBuf),
    ImageNotFound(PathBuf),
    /// `factorforth.image` was missing but a `.zst` sibling existed,
    /// and inflation failed (disk full, corrupt archive, permissions).
    /// Carries the underlying I/O error so the user can act.
    ImageInflateFailed(std::io::Error),
    WorkerSpawn(String),
    /// (Deprecated, kept for backwards compatibility with callers
    /// that didn't switch to WorkerDied yet.)  Worker thread
    /// terminated without further detail.
    WorkerGone,
    /// The worker is no longer healthy.  `DeathCause` describes
    /// how it died (timeout, panic, disconnect) and carries the
    /// last source if known.  Host code can `drop` this Session
    /// and call `Session::new()` to spawn a fresh worker; the
    /// Factor VM's dictionary state persists across worker
    /// restarts.
    WorkerDied(DeathCause),
    AlreadyRunning,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::DllNotFound(p)   => write!(f, "factor.dll not found at {}", p.display()),
            SessionError::ImageNotFound(p) => write!(f, "image not found at {}", p.display()),
            SessionError::ImageInflateFailed(e) =>
                write!(f, "failed to inflate factorforth.image.zst: {e}"),
            SessionError::WorkerSpawn(e)   => write!(f, "spawn worker thread: {e}"),
            SessionError::WorkerGone       => write!(f, "session worker thread terminated"),
            SessionError::WorkerDied(cause) => match cause {
                DeathCause::Timeout { last_source, after } =>
                    write!(f, "session timed out after {:?} while evaluating: {last_source}", after),
                DeathCause::WorkerPanicked { last_source, message } =>
                    write!(f, "session worker panicked: {message}\n  (last source: {last_source})"),
                DeathCause::WorkerGone { last_source } =>
                    write!(f, "session worker disconnected while evaluating: {last_source}"),
            },
            SessionError::AlreadyRunning   => write!(f, "another Session is already active in this process"),
        }
    }
}

impl std::error::Error for SessionError {}

// ─── Worker thread ──────────────────────────────────────────────────────────

enum Command {
    Eval { source: String, reply: Sender<EvalResult> },
    Shutdown,
}

/// Hand-off from the worker (which owns the loaded `factor.dll` and
/// the VM pointer) to the host so the host can request an interrupt
/// from its own thread on timeout.  Sent once at worker startup
/// via a one-shot channel.
///
/// Both fields are raw-pointer-typed and not `Send` by default; we
/// hand-implement Send+Sync.  Safety: the VM pointer is only deref'd
/// inside Factor's own (already-locked) safepoint machinery, which
/// is designed to be poked from another thread.  The function
/// pointer is a stable export from the DLL that lives for the
/// session's lifetime.
pub(crate) struct Interrupter {
    vm: *mut c_void,
    enqueue: unsafe extern "C-unwind" fn(*mut c_void),
}

unsafe impl Send for Interrupter {}
unsafe impl Sync for Interrupter {}

impl Interrupter {
    /// Call from any thread.  Asks Factor's safepoint machinery to
    /// raise `ERROR_INTERRUPT` at the worker's next safepoint check.
    pub(crate) fn trigger(&self) {
        unsafe { (self.enqueue)(self.vm) }
    }
}

/// Factor's embedded API exports we need to drive the VM.
struct NfApi<'lib> {
    new_vm:              Symbol<'lib, unsafe extern "C-unwind" fn() -> *mut c_void>,
    default_parameters:  Symbol<'lib, unsafe extern "C-unwind" fn() -> *mut c_void>,
    free_parameters:     Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void)>,
    params_set_image:    Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void, *const u16)>,
    params_set_signals:  Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void, c_int)>,
    init_factor:         Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void, *mut c_void)>,
    run_startup:         Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void)>,
    eval_string:         Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void, *mut c_char) -> *mut c_char>,
    eval_free:           Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void, *mut c_char)>,
    /// Signals the running VM thread to break out of its current
    /// computation at the next safepoint via Factor's FEP machinery.
    /// Safe to call from any thread.  Worker's eval then returns
    /// with an `ERROR_INTERRUPT` formatted into its output.
    enqueue_interrupt:   Symbol<'lib, unsafe extern "C-unwind" fn(*mut c_void)>,
}

/// Programming-Tools word set, boot-defined into `forth.runtime`.
///
/// Three user-facing words, wired into the resolver as `.s`,
/// `words`, and `dump`:
///
///   - `nf-.s    ( -- )`    non-destructive data-stack print,
///                          gforth-style `<depth> a b c`.
///   - `nf-words ( -- )`    list the user's own definitions (the
///                          `scratchpad` vocab where `:` defs land).
///   - `nf-dump  ( x -- x )` inspect the value on top of the stack:
///                          a type tag + value, plus a 16-byte
///                          hex/ASCII dump of the backing bytes for
///                          strings and nf-addrs.  Leaves x in place.
///
/// Helpers are prefixed `(nf-...)` by convention for "implementation
/// detail, not a user word."
const TOOLS_SETUP_SRC: &str = r#"
USING: kernel math math.parser sequences strings byte-arrays
       io classes words quotations vocabs grouping namespaces
       accessors combinators prettyprint.config forth.runtime ;
IN: forth.runtime

! Map a byte to a printable ASCII char, or '.' if non-printable.
: (nf-ascii) ( ch -- ch' )
    dup 32 < over 126 > or [ drop CHAR: . ] when ;

! One 16-byte row: zero-padded hex offset, the bytes in hex, then
! an ASCII gutter.
: (nf-row) ( offset bytes -- )
    [ 16 >base 4 CHAR: 0 pad-head write "  " write ] dip
    [ [ 16 >base 2 CHAR: 0 pad-head write " " write ] each ] keep
    "  " write [ (nf-ascii) write1 ] each nl ;

! Classic hex+ASCII dump of a byte-array, 16 bytes per line.
: nf-hexdump ( byte-array -- )
    16 <groups> [ 16 * swap (nf-row) ] each-index ;

! Print one value safely without prettyprint (the slim image
! doesn't carry full prettyprint methods).  Integers honour the
! current BASE; floats go through number>string; strings print
! literally; anything else gets a class-name placeholder.
! Print one value safely without prettyprint (the slim image
! doesn't carry full prettyprint methods).  Integers honour the
! current BASE; floats go through number>string; strings print
! literally; anything else gets a short placeholder.  Declared
! `inline` so per-call type narrowing lets `>base` see a proven
! integer in its branch.
: (nf-pp1) ( x -- )
    {
        { [ dup integer? ] [ number-base get >base write ] }
        { [ dup float?   ] [ number>string write ] }
        { [ dup string?  ] [ write ] }
        { [ dup word?    ] [ name>> write ] }
        [ drop "<obj>" write ]
    } cond ; inline

! The short type tag DUMP prints first.
: (nf-type-name) ( x -- str )
    {
        { [ dup integer?   ] [ drop "INT"    ] }
        { [ dup float?     ] [ drop "FLOAT"  ] }
        { [ dup string?    ] [ drop "STRING" ] }
        { [ dup quotation? ] [ drop "XT"     ] }
        { [ dup word?      ] [ drop "XT"     ] }
        { [ dup nf-addr?   ] [ drop "ADDR"   ] }
        [ drop "OTHER" ]
    } cond ;

! Print the value detail (consumes its argument).
: (nf-describe) ( x -- )
    {
        { [ dup integer? ] [ dup number-base get >base write
                             "  (hex " write 16 >base write ")" write nl ] }
        { [ dup float?   ] [ number>string write nl ] }
        { [ dup string?  ] [ dup length number>string write " chars: " write
                             dup write nl >byte-array nf-hexdump ] }
        { [ dup nf-addr? ] [ ba>> dup length number>string write " bytes" write nl
                             nf-hexdump ] }
        { [ dup quotation? ] [ drop "<quotation>" write nl ] }
        { [ dup word?    ] [ name>> write nl ] }
        [ class-of name>> "<" write write ">" write nl ]
    } cond ;

: nf-dump ( x -- x )
    dup (nf-type-name) write "  " write dup (nf-describe) ;

: nf-.s ( -- )
    get-datastack
    "<" write dup length number>string write "> " write
    [ (nf-pp1) " " write ] each nl ;

: nf-words ( -- )
    "scratchpad" vocab-words [ name>> print ] each ;
"#;

fn worker_main(
    dll_path: PathBuf,
    image_path: PathBuf,
    io_state: Arc<IoState>,
    cmd_rx: Receiver<Command>,
    interrupter_slot: Arc<Mutex<Option<Interrupter>>>,
) {
    trace_init();
    trace("worker_main", "entry");
    // Install WF64's VEH (idempotent, process-global) but do NOT
    // register this thread as the worker.  Reason: Factor's
    // data-stack underflow is a guard-page page-fault that
    // Factor's exception_handler converts to
    // ERROR_DATASTACK_UNDERFLOW.  Windows runs VEHs before SEHs,
    // so registering this thread would intercept the fault before
    // Factor sees it.
    //
    // The patched factor.dll (vm-build/vm/factor.cpp) now wraps
    // the eval-callback invocation in install_seh_table /
    // uninstall_seh_table — the same SEH-unwind-table machinery
    // c_to_factor_toplevel uses for STARTUP/SHUTDOWN.  With that,
    // Factor's SEH handler is in the chain for faults inside our
    // listener, so underflow / overflow / FP traps unwind to
    // Factor's recover and become clean "ANS error -N" messages.
    //
    // Truly unrecoverable Rust panics on this thread are still
    // caught by catch_unwind at the Session::new spawn site.
    wf64::igui::crash_handler::install();

    // factor.dll likes its own directory as cwd (looks for sibling
    // DLLs etc.).  Mirror what embed-smoke does.
    if let Some(dir) = dll_path.parent() {
        let _ = std::env::set_current_dir(dir);
    }

    // SAFETY: standard libloading shape.  The Library must outlive
    // every Symbol we extract from it; we keep it on the stack of
    // this function for that reason.
    let lib = unsafe { Library::new(&dll_path) }
        .expect("LoadLibrary factor.dll");
    let api = unsafe {
        NfApi {
            new_vm:             lib.get(b"nf_new_vm\0").unwrap(),
            default_parameters: lib.get(b"nf_default_parameters\0").unwrap(),
            free_parameters:    lib.get(b"nf_free_parameters\0").unwrap(),
            params_set_image:   lib.get(b"nf_params_set_image_path\0").unwrap(),
            params_set_signals: lib.get(b"nf_params_set_signals\0").unwrap(),
            init_factor:        lib.get(b"nf_init_factor\0").unwrap(),
            run_startup:        lib.get(b"nf_run_startup\0").unwrap(),
            eval_string:        lib.get(b"nf_eval_string\0").unwrap(),
            eval_free:           lib.get(b"nf_eval_free\0").unwrap(),
            enqueue_interrupt:   lib.get(b"nf_enqueue_interrupt\0").unwrap(),
        }
    };

    unsafe {
        let vm = (api.new_vm)();
        let params = (api.default_parameters)();
        let img_w: Vec<u16> = {
            use std::os::windows::ffi::OsStrExt;
            let mut v: Vec<u16> = image_path.as_os_str().encode_wide().collect();
            v.push(0);
            v
        };
        (api.params_set_image)(params, img_w.as_ptr());
        // Signals OFF.  Factor's safepoint mechanism (the FEP-
        // interrupt we exposed as `nf_enqueue_interrupt`) requires
        // signals=1 AND Factor's SEH function tables to be
        // installed during the eval — but the embedded eval-callback
        // path goes via `factor_eval_string` → raw function-pointer
        // call, bypassing the `c_to_factor_toplevel` shell that
        // installs the SEH tables.  Result: the safepoint guard-
        // page fault propagates as an AV, killing the process.
        //
        // Filed as #52 — proper fix needs a small VM-side patch
        // that wraps `nf_eval_string`'s body with the same SEH
        // function-table installation that `c_to_factor_toplevel`
        // does.  Until then, hangs in long-running user code
        // require Session::drop + Session::new (which works), and
        // `nf_enqueue_interrupt` is wired but inert.
        (api.params_set_signals)(params, 0);
        (api.init_factor)(vm, params);
        (api.free_parameters)(params);
        (api.run_startup)(vm);

        // Publish the interrupt handle now that the VM is up.  The
        // host (Session::eval) uses this to call enqueue_interrupt
        // on timeout, asking Factor's safepoint machinery to raise
        // ERROR_INTERRUPT at the worker's next safepoint.
        {
            let intr = Interrupter {
                vm,
                enqueue: *api.enqueue_interrupt,
            };
            *interrupter_slot.lock().unwrap() = Some(intr);
        }

        // Register the host binary as the "nf-host" FFI library
        // and bind Factor's I/O streams to host-routed tuples.
        // After this, EMIT / CR / TYPE / `.` and any other word
        // that ultimately calls Factor's `write` flow through
        // `nf_rt_write_char` and land in the session's IoMode sink.
        let exe = std::env::current_exe()
            .expect("current_exe lookup");
        // Factor's parser dislikes raw backslashes in paths;
        // forward slashes work fine on Windows too.
        let exe_str = exe.to_string_lossy().replace('\\', "/");
        // Register "nf-host" as a library tuple with dll: f.
        // Factor's ffi_dlsym falls back to GetModuleHandle(NULL)
        // — the running process — when the library's dll slot is
        // null, which is exactly what we want: the rt_* symbols
        // are exported from this test/CLI binary itself, not from
        // a separate .dll file.  Using `add-library` instead
        // would `LoadLibraryEx` the path and produce a different
        // module handle whose export table the symbols don't
        // appear in.
        let _ = exe_str;  // kept for diagnostics; not used by Factor
        // Use the real exe path and let Factor's add-library
        // call LoadLibraryEx on it.  Per MSDN, LoadLibraryEx on
        // an already-loaded module returns the existing handle,
        // which should be the same module GetModuleHandle(NULL)
        // gives us — so GetProcAddress on it should find rt_*.
        // Listener architecture (mirrors Factor's stock listener).
        //
        // Setup phase: register host library, install host streams,
        // and inject the type-introspection helpers (nf-typeof + the
        // type-code constants + the boolean predicates).  These live
        // in forth.runtime alongside the rest of our runtime words.
        //
        // We define them at session boot rather than baking into the
        // shipped image because that lets us iterate without paying a
        // full image-bootstrap cycle.  If we ever rebuild the image
        // these definitions become persistent and the boot-time eval
        // is a harmless redefinition (Factor accepts re-`:` of the
        // same name with a warning we suppress).
        //
        // Type codes: small, stable integers picked so user code can
        // CASE on them.  100s left for future tuple types.
        let setup = format!(
            "USING: alien alien.libraries forth.runtime kernel \
                    math classes math.parser strings \
                    quotations words combinators ;  \
             \"nf-host\" \"{exe}\" cdecl add-library  \
             install-host-streams  \
             IN: forth.runtime  \
             CONSTANT: int-type     1  \
             CONSTANT: float-type   2  \
             CONSTANT: string-type  3  \
             CONSTANT: xt-type      4  \
             CONSTANT: addr-type    5  \
             CONSTANT: other-type   99  \
             : nf-typeof ( x -- code )  \
                 {{  \
                     {{ [ dup integer?   ] [ drop int-type    ] }}  \
                     {{ [ dup float?     ] [ drop float-type  ] }}  \
                     {{ [ dup string?    ] [ drop string-type ] }}  \
                     {{ [ dup quotation? ] [ drop xt-type     ] }}  \
                     {{ [ dup word?      ] [ drop xt-type     ] }}  \
                     {{ [ dup nf-addr?   ] [ drop addr-type   ] }}  \
                     [ drop other-type ]  \
                 }} cond ;  \
             : nf-int?     ( x -- f ) integer?      bool>flag ;  \
             : nf-float?   ( x -- f ) float?        bool>flag ;  \
             : nf-string?  ( x -- f ) string?       bool>flag ;  \
             : nf-xt?      ( x -- f ) dup quotation? swap word? or bool>flag ;  \
             : nf-addr-pred? ( x -- f ) nf-addr?    bool>flag ;",
            exe = exe_str,
        );
        trace("worker_main", "running setup eval");
        let setup_result = eval_inner(&api, vm, &setup);
        trace("worker_main", &format!(
            "setup done; output={:?}",
            setup_result.interpreter_output));
        if !setup_result.interpreter_output.trim().is_empty() {
            eprintln!(
                "[session] WARNING host-library setup output: {}",
                setup_result.interpreter_output,
            );
        }

        // Programming-Tools word set — .S / WORDS / DUMP.  Defined as
        // a SECOND boot eval (a plain raw string, no format! brace
        // escaping) so the Factor source reads naturally.  Like the
        // type-introspection helpers above, these live in
        // forth.runtime and are boot-defined so we can iterate
        // without an image rebuild.
        //
        // The headline word is DUMP, deliberately re-imagined for our
        // value model: ANS `DUMP ( addr u -- )` hex-dumps raw memory,
        // but our addresses are opaque nf-addr tuples — dumping one
        // would print Factor internals, not user data.  Instead our
        // `DUMP ( x -- x )` inspects the VALUE on top of the stack:
        // it prints a type tag + value, and for strings / nf-addrs it
        // appends a classic 16-byte hex+ASCII dump of the backing
        // bytes.  Non-destructive (leaves x) so it drops into a
        // pipeline as a debugging tap without disturbing the stack.
        let tools_setup = TOOLS_SETUP_SRC;
        trace("worker_main", "running tools setup eval");
        let tools_result = eval_inner(&api, vm, tools_setup);
        trace("worker_main", &format!(
            "tools setup done; output={:?}",
            tools_result.interpreter_output));
        if !tools_result.interpreter_output.trim().is_empty() {
            eprintln!(
                "[session] WARNING tools setup output: {}",
                tools_result.interpreter_output,
            );
        }

        // Spawn a side thread that translates Command::Eval
        // requests into listener-queue pushes.  The worker thread
        // itself enters nf_eval_string("nf-listener-start") and
        // blocks there forever — that call drives Factor's
        // `nf-listener-loop`, which polls the listener queue via
        // FFI (nf_rt_next_command), evaluates each command with
        // `with-datastack` threading the persistent stack through,
        // and signals completion via nf_rt_command_done.
        //
        // We can't share the worker thread between blocking
        // inside Factor AND draining cmd_rx, so the dispatcher
        // runs on a sibling thread that ALSO holds an
        // Arc<IoState> (via the installed CURRENT slot).
        let io_for_dispatcher = io_state.clone();
        let dispatcher = std::thread::Builder::new()
            .name("nf-session-dispatcher".into())
            .spawn(move || {
                trace("dispatcher", "started, waiting on cmd_rx");
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        Command::Eval { source, reply } => {
                            trace("dispatcher", &format!(
                                "got Eval, {} bytes", source.len()));
                            // Prepend a rebind of the host streams
                            // into the current dynamic scope.  See
                            // the long-form comment on
                            // `rebind-host-streams` in runtime.factor.
                            let wrapped = format!(
                                "USING: forth.runtime ; rebind-host-streams\n{source}",
                            );
                            // Push the command, mark "not done",
                            // wait for the Factor listener to
                            // signal completion.
                            {
                                let mut done = io_for_dispatcher.listener_done.lock().unwrap();
                                *done = false;
                            }
                            {
                                let mut pending = io_for_dispatcher.listener_pending.lock().unwrap();
                                *pending = Some(wrapped);
                                io_for_dispatcher.listener_pending_cv.notify_all();
                            }
                            trace("dispatcher", "pushed to listener_pending, waiting for done");
                            // Wait for the listener to signal done.
                            {
                                let mut done = io_for_dispatcher.listener_done.lock().unwrap();
                                while !*done {
                                    done = io_for_dispatcher.listener_done_cv.wait(done).unwrap();
                                }
                            }
                            trace("dispatcher", "listener signaled done");
                            // Flush any partial-line buffer to the
                            // host console.  Forth output frequently
                            // doesn't end with a newline (e.g. `.`
                            // emits "<number> " with trailing space,
                            // no newline) — without this flush the
                            // IDE would never show that text.
                            {
                                let mut flush = io_for_dispatcher.output_flusher.lock().unwrap();
                                (flush)();
                            }
                            trace("dispatcher", "sending reply");
                            let _ = reply.send(EvalResult {
                                interpreter_output: String::new(),
                            });
                        }
                        Command::Shutdown => {
                            // Signal the listener to exit.
                            {
                                let mut done = io_for_dispatcher.listener_done.lock().unwrap();
                                *done = false;
                            }
                            {
                                let mut pending = io_for_dispatcher.listener_pending.lock().unwrap();
                                *pending = Some("__exit__".to_string());
                                io_for_dispatcher.listener_pending_cv.notify_all();
                            }
                            // Don't wait — the listener will exit
                            // its nf_eval_string call and the
                            // worker thread will fall through.
                            break;
                        }
                    }
                }
            })
            .expect("spawn dispatcher");

        // Start the listener loop on THIS thread (the worker thread
        // that owns the VM).  This call blocks until nf-listener-loop
        // sees a "__exit__" command and returns.
        trace("worker_main", "entering eval_inner(nf-listener-start)");
        let listener_result = eval_inner(
            &api, vm,
            "USING: forth.runtime ; nf-listener-start",
        );
        trace("worker_main", &format!(
            "eval_inner(nf-listener-start) returned: output={:?}",
            listener_result.interpreter_output));

        // Listener exited; join the dispatcher (it should already
        // have exited too — its loop breaks on Shutdown).
        let _ = dispatcher.join();
        trace("worker_main", "exit");
        // Factor doesn't expose a clean teardown; on process exit
        // the OS reaps the VM.  Worker exits.
    }
}

unsafe fn eval_inner(api: &NfApi, vm: *mut c_void, source: &str) -> EvalResult {
    trace("eval_inner", &format!(
        "calling nf_eval_string with {} bytes: {:?}",
        source.len(),
        &source[..source.len().min(80)]));
    let c = CString::new(source).expect("eval source contains NUL");
    let raw = (api.eval_string)(vm, c.as_ptr() as *mut c_char);
    trace("eval_inner", &format!(
        "nf_eval_string returned, raw is {}",
        if raw.is_null() { "NULL" } else { "non-null" }));
    if raw.is_null() {
        return EvalResult { interpreter_output: String::new() };
    }
    let captured = CStr::from_ptr(raw).to_string_lossy().into_owned();
    (api.eval_free)(vm, raw);
    trace("eval_inner", &format!("captured {} bytes: {:?}",
        captured.len(), &captured[..captured.len().min(200)]));
    EvalResult { interpreter_output: captured }
}

// ─── Shared I/O state ───────────────────────────────────────────────────────

/// State the extern callbacks read.  Shared between the host's
/// main thread (writes input via feed_input) and the worker
/// (reads via nf_rt_read_char).  Outputs flow the other direction.
struct IoState {
    input_q:     Mutex<VecDeque<u8>>,
    input_cv:    Condvar,
    input_closed: AtomicBool,
    /// What the extern nf_rt_write_char should do with each byte.
    output_writer: Mutex<Box<dyn FnMut(u8) + Send>>,
    /// Optional end-of-eval flusher.  Test/Terminal modes set this
    /// to a no-op.  Gui mode wires it to the host's "push any
    /// buffered partial-line to the console" closure, called by
    /// the dispatcher after each eval completes.
    output_flusher: Mutex<Box<dyn FnMut() + Send>>,
    /// For Test mode: the host can pull captured output here.
    /// `None` for non-capturing modes.
    captured_output: Option<Arc<Mutex<Vec<u8>>>>,
    /// Listener architecture (mirroring Factor's stock listener
    /// loop): the worker thread runs Factor's `nf-listener-loop`
    /// which polls this slot via `nf_rt_next_command`, evaluates
    /// the command with `with-datastack` threading a persistent
    /// `datastack` value through each iteration, and signals
    /// completion via `nf_rt_command_done`.
    ///
    /// `pending`: source the host wants the listener to evaluate.
    ///            `None` means nothing pending; `Some("__exit__")`
    ///            asks the listener to stop.
    /// `done`:    set true by the Factor side after each eval to
    ///            unblock the host.
    listener_pending: Mutex<Option<String>>,
    listener_pending_cv: Condvar,
    listener_done: Mutex<bool>,
    listener_done_cv: Condvar,
}

impl IoState {
    fn new_listener_fields() -> (Mutex<Option<String>>, Condvar, Mutex<bool>, Condvar) {
        (Mutex::new(None), Condvar::new(), Mutex::new(false), Condvar::new())
    }

    fn from_mode(mode: IoMode) -> Self {
        let (lp, lpc, ld, ldc) = Self::new_listener_fields();
        let noop_flush: Box<dyn FnMut() + Send> = Box::new(|| {});
        let (writer, flusher, captured): (
            Box<dyn FnMut(u8) + Send>,
            Box<dyn FnMut() + Send>,
            Option<Arc<Mutex<Vec<u8>>>>,
        ) = match mode {
            IoMode::Test { input, output } => {
                let captured_for_writer = output.clone();
                let w: Box<dyn FnMut(u8) + Send> = Box::new(move |ch| {
                    captured_for_writer.lock().unwrap().push(ch);
                });
                let initial = input;
                let captured = Some(output);
                let state = IoState {
                    input_q: Mutex::new(initial.into_iter().collect()),
                    input_cv: Condvar::new(),
                    input_closed: AtomicBool::new(true), // pre-fed; EOF after consumed
                    output_writer: Mutex::new(w),
                    output_flusher: Mutex::new(noop_flush),
                    captured_output: captured,
                    listener_pending: lp,
                    listener_pending_cv: lpc,
                    listener_done: ld,
                    listener_done_cv: ldc,
                };
                return state;
            }
            IoMode::Terminal => {
                use std::io::Write;
                let w: Box<dyn FnMut(u8) + Send> = Box::new(move |ch| {
                    let _ = std::io::stdout().write_all(&[ch]);
                });
                let f: Box<dyn FnMut() + Send> = Box::new(|| {
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                });
                (w, f, None)
            }
            IoMode::Gui { on_write, on_flush } => (on_write, on_flush, None),
        };
        IoState {
            input_q: Mutex::new(VecDeque::new()),
            input_cv: Condvar::new(),
            input_closed: AtomicBool::new(false),
            output_writer: Mutex::new(writer),
            output_flusher: Mutex::new(flusher),
            captured_output: captured,
            listener_pending: lp,
            listener_pending_cv: lpc,
            listener_done: ld,
            listener_done_cv: ldc,
        }
    }
}

// ─── Global current-session pointer ─────────────────────────────────────────
//
// The extern functions called from Factor (nf_rt_read_char etc.)
// have no way to receive a session pointer; they look at a
// process-wide global.  We allow only one active Session at a
// time, matching Factor's single-VM-per-process constraint.

static CURRENT: OnceLock<Mutex<Option<Arc<IoState>>>> = OnceLock::new();

/// Cross-thread interrupt handle for the active Session.  Set by
/// Session::new, cleared by Drop.  Lets the GUI thread (or any
/// other thread) signal the running eval to abort at the next
/// safepoint without having to route through the IDE worker's
/// event queue — which is the whole point of needing an
/// interrupt in the first place (that worker is blocked inside
/// session.eval waiting for the very eval we want to stop).
static CURRENT_INTERRUPTER: OnceLock<Mutex<Option<Arc<Mutex<Option<Interrupter>>>>>>
    = OnceLock::new();

/// Trigger an interrupt on the currently-installed Session.
/// No-op if no Session is alive.  Safe to call from any thread.
pub fn interrupt_current_session() {
    let slot = CURRENT_INTERRUPTER.get_or_init(|| Mutex::new(None));
    let g = match slot.lock() { Ok(g) => g, Err(_) => return };
    let Some(arc) = g.as_ref() else { return };
    let Ok(inner) = arc.lock() else { return };
    if let Some(intr) = inner.as_ref() {
        trace("interrupt_current_session", "triggering Factor safepoint");
        intr.trigger();
    }
}

fn install_current(state: Arc<IoState>) -> Result<(), SessionError> {
    let slot = CURRENT.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().unwrap();
    if guard.is_some() {
        return Err(SessionError::AlreadyRunning);
    }
    *guard = Some(state);
    Ok(())
}

fn clear_current() {
    if let Some(slot) = CURRENT.get() {
        *slot.lock().unwrap() = None;
    }
}

fn with_current<R>(f: impl FnOnce(&Arc<IoState>) -> R) -> Option<R> {
    let slot = CURRENT.get()?;
    let guard = slot.lock().unwrap();
    guard.as_ref().map(f)
}

// ─── Extern callbacks Factor calls via alien.libraries ──────────────────────
//
// These are dispatched against the current IoState.  None of them
// hold the CURRENT Mutex across blocking operations — they snapshot
// the Arc<IoState> first and release the lock before any wait, so
// the host's main thread can still call feed_input while the worker
// is blocked in nf_rt_read_char.

// Force the linker to keep our extern "C" callbacks even though
// no Rust code references them — Factor's GetProcAddress is the
// only consumer, and dead-code elimination would otherwise drop
// the symbols from binaries that don't reference them directly.
// Each `static` references the corresponding function so the
// symbol survives.
#[used]
static _KEEP_READ_CHAR:  unsafe extern "C-unwind" fn() -> i64 = nf_rt_read_char;
#[used]
static _KEEP_WRITE_CHAR: unsafe extern "C-unwind" fn(i64) = nf_rt_write_char;
#[used]
static _KEEP_READ_LINE:  unsafe extern "C-unwind" fn(*mut u8, i64) -> i64 = nf_rt_read_line;
#[used]
static _KEEP_CHECK_DOUBLE: unsafe extern "C-unwind" fn(f64) -> f64 = rt_check_double;
#[used]
static _KEEP_EMIT_DOUBLE:  unsafe extern "C-unwind" fn(f64) = rt_emit_double;
#[used]
static _KEEP_NEXT_COMMAND: extern "C-unwind" fn() -> *mut c_char = nf_rt_next_command;
#[used]
static _KEEP_COMMAND_DONE: extern "C-unwind" fn() = nf_rt_command_done;
#[used]
static _KEEP_STACK_BEGIN:  extern "C-unwind" fn() = nf_rt_stack_begin;
#[used]
static _KEEP_STACK_ITEM:   extern "C-unwind" fn(i64) = nf_rt_stack_item;
#[used]
static _KEEP_STACK_END:    extern "C-unwind" fn() = nf_rt_stack_end;
#[used]
static _KEEP_COMPILE_ANS:  unsafe extern "C-unwind" fn(*const c_char) -> *mut c_char = rt_compile_ans;

/// Read one byte from the input queue.  Blocks if empty and
/// `input_closed` is false.  Returns -1 at EOF (queue empty AND
/// closed) or if no session is active.
#[no_mangle]
pub extern "C-unwind" fn nf_rt_read_char() -> i64 {
    let state = match with_current(|s| s.clone()) {
        Some(s) => s,
        None => return -1,
    };
    let mut q = state.input_q.lock().unwrap();
    loop {
        if let Some(b) = q.pop_front() {
            return b as i64;
        }
        if state.input_closed.load(Ordering::Acquire) {
            return -1;
        }
        q = state.input_cv.wait(q).unwrap();
    }
}

/// Write one byte to the session's output sink.  Best-effort — if
/// no session is installed, the byte is dropped.
#[no_mangle]
pub extern "C-unwind" fn nf_rt_write_char(ch: i64) {
    // First byte of each batch logs; subsequent ones don't (would
    // be too noisy).  Use a sampled trace: only every ~16 chars or
    // newlines.
    if ch == b'\n' as i64 || (ch & 0xF) == 0 {
        trace("nf_rt_write_char", &format!("ch={ch} ({:?})",
            if ch >= 0 && ch <= 127 { ch as u8 as char } else { '?' }));
    }
    let _ = with_current(|state| {
        let mut writer = state.output_writer.lock().unwrap();
        (writer)(ch as u8);
    });
}

/// Listener FFI — Factor's `nf-listener-loop` calls this to fetch
/// the next user source string.  Blocks until the host pushes one
/// via `Session::eval`.  Returns a malloc'd c-string; the Factor
/// caller is responsible for freeing it via `free`.  On shutdown,
/// returns NULL — the Factor loop treats this as "exit".
#[no_mangle]
pub extern "C-unwind" fn nf_rt_next_command() -> *mut c_char {
    trace("nf_rt_next_command", "entry, blocking on listener_pending");
    let result = with_current(|state| {
        let state = state.clone();
        let mut guard = state.listener_pending.lock().unwrap();
        while guard.is_none() {
            guard = state.listener_pending_cv.wait(guard).unwrap();
        }
        guard.take()
    });
    trace("nf_rt_next_command", &format!(
        "unblocked, got {:?}", result.as_ref().map(|o| o.as_ref().map(|s| s.len()))));
    match result.flatten() {
        Some(s) if s == "__exit__" => std::ptr::null_mut(),
        Some(s) => {
            // Heap-allocate a NUL-terminated copy.  Factor's caller
            // takes ownership (we declare the return as c-string
            // which Factor box-copies into its own GC'd string).
            // Caller frees via libc free on the c-string box's
            // tear-down.  Using libc malloc keeps Factor's
            // expectation that the pointer was malloc'd consistent.
            let cs = match CString::new(s) {
                Ok(c) => c,
                Err(_) => return std::ptr::null_mut(),
            };
            let len = cs.as_bytes_with_nul().len();
            unsafe {
                let p = libc_malloc(len) as *mut c_char;
                if p.is_null() {
                    return std::ptr::null_mut();
                }
                std::ptr::copy_nonoverlapping(
                    cs.as_ptr() as *const u8, p as *mut u8, len,
                );
                p
            }
        }
        None => std::ptr::null_mut(),
    }
}

/// Listener FFI — Factor's `nf-listener-loop` calls this after
/// each eval completes (or fails).  Unblocks the host thread
/// waiting in `Session::eval`.
#[no_mangle]
pub extern "C-unwind" fn nf_rt_command_done() {
    trace("nf_rt_command_done", "entry, signaling listener_done");
    let _ = with_current(|state| {
        let state = state.clone();
        let mut done = state.listener_done.lock().unwrap();
        *done = true;
        state.listener_done_cv.notify_all();
    });
    trace("nf_rt_command_done", "signaled");
}

// ─── Stack-snapshot FFI ─────────────────────────────────────────────────────
//
// Factor's `nf-publish-datastack` calls these three between
// each eval and the matching `nf_rt_command_done`.  Begin
// resets a thread-local buffer, item appends, end publishes
// the snapshot to wf64::igui::stack_view.  Top-of-stack-first
// ordering matches what stack_view::publish expects.

thread_local! {
    static STACK_SNAP_BUF: std::cell::RefCell<Vec<i64>> =
        std::cell::RefCell::new(Vec::new());
}

#[no_mangle]
pub extern "C-unwind" fn nf_rt_stack_begin() {
    STACK_SNAP_BUF.with(|b| b.borrow_mut().clear());
}

#[no_mangle]
pub extern "C-unwind" fn nf_rt_stack_item(v: i64) {
    STACK_SNAP_BUF.with(|b| b.borrow_mut().push(v));
}

#[no_mangle]
pub extern "C-unwind" fn nf_rt_stack_end() {
    // Factor's data stack is bottom-up (top-of-stack is the
    // last item).  stack_view::publish wants top-first, so we
    // reverse the accumulated buffer before shipping.
    let mut cells: Vec<i64> = STACK_SNAP_BUF.with(|b| b.borrow().clone());
    cells.reverse();
    trace("nf_rt_stack_end", &format!("publishing {} cells", cells.len()));
    // wf64's stack_view is the iGui's data-stack pane.  Posting
    // an update is best-effort — if the pane isn't open it's a
    // no-op (just stashes the snapshot).
    wf64::igui::stack_view::publish(cells);
}

extern "C" {
    #[link_name = "malloc"]
    fn libc_malloc(size: usize) -> *mut c_void;
}

/// Round-trip test for the Win64 double calling convention.
/// Factor passes `x` via XMM0; we return `x * 2.0 + 1.0` via XMM0.
/// The test verifies a known-value output matches exactly — any
/// precision loss in the marshaling shows up as a bit mismatch.
#[no_mangle]
pub extern "C-unwind" fn rt_check_double(x: f64) -> f64 {
    x * 2.0 + 1.0
}

/// Write the 8 IEEE-754 little-endian bytes of `x` to the session
/// output sink.  Lets a test push a known double through the FFI
/// and recover the original bits on the Rust side to verify
/// byte-identical transmission.
#[no_mangle]
pub extern "C-unwind" fn rt_emit_double(x: f64) {
    let bytes = x.to_le_bytes();
    let _ = with_current(|state| {
        let mut writer = state.output_writer.lock().unwrap();
        for &b in &bytes {
            (writer)(b);
        }
    });
}

/// Compile an ANS Forth source file to Factor IR.  Called by
/// `INCLUDED`: Factor reads a (c-addr u) path, hands us the bytes,
/// we read the file, run it through the NewFactor compiler, and
/// hand back the resulting Factor IR as a malloc'd C string.
/// The caller is responsible for freeing the returned pointer
/// (via Factor's `alien.libraries:malloc-string` ownership).
///
/// On error (file not found, malformed source, etc.) returns a
/// Factor IR snippet that prints an error message — the eval that
/// runs the snippet then produces visible diagnostics rather than
/// crashing the session.
#[no_mangle]
pub extern "C-unwind" fn rt_compile_ans(path_cstr: *const c_char) -> *mut c_char {
    let result: String = (|| -> Result<String, String> {
        if path_cstr.is_null() {
            return Err("INCLUDED: null path".to_string());
        }
        let path = unsafe { CStr::from_ptr(path_cstr) }
            .to_str()
            .map_err(|e| format!("INCLUDED: path not UTF-8: {e}"))?
            .to_string();
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("INCLUDED: cannot read {path}: {e}"))?;
        crate::compiler::compile(&contents)
            .map_err(|e| format!("INCLUDED: compile failed for {path}: {e}"))
    })().unwrap_or_else(|err_msg| {
        // Return a Factor snippet that prints the error.  The
        // outer (eval) will run it; user sees the message via
        // the captured host streams.
        let escaped = err_msg.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"INCLUDED: {escaped}\\n\" forth.runtime:print-string\n")
    });

    match CString::new(result) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),  // contained internal NUL — unrecoverable
    }
}

/// Read up to `max` bytes into `buf`, stopping at newline (which
/// is consumed but not stored).  Returns the actual count.  This
/// maps onto ANS ACCEPT.
#[no_mangle]
pub extern "C-unwind" fn nf_rt_read_line(buf: *mut u8, max: i64) -> i64 {
    if buf.is_null() || max <= 0 { return 0; }
    let state = match with_current(|s| s.clone()) {
        Some(s) => s,
        None => return 0,
    };
    let mut count: i64 = 0;
    let max = max as isize;
    while count < max as i64 {
        let mut q = state.input_q.lock().unwrap();
        let ch = loop {
            if let Some(b) = q.pop_front() { break b; }
            if state.input_closed.load(Ordering::Acquire) {
                return count;
            }
            q = state.input_cv.wait(q).unwrap();
        };
        if ch == b'\n' { break; }
        unsafe { *buf.offset(count as isize) = ch; }
        count += 1;
    }
    count
}

// ─── Watchdog (per-eval timeout) ────────────────────────────────────────────

// The old `Watchdog` struct (which called `std::process::abort()`
// on timeout, killing the host process along with the Factor VM)
// has been retired.  Session::eval now uses `recv_timeout`
// directly and marks the session as `DeathCause::Timeout` —
// the worker thread is left to leak, but the host stays alive
// and can spawn a fresh Session.  Trade-off documented; see #34
// journal.
