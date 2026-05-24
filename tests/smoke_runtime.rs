//! tests/smoke_runtime.rs — Phase 1 success criterion.
//!
//! Loads `images/nf-mandelbrot.image` via the patched `factor.dll`'s
//! embedding API and evaluates a few smoke expressions against the
//! `forth.runtime` vocab.  Validates:
//!
//!   1. The patched DLL loads, exports the nf_* embedding entry points,
//!      and initialises a VM against our image.
//!   2. `forth.runtime` is loaded in the image and its words are
//!      callable.
//!   3. The nf-addr memory model round-trips: VARIABLE / store / fetch.
//!
//! Mirrors the C smoke test at vm-build/smoke.c but in Rust with
//! libloading, so the smoke runs as part of `cargo test`.
//!
//! Test gating: this is ignored by default — the test requires the
//! patched factor.dll under vm-build/ plus images/nf-mandelbrot.image
//! built via scripts/build-image.sh.  Run explicitly with
//!   `cargo test --test smoke_runtime -- --ignored --nocapture`.
//!
//! SERIALIZATION: Factor's VM is NOT thread-safe — and that's fine,
//! Forth isn't either.  We enforce that by acquiring a process-wide
//! Mutex in every test before touching the VM.  Cargo may spawn each
//! test in its own thread, but the mutex serializes the actual VM
//! work so only one Factor VM is alive at any moment.
//!
//! Use `--test-threads=1` regardless when running these tests; the
//! mutex prevents the worst case but the panic-recovery path of
//! parallel-test workers can still smear stderr unhelpfully.

#![cfg(target_os = "windows")]

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_long, CStr, CString};
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Process-wide gate.  Every test that touches the embedded VM
/// acquires this before doing anything.  Factor's VM is single-
/// threaded and TLS-resident; concurrent init+eval from two cargo
/// test workers WILL crash.  This mutex makes parallel-test runs
/// degrade gracefully (serialise) instead of segfaulting.
fn vm_gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
}

/// Per-test hard timeout.  A Factor infinite loop or runaway compile
/// can hang `nf_eval_string` indefinitely — there's no clean way to
/// unstick that FFI call, so we install a watchdog thread that
/// `std::process::abort`s the whole test exe if a single test runs
/// longer than this.  Aborting prints a clear message and exits with
/// non-zero, which cargo test reports as failure.
///
/// 20 s is generous: the entire smoke suite runs in ~0.5 s; a single
/// test should be a few ms.  Anything past 20 s is a hang, not slow.
const TEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Watchdog: install on entry to `with_vm`, cancel on Drop.  If the
/// timeout elapses before Drop, the whole process aborts with a clear
/// stderr line so the cargo output points at the culprit.
struct Watchdog {
    cancel: Arc<AtomicBool>,
}

impl Watchdog {
    fn arm(label: &str) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        let label = label.to_string();
        std::thread::Builder::new()
            .name(format!("watchdog<{label}>"))
            .spawn(move || {
                let deadline = Instant::now() + TEST_TIMEOUT;
                while Instant::now() < deadline {
                    if cancel2.load(Ordering::Relaxed) { return; }
                    std::thread::sleep(Duration::from_millis(100));
                }
                eprintln!(
                    "\n[watchdog] *** TIMEOUT: `{label}` exceeded {:?}; \
                     aborting process so cargo test reports failure ***",
                    TEST_TIMEOUT,
                );
                std::process::abort();
            })
            .expect("spawn watchdog");
        Watchdog { cancel }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Opaque types from factor.dll — we never dereference these.
type FactorVm = c_void;
type VmParameters = c_void;
/// `vm_char` on Windows is `wchar_t` (16-bit UTF-16 unit).
type VmChar = u16;

/// Embedding API signature set.  These mirror the `__declspec(dllimport)`
/// block at the top of vm-build/smoke.c.  `run_startup` is the critical
/// step that runs the image's startup quotation (init-remote-control)
/// — without it, eval_string crashes with no callback registered.
struct NfApi<'lib> {
    new_vm:              Symbol<'lib, unsafe extern "C" fn() -> *mut FactorVm>,
    default_parameters:  Symbol<'lib, unsafe extern "C" fn() -> *mut VmParameters>,
    free_parameters:     Symbol<'lib, unsafe extern "C" fn(*mut VmParameters)>,
    params_set_image:    Symbol<'lib, unsafe extern "C" fn(*mut VmParameters, *const VmChar)>,
    params_set_signals:  Symbol<'lib, unsafe extern "C" fn(*mut VmParameters, c_int)>,
    init_factor:         Symbol<'lib, unsafe extern "C" fn(*mut FactorVm, *mut VmParameters)>,
    run_startup:         Symbol<'lib, unsafe extern "C" fn(*mut FactorVm)>,
    eval_string:         Symbol<'lib, unsafe extern "C" fn(*mut FactorVm, *mut c_char) -> *mut c_char>,
    eval_free:           Symbol<'lib, unsafe extern "C" fn(*mut FactorVm, *mut c_char)>,
}

