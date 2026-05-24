//! src/bin/embed_smoke.rs — Phase 1 smoke that runs in main thread.
//!
//! cargo test spawns each test in its own thread (for panic isolation).
//! Factor's VM stores critical state in TLS and is initialised in the
//! caller's thread; calling nf_eval_string from a different / nested
//! thread can crash.  We use a separate binary so the smoke runs in
//! `main` directly, mirroring the C smoke test exactly.
//!
//! Usage:
//!   cargo run --bin embed-smoke                    -- defaults to mandelbrot image
//!   cargo run --bin embed-smoke -- nf-slim-v1.image -- alt image
//!   cargo run --bin embed-smoke -- nf-mandelbrot.image "USE: forth.runtime 42 forth.runtime:. flush"

#![cfg(target_os = "windows")]

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, CStr, CString};
use std::os::raw::c_void;
use std::path::PathBuf;

type FactorVm     = c_void;
type VmParameters = c_void;
type VmChar       = u16;

fn main() {
    let mut args = std::env::args().skip(1);
    let image_name = args.next().unwrap_or_else(|| "nf-mandelbrot.image".into());
    let expr = args.next().unwrap_or_else(|| "2 3 + . flush".into());

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dll = PathBuf::from(manifest_dir).join("vm-build").join("factor.dll");
    let img = PathBuf::from(manifest_dir).join("images").join(&image_name);

    assert!(dll.exists(), "factor.dll not at {}", dll.display());
    assert!(img.exists(), "image not at {} — run scripts/build-image.sh", img.display());

    std::env::set_current_dir(dll.parent().unwrap()).unwrap();

    let lib = unsafe { Library::new(&dll) }.expect("LoadLibrary");

    unsafe {
        let nf_new_vm: Symbol<unsafe extern "C" fn() -> *mut FactorVm> =
            lib.get(b"nf_new_vm\0").unwrap();
        let nf_default_parameters: Symbol<unsafe extern "C" fn() -> *mut VmParameters> =
            lib.get(b"nf_default_parameters\0").unwrap();
        let nf_free_parameters: Symbol<unsafe extern "C" fn(*mut VmParameters)> =
            lib.get(b"nf_free_parameters\0").unwrap();
        let nf_params_set_image_path: Symbol<unsafe extern "C" fn(*mut VmParameters, *const VmChar)> =
            lib.get(b"nf_params_set_image_path\0").unwrap();
        let nf_params_set_signals: Symbol<unsafe extern "C" fn(*mut VmParameters, c_int)> =
            lib.get(b"nf_params_set_signals\0").unwrap();
        let nf_init_factor: Symbol<unsafe extern "C" fn(*mut FactorVm, *mut VmParameters)> =
            lib.get(b"nf_init_factor\0").unwrap();
        // nf_run_startup runs the image's startup quotation
        // (init-remote-control) which wires up the eval callback that
        // nf_eval_string depends on.  Without this, eval_string crashes.
        let nf_run_startup: Symbol<unsafe extern "C" fn(*mut FactorVm)> =
            lib.get(b"nf_run_startup\0").unwrap();
        let nf_eval_string: Symbol<unsafe extern "C" fn(*mut FactorVm, *mut c_char) -> *mut c_char> =
            lib.get(b"nf_eval_string\0").unwrap();
        let nf_eval_free: Symbol<unsafe extern "C" fn(*mut FactorVm, *mut c_char)> =
            lib.get(b"nf_eval_free\0").unwrap();

        println!("[smoke] vm");
        let vm = nf_new_vm();
        assert!(!vm.is_null());

        let params = nf_default_parameters();
        assert!(!params.is_null());

        use std::os::windows::ffi::OsStrExt;
        let mut img_w: Vec<u16> = img.as_os_str().encode_wide().collect();
        img_w.push(0);
        nf_params_set_image_path(params, img_w.as_ptr());
        nf_params_set_signals(params, 0);

        println!("[smoke] init_factor against {}", img.display());
        nf_init_factor(vm, params);
        nf_free_parameters(params);

        println!("[smoke] run_startup (init-remote-control etc.)");
        nf_run_startup(vm);

        println!("[smoke] eval: {expr}");
        let c = CString::new(expr).unwrap();
        let raw = nf_eval_string(vm, c.as_ptr() as *mut c_char);
        if raw.is_null() {
            eprintln!("[smoke] eval returned NULL");
            std::process::exit(1);
        }
        let out = CStr::from_ptr(raw).to_string_lossy().into_owned();
        println!("[smoke] output: {out:?}");
        nf_eval_free(vm, raw);
    }
}
