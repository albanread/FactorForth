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
//! KNOWN ISSUE: cargo's test runner spawns each test in a fresh OS
//! thread (for panic isolation), and Factor's VM stores critical
//! state in TLS that gets initialised in nf_init_factor's calling
//! thread.  The eval primitive expects to run on that same thread.
//! From within a cargo-test worker thread the embedding plumbing
//! works up to init but then crashes on the first eval.  The
//! `cargo run --bin embed-smoke` binary in src/bin/ does the same
//! round-trip from main and is the gate that actually verifies the
//! Phase 1 success criterion.  Real fix: keep all VM access on a
//! dedicated session thread (Phase 3 work, when the session module
//! lands).

#![cfg(target_os = "windows")]

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_long, CStr, CString};
use std::os::raw::c_void;
use std::path::{Path, PathBuf};

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
unsafe fn with_vm<F: FnOnce(&NfApi, *mut FactorVm)>(body: F) {
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

// Silence "unused" warnings for the c_long alias on platforms where the
// test compiles but doesn't use it.
#[allow(dead_code)]
fn _suppress_unused() { let _ = std::mem::size_of::<c_long>(); }