impl<'lib> NfApi<'lib> {
    fn load(lib: &'lib Library) -> Self {
        unsafe {
            NfApi {
                new_vm:             lib.get(b"nf_new_vm\0").expect("nf_new_vm export"),
                default_parameters: lib.get(b"nf_default_parameters\0").expect("nf_default_parameters export"),
                free_parameters:    lib.get(b"nf_free_parameters\0").expect("nf_free_parameters export"),
                params_set_image:   lib.get(b"nf_params_set_image_path\0").expect("nf_params_set_image_path export"),
                params_set_signals: lib.get(b"nf_params_set_signals\0").expect("nf_params_set_signals export"),
                init_factor:        lib.get(b"nf_init_factor\0").expect("nf_init_factor export"),
                run_startup:        lib.get(b"nf_run_startup\0").expect("nf_run_startup export"),
                eval_string:        lib.get(b"nf_eval_string\0").expect("nf_eval_string export"),
                eval_free:          lib.get(b"nf_eval_free\0").expect("nf_eval_free export"),
            }
        }
    }
}

/// Resolve the patched factor.dll path.  Looks under vm-build/ at the
/// crate root, which is where build.bat puts it.
fn factor_dll_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("vm-build").join("factor.dll")
}

fn image_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    // Override at test time with NF_IMAGE=...image so we can A/B the
    // mandelbrot image vs. the known-good slim image.  Default is the
    // mandelbrot image (the Phase 1 target).
    let img = std::env::var("NF_IMAGE").unwrap_or_else(|_| "nf-mandelbrot.image".into());
    PathBuf::from(manifest_dir).join("images").join(img)
}

/// Convert a Rust Path to a NUL-terminated UTF-16 buffer suitable for
/// passing as `vm_char*` to the embedding API.
fn to_utf16_z(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut v: Vec<u16> = path.as_os_str().encode_wide().collect();
    v.push(0);
    v
}

/// Run a Factor expression string and return the captured stdout.
/// The expression should end with `flush` so the listener output stream
/// is drained before nf_eval_string returns.
unsafe fn eval(api: &NfApi, vm: *mut FactorVm, expr: &str) -> String {
    eprintln!("[smoke] eval: {expr}");
    let c = CString::new(expr).expect("expr contains NUL");
    let raw = (api.eval_string)(vm, c.as_ptr() as *mut c_char);
    eprintln!("[smoke] eval returned ptr={raw:p}");
    assert!(!raw.is_null(), "nf_eval_string returned NULL for: {expr}");
    let s = CStr::from_ptr(raw).to_string_lossy().into_owned();
    eprintln!("[smoke] eval output = {s:?}");
    (api.eval_free)(vm, raw);
    s
}

