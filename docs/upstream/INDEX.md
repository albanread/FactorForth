# Factor VM documentation — collected upstream sources

Stage 1 of `MANIFESTO.md`.  This index catalogues everything we have locally
in this `docs/upstream/` tree so later stages can grep "where did the spec
say X?" without re-reading the whole Factor source.

**Last refresh:** 2026-05-23.  Status: **substantially complete.**  Local
mirror + canonical paper + rendered handbook indexes captured (1.74 MB
total: 277 vocab-docs, 289 summaries, 39 handbook source files, the DLS '10
paper PDF, primitive- and class-index in markdown).  No PhD thesis exists
(Pestov did Bachelors/Masters in Math, not PhD; the DLS '10 paper is the
only academic publication on Factor — confirmed by inspecting his home
page).  Blog archive at factor-language.blogspot.com still TODO if needed.

---

## What's here

```
docs/upstream/
├── INDEX.md                  ← this file
├── README.md                 ← Factor repo top-level README
├── CONTRIBUTING.md           ← Factor contributor guide
├── LICENSE.txt               ← BSD licence text
├── handbook/                 ← basis/help/ verbatim (39 files)
│                              The canonical Factor handbook source.
│                              ARTICLE: and HELP: definitions for the
│                              top-level handbook structure: language,
│                              system, tools, library, cookbook.
├── vocab-docs/               ← *-docs.factor for VM-critical vocabs
│                              (277 files, ~1 MB)
│   ├── core/                   data model: kernel, math, sequences,
│   │                           strings, words, quotations, classes,
│   │                           layouts, parser, syntax, vocabs, ...
│   └── basis/                  compiler pipeline, FFI, IO, listener,
│                               threads, continuations, memory, generic,
│                               stack-checker, bootstrap, images
├── vocab-summaries/          ← one-line summary.txt per vocab (289 files)
│                              Use as a vocab index when grepping for
│                              "what subsystem owns this concept".
├── vm-source-refs/
│   └── MANIFEST.md           ← table of every vm/*.{cpp,hpp} + src/*.zig
│                              file with its top-of-file design note.
│                              Source files themselves stay in their
│                              canonical location at E:\factor-src\.
└── external/
    ├── papers/
    │   └── dls10-pestov-ehrenberg-groff.pdf  (224 KB)
    │       *"Factor: a dynamic stack-based programming language"*
    │       Pestov / Ehrenberg / Groff, DLS 2010.  The ONE canonical
    │       academic paper on Factor.  Read first.
    └── handbook-html/
        ├── primitive-index.md   (165+ primitives with stack effects —
        │                         THE target list for ANS Forth back-end)
        └── class-index.md       (the 14 built-in classes + notes on
                                  the tuple-class universe we ignore)
```

---

## Top-level handbook entry points

Defined in `handbook/basis/help/handbook/handbook.factor`.  Browse with
`"<name>" help` inside a Factor listener if/when one is running, or read
the article body in the source file directly.

**Most relevant to our mission** (highest priority for Stage 2 read-through):

| Article name | What it covers |
|---|---|
| `handbook-system-reference` | The implementation — VM, GC, compiler, image format. This is the critical one. |
| `evaluator` | Stack machine model — how the VM actually runs code |
| `objects` | Object representation, tagged cells, headers |
| `numbers` | Fixnum / bignum / float / ratio layout and arithmetic |
| `stacks` | Data stack, return stack, callstack contracts |
| `primitive-index` | Catalogue of every `primitive_*` function — our ANS-Forth back-end target list |
| `tail-call-opt` | TCO semantics (matters for Forth recursive words) |
| `class-index` | Built-in class hierarchy |

**Useful but lower priority:**

| Article name | What it covers |
|---|---|
| `handbook-language-reference` | Factor surface language — we are not adopting this, but it explains the IR the compiler consumes |
| `conventions` | Coding / naming conventions (orientation only) |
| `cookbook` | How-to guides (helpful for understanding common patterns) |
| `io` | I/O streams and encodings (relevant when wiring stdio in the slim VM) |

**Ignore for our mission:**

`handbook-tools-reference`, `handbook-library-reference` (these cover the UI, the listener, the prettyprinter, the inspector — all of which we are stripping out).

---

## Mirrored vocab-docs — areas of focus

