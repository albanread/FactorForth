# Inside the API: Embedding the Factor VM

Forth is traditionally a self-hosted, standalone environment. It is rare to see a modern Forth dialect deeply integrated into a host language as an embedded worker. One of the major technical milestones of the NewFactor project is its invisible, tightly-coupled embedding of the Factor JIT compiler within a Rust host.

This document details the mechanics of how the Factor VM was successfully embedded into the NewFactor runtime.

## 1. Exposing the VM

Stock Factor is distributed as a standalone runtime executable (`factor.exe`/`factor.com`) and a library, but the out-of-the-box system hasn't been optimized for in-process C-ABI embedding out of the box in quite some time. 

The breakthrough for NewFactor was discovering that the necessary hooks still existed inside the VM's core C++ codebase. The patch to make Factor natively embeddable was surprisingly small: approximately 25 lines of C++ added to `vm/factor.cpp`. 

This patch exposed a half-dozen wrapper functions (`nf_init_factor`, `nf_eval_string`, `nf_run_startup`) over the existing `factor_vm` C++ class. Thus, NewFactor didn't have to invent a foreign interface; it simply exported the one that the VM's original architect had already laid the groundwork for.

## 2. Bootstrapping the Callbacks

Simply dynamically loading `factor.dll` and calling `nf_eval_string` immediately results in a segfault. The internal C++ evaluator is essentially a hollow shell until it's wired into the language's high-level environment.

In Factor, embedding hinges around Special Objects. The bootstrap sequence looks like this:
1. The Rust host calls `nf_init_factor`.
2. The VM loads `factor.image` (a memory snapshot).
3. The Rust host calls `nf_run_startup`. 

This startup quotation evaluates the key Factor phrase: `init-remote-control`. In the `alien.remote-control` vocabulary, Factor uses its own foreign function interface (`alien-callback`) to compile a high-level Factor evaluator quotation (`[ eval>string utf8 malloc-string ]`) into a raw C function pointer. It registers this pointer into the VM's `OBJ_EVAL_CALLBACK` slot. 

When Rust subsequently calls `nf_eval_string`, the C++ VM retrieves that raw pointer, fires the callback, and safely jumps from the host OS thread directly into the JIT's optimized evaluation path.

## 3. The Thread-Local Restriction

Factor is designed around an incredibly fast VM, but it fundamentally assumes it owns the context. The VM stores critical runtime state inside Thread-Local Storage (TLS)—specifically tracked to the OS thread that originally invoked `nf_init_factor`. 

If a different thread invokes the evaluator, the process immediately crashes. 

To bridge this to a modern concurrent language like Rust, NewFactor maintains a strict Session architecture:
*   The `Session` struct owns the embedded VM.
*   In the NewFactor IDE, all Factor evaluations are cleanly dispatched to a dedicated background worker thread, ensuring the TLS constraint is never broken while leaving the GUI fluid.
*   During testing (`cargo test`), Rust's parallel test instances use a global `OnceLock<Mutex<()>>` to enforce strict serialization across thread boundaries, ensuring isolated, safe VM instances for every test.

## 4. The Clean Split

By exporting an evaluation boundary via `char* -> char*` over the C ABI, NewFactor achieves complete component isolation. 

The Rust frontend acts as the true intelligence—lexing, parsing, inferring stack effects, and cross-compiling Forth code into Canonical Factor IR. Factor never sees the user's source code; it merely receives, optimizes, and executes heavily verified AST streams through the narrow `nf_eval_string` pipeline.