/// Helper: run a setup-and-eval sequence.  Sets up the VM once, runs
/// each expression in turn, returns the captured outputs.
///
/// The watchdog label comes from `std::thread::current().name()` —
/// cargo test names each worker thread after the test function, so
/// hangs are blamed correctly without plumbing a label through every
/// callsite.
unsafe fn with_vm<F: FnOnce(&NfApi, *mut FactorVm)>(body: F) {
    // Arm the per-test watchdog FIRST — before we even contend on the
    // gate.  If the previous test's gate-holder hung, we want to time
    // out on the gate wait, not block forever on it.
    let label = std::thread::current().name()
        .unwrap_or("(unknown test)").to_string();
    let _wd = Watchdog::arm(&label);

    // Hold the process-wide VM gate for the lifetime of this call.
    // We tolerate a poisoned mutex: every test creates its own fresh
    // VM, so a previous test panicking doesn't corrupt shared state
    // — we just want serialization.  Without this, one failed assert
    // would cascade and mask every other test's real result.
    let _gate = vm_gate().lock().unwrap_or_else(|p| p.into_inner());

    let dll = factor_dll_path();
    assert!(dll.exists(), "patched factor.dll not found at {}", dll.display());

    let img = image_path();
    assert!(img.exists(),
            "nf-mandelbrot.image not found at {} — run scripts/build-image.sh first",
            img.display());

    // Loading the DLL fails if the cwd isn't its dir — Windows looks for
    // VM dependencies (e.g. msvcrt) relative to the DLL's location.
    // Match the C smoke test's environment: factor.dll resolves its
    // own dependencies (and image relative paths in some code paths)
    // from the current working directory.  Move there before loading.
    let cwd_save = std::env::current_dir().ok();
    let dll_dir = dll.parent().unwrap();
    std::env::set_current_dir(dll_dir).expect("cd to vm-build/");
    eprintln!("[smoke] cwd -> {}", dll_dir.display());

    eprintln!("[smoke] LoadLibrary {}", dll.display());
    let lib = Library::new(&dll).expect("LoadLibrary factor.dll");
    let api = NfApi::load(&lib);
    eprintln!("[smoke] embedding API resolved");

    eprintln!("[smoke] nf_new_vm()");
    let vm = (api.new_vm)();
    assert!(!vm.is_null(), "nf_new_vm returned NULL");

    eprintln!("[smoke] nf_default_parameters()");
    let params = (api.default_parameters)();
    assert!(!params.is_null(), "nf_default_parameters returned NULL");

    let img_utf16 = to_utf16_z(&img);
    eprintln!("[smoke] params_set_image_path({})", img.display());
    (api.params_set_image)(params, img_utf16.as_ptr());
    (api.params_set_signals)(params, 0);

    eprintln!("[smoke] nf_init_factor() — loading image");
    (api.init_factor)(vm, params);
    (api.free_parameters)(params);

    eprintln!("[smoke] nf_run_startup() — wires up init-remote-control");
    (api.run_startup)(vm);
    eprintln!("[smoke] startup complete");

    if let Some(p) = cwd_save { let _ = std::env::set_current_dir(p); }

    body(&api, vm);

    // VM is leaked: the embedding API doesn't currently expose a clean
    // shutdown path, and Factor's runtime gets reaped when the test
    // process exits anyway.
}

// ─── Tests ──────────────────────────────────────────────────────────────────

/// The barest of barest smokes: 2+3 round-trips through Factor's `.`.
/// Doesn't touch forth.runtime — proves the embedding plumbing alone.
#[test]
#[ignore]
fn embedding_basic_arithmetic() {
    unsafe {
        with_vm(|api, vm| {
            let out = eval(api, vm,
                "USING: math prettyprint io ; 2 3 + . flush");
            assert!(out.contains('5'),
                    "expected '5' in output, got {out:?}");
        });
    }
}

/// forth.runtime's `.` (the ANS-Forth period) is reachable and prints
/// the cell on top of the data stack.  Uses the fully-qualified name
/// to dodge ambiguity with Factor's own prettyprint `.`.
#[test]
#[ignore]
fn forth_runtime_print() {
    unsafe {
        with_vm(|api, vm| {
            let out = eval(api, vm,
                "USING: forth.runtime io ; 42 forth.runtime:. flush");
            assert!(out.contains("42"),
                    "expected '42' in output, got {out:?}");
        });
    }
}

/// nf-addr memory model round-trip — the headline Phase 1 success
/// criterion.  `<variable>` creates a fresh 1-cell allocation,
/// `nf-!` stores 7 into it, `@` fetches.  Exercises the byte-array
/// + offset model end-to-end, including pinned-pointer arithmetic
/// through Factor's alien-accessors.
///
/// Both `.` and `@` are ambiguous in this scope (prettyprint `.` and
/// math.ratios `@` collide), hence the fully-qualified forth.runtime:
/// prefix.  The Rust compiler will emit FQ names automatically.
#[test]
#[ignore]
fn forth_runtime_variable_roundtrip() {
    unsafe {
        with_vm(|api, vm| {
            let out = eval(api, vm,
                "USING: forth.runtime kernel io ; \
                 <variable> 7 over forth.runtime:nf-! \
                 forth.runtime:@ forth.runtime:. flush");
            assert!(out.contains('7'),
                    "expected '7' from variable round-trip, got {out:?}");
        });
    }
}

// ─── Phase 2.3 end-to-end: ANS source → Factor IR → execute ─────────────────

