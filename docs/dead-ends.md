# Dead ends — approaches that were tried and rejected

Recorded so that successor sessions do not relitigate them.  Each entry
includes (a) what was tried, (b) why it was attractive, (c) the concrete
failure mode that closed it, and (d) the lesson.

---

## 1. Spawn `factor.com` as a subprocess; speak a sentinel protocol over piped stdin/stdout

**What was tried.** Use `std::process::Command` to spawn `E:\factor\factor.com`
with `-i=...factor.image -run=listener -no-user-init -no-signals -q`, piping
all three standard handles.  Send transpiled Factor code; read responses
delimited by `%%NF-DONE%%`/`%%NF-READY%%` sentinels.

**Why it was attractive.** Process isolation, automatic crash recovery,
trivial restart, no FFI or CRT compatibility concerns.  Manual one-shot
runs of `factor.com -i=... -e='2 3 + .' -run=none` produce `5` on stdout
cleanly.

**Why it failed.**

- The Forth-on-Factor product is meant to *use Factor's VM as a stack-language
  compiler*, not to drive Factor-the-language as an opaque REPL.  Even when
  the pipe protocol works, we are still tied to Factor's surface syntax,
  its 128 MB standard image, its `command-line-startup`/`quit` lifecycle,
  and its listener's behaviour around prompts, banners, and EOF handling.
- The stock image's `command-line-startup` is built to *terminate the
  process* after the listener returns.  We have to keep the pipe open
  indefinitely to keep the VM alive — and any single eval that causes
  Factor to call `quit` (legitimately or via an unhandled error) takes
  the whole session down.
- Interactive listener-step prints prompts and stack snapshots between
  evals that we then have to suppress with `display-stacks? off` and
  redefining `prompt.` — fighting Factor's listener instead of using
  the VM directly.

**Lesson.** Pipes are a debugging tool, not an architecture.  We want
*FFI-level* integration with the VM; the listener does not belong in
the loop.

---

## 2. Load `factor.dll` in-process; patch its IAT to fix `GetModuleHandle(NULL)`

**What was tried.** `LoadLibraryW("factor.dll")` from our Rust process,
locate the IAT slot for `kernel32!GetModuleHandleW` by walking the PE
import directory, overwrite it with a hook that returns `factor.dll`'s
own HMODULE when called with `NULL`.  Then call the exported
`start_standalone_factor_in_new_thread`.

**Why it was attractive.** Surgical: a single-cell IAT write fixes the
exact bug (`factor_vm::init_ffi()` doing `hFactorDll = GetModuleHandle(NULL)`
and getting our host EXE's HMODULE rather than `factor.dll`'s).  No
recompile of the VM required.

**Why it failed.**

- After the patch worked, the next layer of in-process embedding broke:
  the VM's `init_factor` captures CRT `stdin`/`stdout` FILE* pointers
  via `VALID_HANDLE`, which falls back to `fopen("nul", ...)` whenever
  `_fileno(stdin) == -2` — exactly what holds in a Windows GUI-subsystem
  process.  `_dup2` updates the fd table but does not refresh the
  global `stdin` FILE* in MSVC's CRT.
- Even past that, `start_standalone_factor` runs Factor's `OBJ_STARTUP_QUOT`,
  which is `command-line-startup`, which still reaches for `ui.tools`
  unless we pass `-run=listener`, and still calls `quit` at the end.  We
  are right back at problem (1).
- We never actually need an unmodified `factor.dll`.  The goal is a slim
  custom VM with a clean embedding API.  Adding IAT patches to a 128 MB
  upstream image is the wrong direction of effort.

**Lesson.** Patching binaries to make them behave is a short-term
unblocking move at best.  If the goal is a tightly-integrated VM,
build the VM from source with the API we actually want exported.

---

## 3. Transpile ANS Forth source to Factor source text — *and expose Factor's surface to the user*

**Note (revised 2026-05-24):** the original entry here said "transpilation is
the dead end."  That was over-broad.  After actually building the embedded VM
and using it, we've concluded that *machine-generated, canonical, internal-only*
Factor source is a perfectly reasonable IR — Factor's parser is fast and
correct.  What's the real dead end is exposing Factor's surface (its parsing
words, its stack-effect syntax, its case sensitivity, its vocab system) to
the user.  The entry below has been kept for the historical record; the
*current* design uses machine-generated Factor source as an internal IR
between the Rust ANS Forth compiler and the VM.  See MANIFESTO.md mission
restatement point 2 for the current architecture.

**What was tried (the actual dead end).** A hand-written Rust `Transpiler`
that consumed ANS Forth source and emitted Factor source meant to be edited,
debugged, and read by the user.  Errors during parse showed Factor's
diagnostics with Factor's call stack frames.  Definitions reflected Factor's
stack-effect requirements verbatim.  Programs were *Factor-flavoured*, not
ANS-Forth.

**Why it failed.**

- The user, when something went wrong, found themselves debugging Factor —
  a language they did not know and never asked to learn.  Programs that
  looked like ANS Forth in the editor produced error messages that
  referenced quotations, the data stack pretty-printer, vocab-search-error,
  things with no equivalent in any ANS Forth book.
- Drift was inevitable: when an ANS construct didn't directly map, the
  natural fix was to "just use Factor's version", which leaked Factor
  idiom into the user's mental model.  Over time the language being
  written would have ceased to be ANS Forth at all.
- The original rationale also cited performance/type-info concerns about
  going through Factor's parser; on closer inspection those were
  overstated.  Factor's parser is fast and the optimising compiler handles
  the type inference at least as well as anything we'd write Rust-side
  for a quotation we just constructed.

**Lesson.**  *Factor-as-target is the architecture; Factor-as-language is
the trap.*  The IR boundary is internal; the user-facing language is
ANS Forth, full stop.  Machine-generated canonical Factor source as an
internal IR is fine — even good — because Factor's parser is the part of
Factor we want to use.  Hand-edited or user-visible Factor source is the
real dead end.
