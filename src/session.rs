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
//! functions — `rt_read_char`, `rt_write_char`, `rt_read_line` —
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
//!   ├── while running, Factor may call rt_read_char/rt_write_char
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
    Gui {
        on_write: Box<dyn FnMut(u8) + Send>,
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
    /// Convenience: standard paths under the crate's manifest dir.
    pub fn defaults_for_crate(mode: IoMode) -> Self {
        let manifest = env!("CARGO_MANIFEST_DIR");
        Self {
            dll_path:   PathBuf::from(manifest).join("vm-build").join("factor.dll"),
            image_path: PathBuf::from(manifest).join("images").join("nf-mandelbrot.image"),
            mode,
            eval_timeout: Duration::from_secs(20),
        }
    }
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
}

impl Session {
    pub fn new(opts: SessionOpts) -> Result<Self, SessionError> {
        let SessionOpts { dll_path, image_path, mode, eval_timeout } = opts;

        // Validate paths up front so we fail clearly rather than
        // in worker-thread land.
        if !dll_path.exists() {
            return Err(SessionError::DllNotFound(dll_path));
        }
        if !image_path.exists() {
            return Err(SessionError::ImageNotFound(image_path));
        }

        // Build the shared I/O state from the mode.
        let io_state = Arc::new(IoState::from_mode(mode));

        // Install as the process-wide current session.  Extern
        // functions look here when Factor calls them.
        install_current(io_state.clone())?;

        // Spawn the worker.
        let (cmd_tx, cmd_rx) = channel();
        let state_for_worker = io_state.clone();
        let worker = std::thread::Builder::new()
            .name("nf-session-worker".into())
            .spawn(move || {
                worker_main(dll_path, image_path, state_for_worker, cmd_rx);
            })
            .map_err(|e| SessionError::WorkerSpawn(e.to_string()))?;

        Ok(Session { cmd_tx, io_state, worker: Some(worker), eval_timeout })
    }

    /// Send source to the worker for evaluation.  Blocks until the
    /// worker returns a result, or aborts the process if the per-
    /// eval timeout fires.
    pub fn eval(&self, source: &str) -> Result<EvalResult, SessionError> {
        let _wd = Watchdog::arm("session.eval", self.eval_timeout);
        let (reply_tx, reply_rx) = channel();
        self.cmd_tx.send(Command::Eval {
            source: source.to_string(),
            reply: reply_tx,
        }).map_err(|_| SessionError::WorkerGone)?;
        reply_rx.recv().map_err(|_| SessionError::WorkerGone)
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

impl Drop for Session {
    fn drop(&mut self) {
        // Tell the worker to stop, join it, and clear the global.
        let _ = self.cmd_tx.send(Command::Shutdown);
        if let Some(h) = self.worker.take() {
            // Joining might block if the worker is stuck inside a
            // Factor call; the per-eval watchdog should have
            // aborted before we reach Drop in that case.
            let _ = h.join();
        }
        clear_current();
    }
}

// ─── Errors ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SessionError {
    DllNotFound(PathBuf),
    ImageNotFound(PathBuf),
    WorkerSpawn(String),
    WorkerGone,
    AlreadyRunning,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::DllNotFound(p)   => write!(f, "factor.dll not found at {}", p.display()),
            SessionError::ImageNotFound(p) => write!(f, "image not found at {}", p.display()),
            SessionError::WorkerSpawn(e)   => write!(f, "spawn worker thread: {e}"),
            SessionError::WorkerGone       => write!(f, "session worker thread terminated"),
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

/// Factor's embedded API exports we need to drive the VM.
struct NfApi<'lib> {
    new_vm:              Symbol<'lib, unsafe extern "C" fn() -> *mut c_void>,
    default_parameters:  Symbol<'lib, unsafe extern "C" fn() -> *mut c_void>,
    free_parameters:     Symbol<'lib, unsafe extern "C" fn(*mut c_void)>,
    params_set_image:    Symbol<'lib, unsafe extern "C" fn(*mut c_void, *const u16)>,
    params_set_signals:  Symbol<'lib, unsafe extern "C" fn(*mut c_void, c_int)>,
    init_factor:         Symbol<'lib, unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    run_startup:         Symbol<'lib, unsafe extern "C" fn(*mut c_void)>,
    eval_string:         Symbol<'lib, unsafe extern "C" fn(*mut c_void, *mut c_char) -> *mut c_char>,
    eval_free:           Symbol<'lib, unsafe extern "C" fn(*mut c_void, *mut c_char)>,
}

fn worker_main(
    dll_path: PathBuf,
    image_path: PathBuf,
    _state: Arc<IoState>,
    cmd_rx: Receiver<Command>,
) {
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
            eval_free:          lib.get(b"nf_eval_free\0").unwrap(),
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
        (api.params_set_signals)(params, 0);
        (api.init_factor)(vm, params);
        (api.free_parameters)(params);
        (api.run_startup)(vm);

        // Command dispatch loop.
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                Command::Eval { source, reply } => {
                    let result = eval_inner(&api, vm, &source);
                    let _ = reply.send(result);
                }
                Command::Shutdown => break,
            }
        }
        // Factor doesn't expose a clean teardown; on process exit
        // the OS reaps the VM.  Worker exits.
    }
}