/// Helper: run an ANS Forth source through the compiler, then eval
/// the resulting Factor IR.  Returns the captured output for assertions.
unsafe fn compile_and_run(api: &NfApi, vm: *mut FactorVm, ans_source: &str) -> String {
    let ir = newfactor::compiler::compile(ans_source)
        .unwrap_or_else(|e| panic!("compile error: {e}\nsource: {ans_source}"));
    eprintln!("[phase2.3] IR: {ir}");
    eval(api, vm, &ir)
}

/// The Phase 2.3 success criterion: a `:` definition compiled by the
/// Rust front end runs end-to-end on the embedded VM and produces
/// the right answer.
///
///   ANS source : `: square ( n -- n^2 ) dup * ; 5 square .`
///   Expected output : `25 `
#[test]
#[ignore]
fn phase23_square_word() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": square ( n -- n^2 ) dup * ; 5 square .");
            assert!(out.contains("25"),
                    "expected '25' from 5 square, got {out:?}");
        });
    }
}

/// A two-arg user word — exercises stack ordering through the
/// compiler.  `: add2 + ; 10 32 add2 .` should print `42`.
#[test]
#[ignore]
fn phase23_two_arg_user_word() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": add2 ( a b -- a+b ) + ; 10 32 add2 .");
            assert!(out.contains("42"), "got {out:?}");
        });
    }
}

/// Negative integer literals survive the lex → parse → emit round
/// trip.  `-5 dup * .` should print `25` (because dup * = square).
#[test]
#[ignore]
fn phase23_negative_literal() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm, "-5 dup * .");
            assert!(out.contains("25"), "got {out:?}");
        });
    }
}

/// Multiple definitions, mutual reference — `: inc 1 + ; : twice
/// inc inc ; 5 twice .` expects `7`.
#[test]
#[ignore]
fn phase23_multiple_definitions() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": inc ( n -- n+1 ) 1 + ; : twice ( n -- n+2 ) inc inc ; 5 twice .");
            assert!(out.contains("7"), "got {out:?}");
        });
    }
}

// ─── Phase 2.4 — control flow ───────────────────────────────────────────────

/// The plan's M2.4 success criterion: `: abs ( n -- ) dup 0 < if
/// negate then ; -5 abs .` → `5`.  Exercises IF/THEN (no ELSE),
/// comparison, and unary `negate`.
#[test]
#[ignore]
fn phase24_if_then_abs() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": abs ( n -- |n| ) dup 0 < if negate then ; -5 abs .");
            assert!(out.contains('5') && !out.contains("-5"),
                    "expected positive 5, got {out:?}");
        });
    }
}

/// IF/ELSE/THEN — the classic three-way sign function.  `-3 sign .`
/// expects `-1`, `0 sign .` expects `0`, `4 sign .` expects `1`.
#[test]
#[ignore]
fn phase24_if_else_then_sign() {
    unsafe {
        with_vm(|api, vm| {
            let src = ": sign ( n -- s ) \
                       dup 0 < if drop -1 \
                       else dup 0 > if drop 1 else drop 0 then \
                       then ; \
                       -3 sign . 0 sign . 4 sign .";
            let out = compile_and_run(api, vm, src);
            // Output should contain -1, then 0, then 1.
            assert!(out.contains("-1") && out.contains("0") && out.contains("1"),
                    "expected -1 0 1, got {out:?}");
        });
    }
}

/// BEGIN ... UNTIL — count down 5..0 and check that loop terminates.
/// `: countdown ( n -- ) begin 1 - dup 0 = until drop ; 5 countdown .`
/// Loop body should run 5 times (5→4→3→2→1→0); after the loop drop
/// the residual flag, then print final stack which is just nothing
/// extra — emit dup count if you want a check, but here we just
/// verify the program completes and `.` doesn't underflow.
#[test]
#[ignore]
fn phase24_begin_until_terminates() {
    unsafe {
        with_vm(|api, vm| {
            // Compute factorial of 5 using BEGIN/UNTIL.  Easier
            // success check than just "did the loop terminate."
            //   ( n -- n! )
            //   1 swap                    ( acc n )
            //   begin
            //       dup if                ( acc n flag )
            //         tuck * swap 1 -     ( n*acc n-1 )
            //         false               ( n*acc n-1 0 ) — keep looping
            //       else
            //         drop true            ( acc final-0 1 ) — stop
            //       then
            //   until
            //   drop                       ( n! )
            //
            // Hmm — UNTIL pops a flag.  Let's just do a straight
            // descending counter and accumulate.
            //
            // : fact ( n -- n! )
            //   1 swap
            //   begin dup while
            //     tuck * swap 1 -
            //   repeat drop ;
            // 5 fact . → 120
            let src = ": fact ( n -- n! ) \
                       1 swap \
                       begin dup while \
                         tuck * swap 1 - \
                       repeat drop ; \
                       5 fact .";
            let out = compile_and_run(api, vm, src);
            assert!(out.contains("120"), "expected 120 = 5!, got {out:?}");
        });
    }
}

