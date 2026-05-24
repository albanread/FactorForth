# NewFactor Manifesto

> **Mission restatement (2026-05-23).** Two earlier passes drifted toward the wrong implementation. This block is normative; if anything below it reads otherwise, this block wins.
>
> 1. **We want Factor's VM. We do not want Factor.**
>    Factor's VM is — uniquely, as far as we know — a JIT that takes a concatenative stack-based front end and lowers it through a register-allocated CFG, including PICs and a generational GC. That engine is what we are reusing. Factor the *language* (its parser, its listener, its `ui.tools`, its image-bundled standard library, its `command-line-startup` → `quit` lifecycle) is **not** the target. It is the upstream noise we strip away.
>
> 2. **The user writes ANS Forth. The user never sees Factor.**
>    (Revised 2026-05-24, replacing the v1 "Rust emits VM-level objects directly" wording.  The original intent — that the *language* never drifts from ANS Forth into ANS-flavoured Factor — stands.  The implementation mechanism has been clarified.)
>
>    Not in source: user writes ANS Forth files.  Not in error messages: parse errors, stack-effect errors, and runtime errors are translated back to ANS terms before being shown.  Not in the dictionary: word names resolve in an ANS-conventional case-folded namespace.  Not in tooling: any REPL, editor integration, or debugger speaks ANS Forth, not Factor.
>
>    Internally, the Rust compiler does the entire ANS Forth front end — tokenisation, BASE-aware number parsing, ANS string and comment syntax, parsing words (`:` `;` `IF` `ELSE` `THEN` `BEGIN` `UNTIL` `WHILE` `REPEAT` `DO` `LOOP` `+LOOP` `LEAVE` `I` `J` `CREATE` `DOES>` `VARIABLE` `CONSTANT` `."` `S"` `[CHAR]` `[']`), stack-effect inference, the dictionary, error reporting — and emits **canonical, machine-generated Factor source** as its intermediate representation.  Factor's optimising compiler and JIT then execute it.
>
>    This IR is *an implementation detail*, swappable for direct VM-cell emission as a future optimisation, transparent to the user.  The choice of "Factor source as IR" over "Factor VM cell arrays as IR" is pragmatic: Factor's parser is fast, correct, and well-optimised — re-implementing it in Rust just to "bypass" it would buy nothing real.  What we explicitly will not do is expose Factor's surface to the user.  *Factor-as-target is the architecture; Factor-as-language is the trap.*
>
> 3. **Factor's primitives already cover ~half of ANS Forth.**
>    The VM exports `primitive_*` symbols for stack moves, integer arithmetic, comparisons, byte-array and string access, the return stack, control transfer, fixnum/bignum/float math. ANS core words are largely a thin renaming layer over those — we wire `DUP` to Factor's `dup` primitive (or its inlined equivalent), `+` to the right specialised arithmetic, etc. We **invent no semantics that the VM already provides**.
>
> 4. **We build the minimal VM ourselves.**
>    The redistributed `factor.dll` + 128 MB `factor.image` is not the artefact we ship. It is Factor's full development environment frozen into a binary. We compile a stripped-down VM from `E:\factor-src\` (C++ or Zig flavours both available), with an explicit C embedding API (`vm_create`, `vm_load_image`, `vm_call_quot`, `vm_destroy`), and we feed it a minimal image containing only the kernel + the runtime words our Rust compiler emits against. Target image size: single-digit MB, not 128 MB.
>
> 5. **Tight integration means in-process FFI, not pipes.**
>    The VM is linked (or `LoadLibrary`'d) into our Rust host. We call its embedding API directly. There is no subprocess; there is no listener; there is no stdin/stdout sentinel protocol. Pipes were a debugging crutch attempting to make the unmodified `factor.com` usable — that path is closed.
>
> ### Approaches explicitly ruled out
>
> - Spawning `factor.com` / `factor.exe` and piping a sentinel protocol over stdin/stdout. (Image size unchanged, IDE-style listener stays in the loop, lifecycle owned by Factor's `command-line-startup` → `quit`.)
> - Loading the stock `factor.dll` in-process via `LoadLibrary` + IAT patching `GetModuleHandle`. (Even when made to work, lands you inside Factor's `start_standalone_factor` → `command-line-startup` machinery, dragging the whole standard image along.)
> - Transpiling ANS Forth source into *user-visible* Factor source text. (Couples us to the surface syntax of a language we are explicitly not adopting; means the programmer is writing Factor-flavoured Forth, not ANS Forth.)  This rules out hand-written rewriters that surface in error messages, tooling, or the dictionary.  It does NOT rule out machine-generated Factor source used purely as an internal IR — see point 2 above for the distinction.
>
> ### What "tightly integrated" means in practice
>
> Rust owns the parser, the compile-time stack-effect analysis, the dictionary, the error model, and the bridge to the VM. The VM owns the quotation/word/code-heap representation, the JIT, the GC, the runtime stack discipline, and the FFI primitives. The seam between them is the embedding C API the VM exposes (`nf_init_factor`, `nf_eval_string`, `nf_call_quotation`, etc.) — defined and added to the VM source by us; small enough to fit on one screen.
>
> Above that seam, between Rust and the VM, sits **one small Factor-side vocab** called `forth.runtime`.  It contains the runtime-callable words that ANS Forth needs but Factor doesn't directly express: the cell-addressed memory model (built on Factor byte-arrays), the return stack (built on the special-object slot trick from `forth.fstack`), the ANS boolean convention (`-1`/`0` ↔ `t`/`f`), the small handful of I/O words that differ semantically from Factor's, and any primitive-renaming aliases the Rust compiler can't handle by direct lookup.  Rough budget: 200–400 lines of Factor, one vocab, not the eleven we accumulated speculatively.  Everything else in ANS Forth — the parsing words, the control flow, the defining words, the dictionary, the error reporting — is Rust's job.
>
> This is not a vibe-coding task. It will take weeks to do correctly. Stages below are sequenced to surface fatal misunderstandings *early*.

---

## What we are doing

Factor is a concatenative language with a mature optimising compiler (20+ CFG passes, linear-scan register allocation, polymorphic inline caches) and a generational GC (nursery/aging/tenured, precise roots, card-marking write barriers). Both subsystems are essentially language-agnostic. The goal is to exploit them for Forth — the simplest useful concatenative language — and then push beyond Forth toward a faster, more dynamic successor.

The three work streams are:

1. **Rust ANS Forth → Factor VM** — a Rust crate that parses ANS Forth and emits Factor-VM-level objects (quotations, words, primitive calls, code-heap entries) directly through the VM's embedding API.
2. **Minimal VM build** — compile a slim VM from `E:\factor-src\`, add a clean embedding API, drop the standard library / listener / UI / parser; produce a small image containing only what our Rust compiler emits against.
3. **Dynamic language research** — once ANS Forth runs correctly on the slim VM, explore what Factor's inline-cache and type-inference machinery can do for a Forth dialect with first-class words, dynamic dispatch, and optional gradual types.

---

## What we are not doing

- We are not writing a new GC. Factor's generational collector is correct, precise, and production-proven. We reuse it.
- We are not writing a new optimising compiler. Factor's CFG pipeline (tree IR → CFG → linear scan → codegen) already handles stack languages. We feed it.
- We are not preserving Factor's OpenGL listener. The UI is architecturally separate (backend HOOKs in `basis/ui/backend/backend.factor`). We do not load it.
- We are not targeting ANS conformance for its own sake. ANS is the baseline. The interesting work begins where ANS ends.

---

## Why Factor's compiler is the right foundation

Both Factor and Forth are postfix, stack-based, and word-centric. The structural correspondences are direct:

| Forth concept | Factor equivalent |
|---|---|
| Word definition (`:`/`;`) | `GENERIC:` / `M:` / `: foo ... ;` |
| Quotation / XT | Factor quotation `[ ... ]` |
| Stack effect | Inferred stack effect `( a b -- c )` |
| Compile-time words (`IMMEDIATE`) | Parsing words |
| `DOES>` / `CREATE` | `MACRO:` / parsing-word protocol |
| Dictionary | Factor vocabulary system |
| Threaded code | Compiled quotations / inline caches |

Factor's tree IR nodes (`#push`, `#call`, `#shuffle`) map directly onto Forth's data stack operations. The CFG optimizer (copy propagation, DCE, inlining) produces good code for tight Forth inner loops with no changes. The inline cache system handles Forth's late-binding word references (forward references, redefinition) at near-zero overhead once warmed up.

---

## The sequence

### Stage 1 — Download all Factor VM documentation

Source code shows what the VM *does*; documentation shows what it is *supposed to do*, including invariants the source assumes but does not enforce.  Read the docs first.

Collect locally (everything goes under `E:\NewFactor\docs\upstream\`):

- **Repository markdown** — `README.md`, `CONTRIBUTING.md`, anything `.md` or `.txt` under `E:\factor-src\` (vm, basis, core, extra).  Many vocabs ship a `summary.txt`.
- **In-source comments** — `vm/*.hpp` and `vm/*.cpp` contain extensive design notes in block comments; extract these into per-subsystem digest files (`vm-gc.md`, `vm-codegen.md`, `vm-image.md`, …).
- **Factor's help articles** — Factor's structured docs live as `ARTICLE:` and `HELP:` definitions in `.factor` files scattered through `basis/help/`, `core/`, `basis/` and `extra/`.  These are *the* canonical handbook (`"vm" help`, `"compiler" help`, `"objects" help`, `"images" help`, `"alien" help`, `"alien.remote-control" help` are entry points).  Extract verbatim into a flat `articles/` tree keyed by article name.
- **Comments on key primitives** — `vm/primitives.hpp` and its companions identify the C-ABI surface; copy these into `primitives-reference.md` and annotate which Forth words each one will back.

Collect externally (web fetch into `docs/upstream/external/`):

- **Slava Pestov's PhD thesis** — *"Factor: A Dynamic Stack-Based Programming Language"* (2010, Pestov / Ehrenberg / Groff is the published JFP version).  Authoritative source on tree-IR → CFG passes, inline caches, and the GC design.
- **The online Factor handbook** at `https://docs.factorcode.org/` — same articles as in-source but rendered, easier to read linearly.
- **factorcode.org** wiki / blog posts that survive — many design notes by Pestov on inline caching, generational collection, and PIC machinery were posted on his blog before being folded into the handbook.

Deliverable: `E:\NewFactor\docs\upstream\INDEX.md` listing every artefact, its origin, and a one-line summary of what it covers, so that later stages can grep for "where did the spec say X" in seconds rather than re-reading source.

### Stage 2 — Understand the VM's runtime representation

With Stage 1 in hand, read the VM source until we can answer cold:

- What is the in-memory layout of a `quotation`? A `word`? An `array`? A `byte_array`? (See `vm/objects.hpp`, Zig mirror at `src/layouts.zig`.)
- How does the JIT discover the entry point of a quotation? How are uncompiled quotations bridged via `lazy_jit_compile`?
- Which primitives exist, and what is their calling convention from JITted code? (`vm/primitives.cpp`, exported as `primitive_*` from the DLL.)
- What does the image-file format look like? Header, special-objects array, data-heap section, code-heap section, relocations. (`vm/image.cpp`, mirror at `src/image.zig`.)
- How does GC see roots from JITted frames? (Card-marking write barriers, safepoint maps — `vm/safepoints.cpp`, `vm/code_blocks.cpp`.)

Deliverable: a short note in `docs/vm-representation.md` summarising these, with concrete struct definitions copied in. This is the contract our Rust compiler emits against.

### Stage 3 — Minimal VM build with embedding API

Build the VM from `E:\factor-src\` with the standard library *not* compiled into the image. Add a small C API surface to the VM and export it from the resulting library:

```c
// proposed minimal embedding API — to be refined as we learn the VM
factor_vm* nf_vm_create(void);
int        nf_vm_load_image(factor_vm*, const char* image_path);
cell       nf_vm_intern_word(factor_vm*, const char* name);
cell       nf_vm_make_quotation(factor_vm*, const cell* elements, size_t n);
cell       nf_vm_call_quotation(factor_vm*, cell quot);
void       nf_vm_destroy(factor_vm*);
```

(`cell` = tagged 64-bit value. Quotations are arrays of words / primitives / immediates.)

Build options to evaluate:
- **C++ VM** (`vm/*.cpp`) — closer to the published reference, but needs MSVC/MinGW.
- **Zig VM** (`src/*.zig`) — currently Linux/macOS-tuned, would need Windows porting work for pthread → CreateThread, signals → SEH; on the upside, builds with the `zig` we already have on PATH and has cleaner symbol management.

Pick one, document why in `docs/vm-build-choice.md`, then build it.

Concurrent task: produce a **boot image** containing only what we actually need:
- kernel, math, arrays, byte-arrays, strings, sequences (Factor's core protocol layer)
- io.streams.c (so that `write`/`flush`/`read-line` reach real CRT FILE*s)
- *not* `ui.*`, *not* `tools.*`, *not* `compiler.tree` test suite, *not* anything self-bootstrapping.

This is the gate: if we cannot produce a single-digit-MB image that hosts our compiler's output, the project does not proceed.

### Stage 4 — Rust ANS Forth compiler emitting VM objects

A Rust crate `forthc-factor` that:

1. Parses ANS Forth source (`:` ... `;`, `IF`/`ELSE`/`THEN`, `BEGIN`/`UNTIL`, numbers, strings, character literals, comments).
2. Resolves words against:
   - **Built-in primitives** — direct calls to Factor `primitive_*` (`DUP` → `primitive_dup`-equivalent inlined; `+` → `primitive_fixnum_add` or its inline form; `@` → cell-load primitive; etc.).
   - **Runtime kernel words** — references into the slim image's word table.
   - **User dictionary** — words our compiler has emitted earlier in this session.
3. Performs Forth-side static checking (matched balanced control structures, basic stack-effect inference where possible).
4. Emits Factor *quotation objects* by calling the VM's embedding API: `nf_vm_make_quotation` over a vector of tagged word/primitive references and immediate values.
5. Hands the resulting quotation to the VM via `nf_vm_call_quotation`.

The crate is library-shaped: a separate `nf` REPL host crate consumes it for interactive use, and `wf64`'s `newfactor-ui` will eventually wire its eval pane through it.

### Stage 5 — ANS test suite, run from Rust

Embed the Hayes / Gforth ANS Forth test harness as Rust integration tests. Each test:

- spins up an `nf_vm`,
- compiles a Forth source fragment,
- calls it,
- inspects the resulting data stack (read out via an embedding-API accessor like `nf_vm_pop` or by emitting a `.s`-equivalent quotation that pushes the stack snapshot somewhere we can read).

All ANS Required words must pass before Stage 6 begins. Failures are compiler bugs, not "deviations".

### Stage 6 — Dynamic Forth extensions

Once the ANS baseline is correct, explore what Factor's machinery enables beyond standard Forth:

**6a. Typed words and specialisation**

Factor's `GENERIC:` system allows multiple definitions of a word dispatched on argument type. A Forth word `+` can be specialised for fixnums, floats, bignums, and vectors without any runtime overhead on the common (fixnum) path — exactly what Factor already does. Adopt this model.

**6b. Inline caches for redefinition**

Standard Forth redefinition requires walking the dictionary. With inline caches (as in `vm/inline_cache.cpp`), a redefined word invalidates cached call sites and re-specialises on next execution. This gives Forth interactive redefinition at near-native speed.

**6c. Gradual stack effects**

Factor's stack checker rejects words with unknown effects at compile time. Relax this to a gradual model: words may declare partial effects; unchecked words fall back to interpreted execution. This preserves ANS `CATCH`/`THROW` and `EXECUTE` semantics while still specialising the hot path.

**6d. First-class continuations / coroutines**

Factor has `callcc0`, `callcc1`, and a context system (`basis/concurrency/`). Map these to Forth coroutines and co-routines — useful for embedded DSLs, generators, cooperative multitasking.

### Stage 7 — Go-style concurrency primitives

Factor's context system supports multiple execution contexts sharing a heap, with the GC scanning all stacks. This is the substrate needed for goroutine-style lightweight threads:

- Each Forth "task" is a Factor context with its own data/return stacks
- The GC already scans all live contexts for roots
- Channel-style synchronisation can be built as Factor vocabularies over the existing `basis/concurrency/mailbox` or `basis/channels` abstractions

The target: Forth with `TASK`, `CHANNEL`, `SEND`, `RECEIVE` — ANS-compatible in single-threaded mode, concurrent when tasks are active.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Rust host (nf REPL, newfactor-ui, integration      │
│  test harness) — owns dictionary, parser, prompt.   │
└────────────────────┬────────────────────────────────┘
                     │  Rust crate API
┌────────────────────▼────────────────────────────────┐
│  forthc-factor (Rust crate)                         │
│    parse ANS Forth source                           │
│    resolve words → primitive ids / kernel words /   │
│      previously-emitted user words                  │
│    static stack-effect checks                       │
│    build Factor quotation objects                   │
└────────────────────┬────────────────────────────────┘
                     │  nf_vm_make_quotation,
                     │  nf_vm_call_quotation
                     │  (C ABI, in-process FFI)
┌────────────────────▼────────────────────────────────┐
│  nf-factor-vm  (slim VM, built from E:\factor-src\) │
│    Tree IR → CFG → linear scan → x86-64 / ARM64    │
│    Generational GC: nursery / aging / tenured       │
│    Precise roots, card-marking, safepoints          │
│    NO listener · NO ui.tools · NO standard library  │
└─────────────────────────────────────────────────────┘
```

The seam is the C ABI between the middle and bottom boxes. Nothing in the Rust crate ever touches Factor surface syntax; nothing in the slim VM ever opens a window or a console.

---

## Constraints

- The Factor VM and compiler source (`E:\factor-src\`) are read-only reference. Changes needed for headless operation are isolated to new vocabularies and a minimal boot script; we do not fork the core.
- ANS conformance is verified by the published test suite before any extension work begins.
- Performance is measured against gforth and swiftforth at `-O2` for the ANS core. The target is competitive — not beating native Forth kernels, but not embarrassing either. Factor's inliner and type specialiser should close most of the gap on common idioms.
- The GC is not tuned until Stage 3 passes. Correctness first.
- The OpenGL listener is never loaded. The UI vocabulary must not appear in any boot image used by this project.

---

## Opportunities for a faster dynamic Forth

The central insight: **ANS Forth is slow because it is untyped and interpreted; Factor's compiler is fast because it infers types and specialises at compile time. The same compiler applied to Forth words buys Factor's speed for Forth's simplicity.**

Concrete gains expected:

| Technique | Forth baseline | With Factor compiler |
|---|---|---|
| Fixnum arithmetic | 1–2 ns (STC) | ~0.3 ns (inlined, no tag check on known-fixnum path) |
| Word dispatch | Indirect call through XT | Direct call after inline cache warms |
| Array access | `@` with manual bounds | Specialised `nth` with bounds elided on proven-safe path |
| String operations | `CMOVE` loop | SIMD via Factor's `specialized-arrays` on wide types |
| Recursive words | Standard call/return | Tail-call eliminated where stack effect permits |

The dynamic Forth variant — working title **NDynaForth** — is not a separate project. It is what Stage 4 and Stage 5 produce incrementally as the ANS baseline hardens.

---

## Source layout (planned)

```
E:\NewFactor\
├── MANIFESTO.md              ← this file
├── docs/
│   ├── vm-representation.md  ← Stage 0 deliverable
│   └── vm-build-choice.md    ← Stage 1 deliverable (C++ vs Zig)
├── vm/                       ← slim VM build artefacts and patches
│   ├── patches/              ← minimal diffs against E:\factor-src\
│   ├── embedding-api.h       ← the nf_vm_* C ABI we add
│   └── boot.factor           ← script that produces the slim boot image
├── crates/
│   ├── forthc-factor/        ← Rust ANS Forth → Factor-VM-objects compiler
│   │   └── src/
│   │       ├── lex.rs        ← tokeniser
│   │       ├── parse.rs      ← word definitions, control structures
│   │       ├── resolve.rs    ← primitive table + dictionary lookup
│   │       ├── stack_eff.rs  ← static stack-effect checks
│   │       ├── emit.rs       ← build quotation arrays, call FFI
│   │       └── ffi.rs        ← bindgen for nf_vm_*
│   └── nf-vm-sys/            ← raw FFI bindings + build.rs that compiles
│                                the slim VM (cc / zig-cc / cmake)
└── images/
    └── nf-kernel.image       ← the slim boot image (target: < 8 MB)
```

Notes on the layout:

- No `.factor` files in the source tree. Forth source the user writes is parsed by Rust; nothing in this project consumes Factor syntax.
- The `vm/patches/` directory holds the *smallest possible* diff against pristine `E:\factor-src\` — embedding-API exports, image-loader tweaks, that's it. We do not fork the VM; we annotate it.
- `nf-vm-sys` is the platform abstraction. Choosing C++ vs Zig (Stage 1) decides whether its `build.rs` shells out to `cl.exe` or `zig build-lib`.