unsafe fn eval_inner(api: &NfApi, vm: *mut c_void, source: &str) -> EvalResult {
    let c = CString::new(source).expect("eval source contains NUL");
    let raw = (api.eval_string)(vm, c.as_ptr() as *mut c_char);
    if raw.is_null() {
        return EvalResult { interpreter_output: String::new() };
    }
    let captured = CStr::from_ptr(raw).to_string_lossy().into_owned();
    (api.eval_free)(vm, raw);
    EvalResult { interpreter_output: captured }
}

// ─── Shared I/O state ───────────────────────────────────────────────────────

/// State the extern callbacks read.  Shared between the host's
/// main thread (writes input via feed_input) and the worker
/// (reads via rt_read_char).  Outputs flow the other direction.
struct IoState {
    input_q:     Mutex<VecDeque<u8>>,
    input_cv:    Condvar,
    input_closed: AtomicBool,
    /// What the extern rt_write_char should do with each byte.
    output_writer: Mutex<Box<dyn FnMut(u8) + Send>>,
    /// For Test mode: the host can pull captured output here.
    /// `None` for non-capturing modes.
    captured_output: Option<Arc<Mutex<Vec<u8>>>>,
}

impl IoState {
    fn from_mode(mode: IoMode) -> Self {
        let (writer, captured): (Box<dyn FnMut(u8) + Send>, Option<Arc<Mutex<Vec<u8>>>>) =
            match mode {
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
                        captured_output: captured,
                    };
                    return state;
                }
                IoMode::Terminal => {
                    use std::io::Write;
                    let w: Box<dyn FnMut(u8) + Send> = Box::new(move |ch| {
                        let _ = std::io::stdout().write_all(&[ch]);
                    });
                    (w, None)
                }
                IoMode::Gui { on_write } => (on_write, None),
            };
        IoState {
            input_q: Mutex::new(VecDeque::new()),
            input_cv: Condvar::new(),
            input_closed: AtomicBool::new(false),
            output_writer: Mutex::new(writer),
            captured_output: captured,
        }
    }
}

// ─── Global current-session pointer ─────────────────────────────────────────
//
// The extern functions called from Factor (rt_read_char etc.)
// have no way to receive a session pointer; they look at a
// process-wide global.  We allow only one active Session at a
// time, matching Factor's single-VM-per-process constraint.

static CURRENT: OnceLock<Mutex<Option<Arc<IoState>>>> = OnceLock::new();

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
// is blocked in rt_read_char.

/// Read one byte from the input queue.  Blocks if empty and
/// `input_closed` is false.  Returns -1 at EOF (queue empty AND
/// closed) or if no session is active.
#[no_mangle]
pub extern "C" fn rt_read_char() -> i64 {
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
pub extern "C" fn rt_write_char(ch: i64) {
    let _ = with_current(|state| {
        let mut writer = state.output_writer.lock().unwrap();
        (writer)(ch as u8);
    });
}

/// Read up to `max` bytes into `buf`, stopping at newline (which
/// is consumed but not stored).  Returns the actual count.  This
/// maps onto ANS ACCEPT.
#[no_mangle]
pub extern "C" fn rt_read_line(buf: *mut u8, max: i64) -> i64 {
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

struct Watchdog { cancel: Arc<AtomicBool> }

impl Watchdog {
    fn arm(label: &str, timeout: Duration) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        let label = label.to_string();
        std::thread::Builder::new()
            .name(format!("watchdog<{label}>"))
            .spawn(move || {
                let deadline = Instant::now() + timeout;
                while Instant::now() < deadline {
                    if cancel2.load(Ordering::Relaxed) { return; }
                    std::thread::sleep(Duration::from_millis(100));
                }
                eprintln!(
                    "\n[session] *** TIMEOUT: `{label}` exceeded {:?}; aborting ***",
                    timeout,
                );
                std::process::abort();
            })
            .expect("spawn watchdog");
        Watchdog { cancel }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) { self.cancel.store(true, Ordering::Relaxed); }
}