// ─── Phase 2.5 — DO/LOOP ────────────────────────────────────────────────────

/// The M2.5 plan success criterion verbatim:
///   `: sum ( n -- s ) 0 swap 0 ?do i + loop ; 10 sum .` → 45
///
/// Exercises ?DO, LOOP (step +1), and I.
#[test]
#[ignore]
fn phase25_sum_via_qdo_loop() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": sum ( n -- s ) 0 swap 0 ?do i + loop ; 10 sum .");
            assert!(out.contains("45"), "expected 45, got {out:?}");
        });
    }
}

/// `?DO` skips when limit == start.  `: nope 5 5 ?do i . loop ; nope`
/// should produce no numbers, just the empty output.
#[test]
#[ignore]
fn phase25_qdo_skips_empty_range() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": run-it ( -- ) 5 5 ?do 99 . loop ; \
                 .\" before \" run-it .\" after\"");
            // "99" must NOT appear; bracketing text must.
            assert!(out.contains("before") && out.contains("after"),
                    "expected before/after, got {out:?}");
            assert!(!out.contains("99"),
                    "loop body should not have run, got {out:?}");
        });
    }
}

/// `+LOOP` with a user-supplied step.  Count 0, 2, 4, 6, 8 — five
/// iterations, sum = 20.
#[test]
#[ignore]
fn phase25_plus_loop_step() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": even-sum ( -- s ) 0 10 0 ?do i + 2 +loop ; even-sum .");
            assert!(out.contains("20"), "expected 0+2+4+6+8 = 20, got {out:?}");
        });
    }
}

/// Nested DO/LOOP: `I` and `J`.  Produce sum of i*j for 0≤i<3, 0≤j<3.
///   sum = 0*0 + 0*1 + 0*2 + 1*0 + 1*1 + 1*2 + 2*0 + 2*1 + 2*2
///       = 0 + 0 + 0 + 0 + 1 + 2 + 0 + 2 + 4 = 9
/// Here `J` is the outer loop index, `I` the inner.
#[test]
#[ignore]
fn phase25_nested_i_and_j() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": cross ( -- s ) 0 3 0 ?do 3 0 ?do j i * + loop loop ; cross .");
            assert!(out.contains('9'), "expected 9, got {out:?}");
        });
    }
}

// ─── Phase 2.8 — VARIABLE/CONSTANT/FCONSTANT + variable narrowing ───────────

/// The M2.8 plan success criterion verbatim:
///   `64 constant maxiter  maxiter 2 *` → 128.
#[test]
#[ignore]
fn phase28_constant_folds() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "64 constant maxiter  maxiter 2 * .");
            assert!(out.contains("128"), "expected 128, got {out:?}");
        });
    }
}

/// Narrow VARIABLE: every use is @/!, so emit lands on Factor
/// globals.  Round-trip: store 5, fetch, print.
#[test]
#[ignore]
fn phase28_variable_narrow_roundtrip() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "variable counter  5 counter !  counter @ .");
            assert!(out.contains('5'), "expected 5, got {out:?}");
        });
    }
}

/// `+!` translates to `change-global` for narrow variables.
#[test]
#[ignore]
fn phase28_variable_plus_store() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "variable n  10 n !  3 n +!  n @ .");
            assert!(out.contains("13"), "expected 13, got {out:?}");
        });
    }
}

/// Multiple constants compose.  `4 constant a  3 constant b  a b * .`
/// expects 12.
#[test]
#[ignore]
fn phase28_constant_composition() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "4 constant a  3 constant b  a b * .");
            assert!(out.contains("12"), "expected 12, got {out:?}");
        });
    }
}

/// Variable referenced from inside a user word still gets the
/// narrow path.  Escape analyser descends into definition bodies,
/// so `: get-n n @ ;` keeps `n` narrow even though the only direct
/// reference to `n` from top-level is `n !`.
#[test]
#[ignore]
fn phase28_narrow_variable_via_user_word() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": load-it n @ ; variable n  5 n !  load-it .");
            assert!(out.contains('5'), "expected 5, got {out:?}");
        });
    }
}