Inside `vocab-docs/` the files are organised by their original path under
Factor source.  When chasing a question, search by area:

| Subsystem | Where to look | Why we care |
|---|---|---|
| Stack primitives (`dup`, `drop`, `swap`, ...) | `vocab-docs/core/kernel/` | These ARE our ANS-Forth primitives renamed |
| Number representation | `vocab-docs/core/math/` and `core/math/{integers,floats,ratios,parser}/` | Forth `+`, `-`, `*`, `/`, `MOD` map to these |
| Arrays, byte-arrays, strings | `vocab-docs/core/{arrays,byte-arrays,strings,sequences}/` | Forth memory ops (`@`, `!`, `C@`, `CMOVE`) map here |
| Quotations | `vocab-docs/core/quotations/` | **The runtime representation we will emit from Rust.** |
| Words | `vocab-docs/core/words/` | **How words are represented in memory.** |
| Object layout, tagging | `vocab-docs/core/layouts/` | TAG bits, cell encoding — must be exact in our emitter |
| Classes (built-in hierarchy) | `vocab-docs/core/classes/builtin/` | Cross-reference with `objects` handbook article |
| Generic dispatch | `vocab-docs/core/generic/` | For Stage 6 (typed Forth dispatch) |
| Parser, syntax | `vocab-docs/core/{parser,syntax,lexer}/` | We *bypass* this with our Rust parser — but useful to understand what we're skipping |
| Compiler — tree IR | `vocab-docs/basis/compiler/` (top-level) | Front-end of the optimising compiler |
| Compiler — CFG IR | `vocab-docs/basis/compiler/cfg/` and its many subdirs | The unique part — stack→register lowering, linear-scan RA, intrinsics |
| FFI (alien) | `vocab-docs/{core,basis}/alien/` | Calling C from Factor — and reverse via `alien.remote-control` |
| Stack effects | `vocab-docs/core/effects/` and `basis/stack-checker/` | Static effect checking we will replicate Rust-side |
| Streams, I/O | `vocab-docs/core/io/` and `basis/io/` | OBJ_STDIN / OBJ_STDOUT machinery |
| Image / bootstrap | `vocab-docs/basis/{bootstrap,images}/` | How a Factor image is built — critical for the slim image task |
| Continuations | `vocab-docs/core/continuations/` and `basis/continuations/` | Stage 6d (coroutines) |
| Memory / GC | `vocab-docs/core/memory/` | Caller-visible GC interface |
| Listener | `vocab-docs/basis/listener/` | What we are *not* using — useful negatively |
| Threads | `vocab-docs/basis/threads/` | Stage 7 (concurrency) |

---

## VM source manifest

`vm-source-refs/MANIFEST.md` lists every C++ `vm/*.{cpp,hpp}` and Zig
`src/*.zig` file with its top-of-file design comment.  Use it to jump
straight to the relevant subsystem when reading Stage 2 (VM representation).

Key files to read first for Stage 2 (in order):

1. `vm/layouts.hpp` — TAG_MASK, type tags, cell encoding
2. `vm/objects.hpp` — `object`, `word`, `quotation`, `array`, `byte_array`, `string` C++ structs
3. `vm/primitives.hpp` + `vm/primitives.cpp` — primitive table and signatures (~165 entries)
4. `vm/code_blocks.hpp` + `vm/code_blocks.cpp` — JIT'd code-block format
5. `vm/image.hpp` + `vm/image.cpp` — image-file header + section layout
6. `vm/factor.cpp` — `init_factor`, `start_standalone_factor`, the lifecycle we are *replacing* with `nf_vm_*`
7. `vm/vm.hpp` — the `factor_vm` class itself

Zig mirror (`src/*.zig`) is layout-compatible by comptime assertion;
useful as a second perspective when a C++ comment is terse.

---

## Boot image — acquired

| Item | Status |
|---|---|
| `E:\factor-src\boot.windows-x86.64.image` (4.51 MB) | ✅ Downloaded from `https://downloads.factorcode.org/images/master/` on 2026-05-23.  MD5 `45212783fe17514e532b552549a2be3b` verified.  Smoke-tested with installed `E:\factor\factor.com`: Stage 1 init runs to completion, image format compatible. |
| Slim-image bootstrap mechanism | ✅ Already exists in stock Factor — see `basis/bootstrap/stage2.factor` lines 56–86.  `default-components` = `"math compiler threads io tools ui ui.tools unicode help handbook"`.  CLI args `include=...` and `exclude=...` override the set.  No source modifications needed. |

