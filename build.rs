//! build.rs — export FFI symbols + embed the Windows manifest.
//!
//! ## FFI exports
//!
//! Factor calls into the host via `alien.libraries` + `FUNCTION:`
//! declarations.  On Windows that resolves to `GetProcAddress` on
//! whichever module the host binary became.  But `#[no_mangle]
//! pub extern "C"` in a Rust *binary* (.exe) doesn't auto-export
//! the way it does in a `cdylib` — the symbols are present in
//! the executable but not in its export table, so GetProcAddress
//! can't find them.
//!
//! Fix: emit `/EXPORT:<name>` linker args for each function we
//! want Factor to be able to call back into.  Applies to all
//! binary outputs (bins + tests + examples), which matters
//! because tests also use the Session and therefore need the
//! same callbacks reachable.
//!
//! Only needed on Windows (MSVC linker syntax).  On other
//! platforms the binary's symbol table is reachable directly.
//!
//! ## Manifest embed
//!
//! `tools/factorforth-ui.exe.manifest` gets baked into the
//! factorforth-ui binary so the IDE gets:
//!   - Per-monitor-v2 DPI awareness (crisp Direct2D on hi-DPI)
//!   - Common Controls v6 visual styles
//!   - UTF-8 active code page
//!   - supportedOS GUIDs through Windows 11
//!
//! `embed-resource` handles invoking rc.exe (MSVC) and stitching
//! the .res into the linker line.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=tools/factorforth-ui.rc");
    println!("cargo:rerun-if-changed=tools/factorforth-ui.exe.manifest");
    println!("cargo:rerun-if-changed=tools/factorforth.ico");
    let target = std::env::var("TARGET").unwrap_or_default();
    let is_windows = target.contains("pc-windows") || target.ends_with("-msvc");
    if !is_windows { return; }

    // Embed the IDE's manifest.  embed-resource attaches the .res
    // file to every binary in this crate, but that's harmless —
    // only the IDE's window-creating code actually uses the DPI
    // awareness etc.
    embed_resource::compile("tools/factorforth-ui.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap();

    // Names must match the `#[no_mangle] pub extern "C" fn ...`
    // declarations in src/session.rs.  Keep these in sync.
    let exports = [
        // Names are nf_rt_* (not bare rt_*) because WF64's lib —
        // which we depend on via path for its iGui module — defines
        // rt_read_char / rt_write_char / rt_read_line with different
        // signatures.  Renaming our exports avoids the linker
        // multi-definition error when building the GUI binary.
        "nf_rt_read_char",
        "nf_rt_write_char",
        "nf_rt_read_line",
        // Listener loop FFI - Factor's nf-listener-loop blocks on
        // these instead of nf_eval_string flow-of-control.  Lets
        // us mirror Factor's stock listener architecture, with
        // data-stack values threaded through each iteration.
        "nf_rt_next_command",
        "nf_rt_command_done",
        // Stack-snapshot FFI - Factor calls these after every
        // eval to ship the current datastack to the IDE's stack
        // pane.  Without them in the export table the listener
        // would crash on its first publish call (this is the
        // same gotcha that bit nf_rt_next_command before).
        "nf_rt_stack_begin",
        "nf_rt_stack_item",
        "nf_rt_stack_end",
        // Float-FFI proof-of-life: prove the Win64 ABI delivers
        // doubles through XMM0 both ways without precision loss.
        // Foundation for real-time graphics (frame dt, vertex
        // streams, audio sample rate).
        "rt_check_double",
        "rt_emit_double",
        // M2.x #32 ANS File Access: INCLUDED calls this from
        // Factor to read + compile an ANS source file.
        "rt_compile_ans",
        // NOTE: the rt_gpane_* graphics FFI used to be exported here,
        // but those functions were *defined* in wf64::runtime.  When
        // we cut the wf64 crate dependency (we use Factor, not WF64's
        // MASM/JASM/NewGC engine) and forked only the igui window
        // shell into crates/igui, those definitions went with WF64.
        // The Factor-callable graphics API (forth.wf64-gfx) is
        // therefore unwired for now; it will be reintroduced — owned
        // by us this time — as part of the CoreProtocols GUI layer
        // (Phase 4), which is where a canvas/drawing FFI belongs.
        // See docs/design/core-protocols.md §5.
    ];
    // Apply only to bins/tests that actually link session.rs.
    // The legacy `embed-smoke` binary doesn't (it does its own
    // libloading directly) and would fail linking if we asked
    // it to export non-existent symbols.
    // Only bins that actually link `newfactor::session` belong
    // here.  newfactor-ui is a Phase-0 placeholder today; it'll
    // join the list when 3.4 wires the GUI through Session.
    // Every binary that creates a Session needs these symbols
    // exported.  GetProcAddress(GetModuleHandle(NULL), ...) is how
    // Factor's FFI finds them — they must be in the exe's export
    // table, not just present in the .text section.  The IDE
    // binary (factorforth-ui) is THE primary user of Session, so
    // forgetting to add it here is what caused #54's "Cannot
    // resolve C library function" hang.
    let bins_with_session = ["newfactor", "factorforth-ui"];
    for name in exports {
        for bin in bins_with_session {
            println!("cargo:rustc-link-arg-bin={bin}=/EXPORT:{name}");
        }
        // All test binaries link the lib (which has session.rs),
        // so they all get the exports.
        println!("cargo:rustc-link-arg-tests=/EXPORT:{name}");
    }
}