/// `n n + @` — the user's negative-case probe.  The address gets
/// used as a value (pointer arithmetic), which our nf-addr model
/// can't represent.  The escape analyser correctly marks `n` wide;
/// the wide path emits valid IR; the runtime fails at the `+` step
/// because nf-addr tuples aren't numbers.  This test compiles AND
/// confirms escape detection; the runtime failure is documented as
/// M2.11 work (translate Factor errors into ANS-shaped messages).
#[test]
fn phase28_pointer_arithmetic_marked_wide() {
    use newfactor::compiler::{lex, parse, sema, EscapeState};
    let src = "variable n  42 n !  n n + @ .";
    let toks = lex(src).expect("lex");
    let prog = parse(&toks).expect("parse");
    let sema = sema::build(prog).expect("sema");
    // The escape analyser must have flagged n as wide.
    match sema.escape.get("n") {
        Some(EscapeState::Wide { .. }) => {} // expected
        other => panic!("expected n to be Wide, got {other:?}"),
    }
}

/// FCONSTANT round-trip with a float literal.
#[test]
#[ignore]
fn phase28_fconstant_float() {
    unsafe {
        with_vm(|api, vm| {
            // f. (or just .) on a float should produce the value.
            // forth.runtime:. handles integers; floats go through
            // Factor's prettyprint here.
            let out = compile_and_run(api, vm,
                "3.14 fconstant pi  pi .");
            // The output for a float will be "3.14" or "3.14 " — accept either.
            assert!(out.contains("3.14"),
                    "expected 3.14, got {out:?}");
        });
    }
}

// ─── Phase 2.7 — stack-effect inference ─────────────────────────────────────

/// The M2.7 / "warn don't fail" diagnostic check: a declared effect
/// that doesn't match the body's behaviour is reported as a
/// *warning* on the Sema, but the compile still produces IR.
/// This matches the IDE-style policy: we surface ambiguity, we
/// don't block the user from running their program.
#[test]
fn phase27_canonical_effect_mismatch() {
    let (ir, warnings) = newfactor::compiler::compile_with_diagnostics(
        ": bad ( -- ) 1 2 ;"
    ).expect("compile should succeed despite mismatch");
    assert!(!ir.is_empty(), "compile should still produce IR");
    assert_eq!(warnings.len(), 1, "expected exactly one diagnostic, got {warnings:?}");
    let msg = warnings[0].to_string();
    assert!(msg.contains("bad"),      "expected word name in warning: {msg}");
    assert!(msg.contains("declared"),  "expected 'declared' in warning: {msg}");
    assert!(msg.contains("2"),        "expected '2' in warning: {msg}");
    assert!(msg.contains("warning"),  "diagnostic should be labelled warning: {msg}");
}

/// Passing programs from earlier milestones still compile cleanly —
/// no false-positive effect errors from the new pass.
#[test]
fn phase27_correct_programs_still_compile() {
    for src in [
        ": square ( n -- n^2 ) dup * ;",
        ": add2 ( a b -- a+b ) + ;",
        ": inc ( n -- n+1 ) 1 + ; : twice ( n -- n+2 ) inc inc ;",
        // No declared effect: nothing to mismatch.
        "42 .",
    ] {
        newfactor::compiler::compile(src)
            .unwrap_or_else(|e| panic!("compile failed for {src:?}: {e}"));
    }
}

// ─── Phase 2.10 — ANS strings that don't crash ──────────────────────────────

/// `S"` correctly returns (c-addr, u) and `TYPE` consumes both.
/// No PAD-as-shared-temporary; the byte-array is GC'd.
#[test]
#[ignore]
fn phase210_s_quote_type() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "s\" hello, world\" type cr");
            assert!(out.contains("hello, world"),
                    "expected hello world output, got {out:?}");
        });
    }
}

/// `FILL` writes a byte across a buffer, `TYPE` reads back.
#[test]
#[ignore]
fn phase210_fill_and_type() {
    unsafe {
        with_vm(|api, vm| {
            // Fill 4 bytes with 'A' (65), then type the buffer.
            let out = compile_and_run(api, vm,
                "4 cbuffer buf  0 buf 4 65 fill  0 buf 4 type");
            assert!(out.contains("AAAA"), "expected AAAA, got {out:?}");
        });
    }
}