### Slim bootstrap procedure

From `E:\factor-src\`:

```
factor.com -i=boot.windows-x86.64.image ^
    include="math compiler threads io unicode" ^
    exclude="ui ui.tools tools help handbook"
```

Bootstrap takes **~5–20 minutes** of single-core CPU (paper §3.5 implies a few minutes on 2010 hardware; in practice with the optimising compiler loaded it's longer).  Output is written to `factor.image` next to the VM, **overwriting** any existing image — so we'll either rename it (`nf-slim.image`) or run the bootstrap in a working dir that has its own copy of `factor.com`.

The slim image will include:
- kernel, sequences, math (fixnum/bignum/float/ratio), strings
- the optimising compiler (this is what gives us LuaJIT-class output)
- threads, io (incl. `io.streams.c` for stdio FILE*)
- unicode (string protocol depends on it)
- everything the above transitively `require`s

The slim image will **exclude**:
- `ui` and `ui.tools` (the OpenGL development environment we never want loaded)
- `tools` (the prettyprinter, inspector, single-stepper — dev convenience, not needed in our embedded use)
- `help` and `handbook` (interactive documentation browsing)

**Expected slim-image size:** unknown until we run the bootstrap.  Educated guess: 20–40 MB based on the optimising compiler being a substantial fraction of the standard library.  Worst case ~60 MB.  Either way, dramatically less than 128 MB and easily redistributable.

A follow-up step after the basic slim image works: load `alien.remote-control` and our `forth.all` runtime into the slim image, then `save-image` to produce `nf.image` — that's the embeddable artifact our Rust code links against.

---

## External documentation — status

| Source | Status | Notes |
|---|---|---|
| **DLS 2010 paper (Pestov, Ehrenberg, Groff)** | ✅ Downloaded to `external/papers/dls10-pestov-ehrenberg-groff.pdf` | 224 KB.  Correct year is 2010, not 2008 as initially recorded.  Authoritative VM/compiler overview — read first. |
| **Pestov PhD thesis** | ✅ Confirmed does not exist | Pestov did Bachelors/Masters in Math, then moved to industry (now at Apple working on Swift).  His home page at `https://factorcode.org/slava/` lists only the DLS '10 PDF and one YouTube talk for Factor.  The DLS paper is the academic ceiling. |
| **Online handbook (docs.factorcode.org)** | ✅ Same content as in-source `*-docs.factor` files we already mirrored | Rendered version adds nothing of substance.  Two generated indexes that ARE useful (primitive-index, class-index) have been captured separately as markdown under `external/handbook-html/`. |
| **Pestov's home page** | ✅ Inventoried | Mostly Swift work now.  Only Factor links are the DLS PDF (captured) and a YouTube talk *"Factor: an extensible interactive language"* at `https://www.youtube.com/watch?v=f_0QlhYlS8g` (not fetched — video). |
| **Factor blog archive** | ⏳ Not fetched | `http://factor-language.blogspot.com/` reportedly has design-decision posts on inline caching, GC, write barriers from the Pestov era.  Defer until Stage 2 surfaces specific questions the DLS paper + source comments can't answer. |
| **Talks / YouTube** | ⏳ Not fetched | Two talks linked from Pestov's page.  Defer — text sources should suffice. |

**Stage 1 is substantially complete.**  All upstream *textual* documentation
worth reading offline is now in `docs/upstream/`.  The blog and talks are
optional supplements that only matter if Stage 2 (VM internals read-through)
exposes a question the local + DLS sources can't answer.

---

## How to use this index

1. **Looking for a concept?** → check the "Mirrored vocab-docs — areas of focus" table; grep inside that subdirectory.
2. **Looking for an implementation detail?** → start at `vm-source-refs/MANIFEST.md`, find the file, then open it at `E:\factor-src\vm\<file>`.
3. **Looking for the spec of a primitive?** → in a future revision, the `primitive-index` article will be extracted here; for now, search `vocab-docs/core/kernel/kernel-docs.factor` and friends.
4. **Need the big picture?** → read in this order: the DLS '08 paper (when fetched) → `evaluator` article → `objects` article → `handbook-system-reference` article.