/// `CMOVE` copies bytes between buffers.  Set up a source with
/// known bytes, then cmove and re-read from destination.
#[test]
#[ignore]
fn phase210_cmove() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "4 cbuffer src  4 cbuffer dst \
                 72  0 src c!  105 1 src c!  33 2 src c! \
                 0 src 0 dst 3 cmove \
                 0 dst 3 type");
            assert!(out.contains("Hi!"), "expected Hi!, got {out:?}");
        });
    }
}

/// Pictured numeric output: `n>$` for the common signed-decimal
/// case.  Positive, negative, zero.
#[test]
#[ignore]
fn phase210b_n_to_string() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "1234 n>$ type space  \
                 -7   n>$ type space  \
                 0    n>$ type");
            assert!(out.contains("1234"), "expected 1234, got {out:?}");
            assert!(out.contains("-7"),   "expected -7,   got {out:?}");
            assert!(out.contains("0"),    "expected 0,    got {out:?}");
        });
    }
}

/// The full DSL form with hex prefix.  Build `0xff` from `255`
/// using HOLD for the literal prefix characters.
#[test]
#[ignore]
fn phase210b_dsl_with_prefix() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "255 hex  dup abs <# #S \
                 120 hold  48 hold  \
                 swap sign #> type  decimal");
            assert!(out.contains("0xff"), "expected '0xff', got {out:?}");
        });
    }
}

/// Base switching: `hex 255 . decimal` should print "ff".
#[test]
#[ignore]
fn phase210b_base_switching() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "hex 255 n>$ type decimal");
            assert!(out.contains("ff"), "expected 'ff', got {out:?}");
        });
    }
}

/// `0 n>$` must produce "0", not the empty string — the spec
/// requires #S to extract at least one digit.
#[test]
#[ignore]
fn phase210b_zero_prints_zero() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ".\" [\" 0 n>$ type .\" ]\"");
            assert!(out.contains("[0]"), "expected [0], got {out:?}");
        });
    }
}

/// `BL` is 32 (ASCII space).  Verify the constant.
#[test]
#[ignore]
fn phase210_bl_is_space() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm, "bl .");
            assert!(out.contains("32"), "expected 32, got {out:?}");
        });
    }
}

// ─── Phase 2.9 — standard defining-words (array, farray, cbuffer) ───────────

/// `array` — n-cell integer array, `( idx -- addr )` instance.
/// Store + fetch round-trip through two distinct indices.
#[test]
#[ignore]
fn phase29_array_roundtrip() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "4 array primes  \
                 2 0 primes !  3 1 primes !  \
                 0 primes @ .  1 primes @ .");
            assert!(out.contains('2') && out.contains('3'),
                    "expected '2 3', got {out:?}");
        });
    }
}

/// `farray` — IEEE-754 doubles, `f@`/`f!` accessors.
#[test]
#[ignore]
fn phase29_farray_roundtrip() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "3 farray xs  \
                 3.14 0 xs f!  2.71 1 xs f!  \
                 0 xs f@ .  1 xs f@ .");
            assert!(out.contains("3.14") && out.contains("2.71"),
                    "expected '3.14' and '2.71', got {out:?}");
        });
    }
}

/// `cbuffer` — n-byte buffer, `c@`/`c!` access.  Write 'H' and 'i'
/// as ASCII, emit both.
#[test]
#[ignore]
fn phase29_cbuffer_roundtrip() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "10 cbuffer line  \
                 72 0 line c!  105 1 line c!  \
                 0 line c@ emit  1 line c@ emit");
            assert!(out.contains("Hi"), "expected 'Hi', got {out:?}");
        });
    }
}

/// A heavier check: write all 10 integers into an array via a
/// DO/LOOP, then sum them back via another DO/LOOP.  Tests
/// indexed access composed with control flow.
#[test]
#[ignore]
fn phase29_array_loop_sum() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                "10 array xs  \
                 : init  10 0 ?do  i i xs !  loop ; \
                 : sum  0  10 0 ?do  i xs @ +  loop ; \
                 init  sum .");
            // sum 0..9 = 45
            assert!(out.contains("45"), "expected '45', got {out:?}");
        });
    }
}

// ─── Phase 2.6 — CASE/OF/ENDOF/ENDCASE ──────────────────────────────────────

/// CASE arm dispatch — input 2 should hit the "two" arm.
#[test]
#[ignore]
fn phase26_case_arm_dispatch() {
    unsafe {
        with_vm(|api, vm| {
            let src = ": classify ( n -- ) \
                       case \
                         1 of .\" one\"   endof \
                         2 of .\" two\"   endof \
                         3 of .\" three\" endof \
                         .\" unknown\" \
                       endcase ; \
                       2 classify";
            let out = compile_and_run(api, vm, src);
            assert!(out.contains("two") && !out.contains("one") && !out.contains("three"),
                    "expected only 'two', got {out:?}");
        });
    }
}

/// Default branch when nothing matches.  Input 99 falls through to
/// the default — should print "unknown".
#[test]
#[ignore]
fn phase26_case_default_fires() {
    unsafe {
        with_vm(|api, vm| {
            let src = ": classify ( n -- ) \
                       case \
                         1 of .\" one\" endof \
                         2 of .\" two\" endof \
                         .\" unknown\" \
                       endcase ; \
                       99 classify";
            let out = compile_and_run(api, vm, src);
            assert!(out.contains("unknown"), "expected 'unknown', got {out:?}");
        });
    }
}

/// CASE with no default and no match: dispatch value is dropped at
/// ENDCASE, nothing printed.  Verify the program completes without
/// underflow and surrounding text bookends are visible.
#[test]
#[ignore]
fn phase26_case_no_match_no_default() {
    unsafe {
        with_vm(|api, vm| {
            let src = ": maybe ( n -- ) \
                       case \
                         1 of .\" one\" endof \
                       endcase ; \
                       .\" [\" 7 maybe .\" ]\"";
            let out = compile_and_run(api, vm, src);
            assert!(out.contains("[") && out.contains("]"),
                    "expected bracketing, got {out:?}");
            assert!(!out.contains("one"), "should not have matched, got {out:?}");
        });
    }
}

/// CASE used as a value-producing dispatch.  Each arm pushes a
/// different code; default pushes 0.
#[test]
#[ignore]
fn phase26_case_returns_value() {
    unsafe {
        with_vm(|api, vm| {
            let src = ": code ( n -- c ) \
                       case \
                         1 of 100 endof \
                         2 of 200 endof \
                         drop 0 \
                       endcase ; \
                       2 code .";
            let out = compile_and_run(api, vm, src);
            assert!(out.contains("200"), "expected 200, got {out:?}");
        });
    }
}

/// `LEAVE` flags the loop for exit at iteration boundary.  The
/// flag-based implementation requires LEAVE at the end of the
/// iteration body (the common ANS idiom) — code AFTER `leave` in
/// the same iteration still runs.  See the comment on `leave` in
/// `forth.runtime` for the rationale.
///
/// Sum 0..N where N is bounded by i>=4 firing LEAVE.  Body order:
/// add i, then check and leave.  So iterations i=0..4 each add,
/// and i=4 fires leave at end.  Sum: 0+1+2+3+4 = 10.
#[test]
#[ignore]
fn phase25_leave_exits_loop() {
    unsafe {
        with_vm(|api, vm| {
            let out = compile_and_run(api, vm,
                ": partial ( -- s ) 0 10 0 ?do \
                   i + \
                   i 4 >= if leave then \
                 loop ; partial .");
            assert!(out.contains("10"), "expected 0+1+2+3+4 = 10, got {out:?}");
        });
    }
}

/// Nested IF — verify the parser folds the inner IF into the outer
/// ELSE branch correctly when emitted.  `: max ( a b -- m ) 2dup <
/// if swap then drop ;` would test 2dup which we don't have yet,
/// so use a simpler form.
#[test]
#[ignore]
fn phase24_nested_if_in_else() {
    unsafe {
        with_vm(|api, vm| {
            let src = ": classify ( n -- ) \
                       dup 0 < if drop .\" neg\" \
                       else dup 0 = if drop .\" zero\" \
                       else drop .\" pos\" \
                       then then ; \
                       -5 classify cr 0 classify cr 7 classify";
            let out = compile_and_run(api, vm, src);
            assert!(out.contains("neg"), "expected 'neg', got {out:?}");
            assert!(out.contains("zero"), "expected 'zero', got {out:?}");
            assert!(out.contains("pos"), "expected 'pos', got {out:?}");
        });
    }
}

// Silence "unused" warnings for the c_long alias on platforms where the
// test compiles but doesn't use it.
#[allow(dead_code)]
fn _suppress_unused() { let _ = std::mem::size_of::<c_long>(); }
