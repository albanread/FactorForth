# Porting WF64's LET and managed-strings to NewFactor — feasibility

A diversion before resuming the ANS slog. Both WF64 extensions look
**genuinely useful** and **noticeably easier** to ship on the Factor
backend than they were on WF64's LLVM-MC backend, because we can
delegate the heavy lifting (codegen, GC, float unboxing) to Factor
instead of doing it ourselves.

---

## 1. `LET` — infix algebraic expression DSL

### What it is in WF64

A self-contained sub-language embedded inside Forth. Source between
`LET` and `END` is **not** read as Forth — it's parsed as infix
algebra and JIT-compiled to a standalone Win64 function. Example
straight from the test suite:

```forth
LET (z_re, z_im, x, y) -> (z_next_re, z_next_im, mag) =
    re, im, rmag
    WHERE re   = z_re * z_re - z_im * z_im + x
    WHERE im   = 2 * z_re * z_im + y
    WHERE rmag = re * re + im * im
END
```

The motivation is straight from the user's earlier note about
graphics: postfix arithmetic with several intermediate values turns
into wall of stack juggling. LET reads like math, takes named
inputs, returns named outputs, and supports `WHERE` for let-style
intermediates with topo-sort + cycle detection.

**Surface:**

| Feature | Examples |
|---|---|
| Operators | `+ - * /`, unary `-`, `**` (desugared to `pow`) |
| Comparisons | `< <= > >= == !=` returning 0.0 or 1.0 |
| Ternary | `select(cond, then, else)` |
| SSE-direct intrinsics | `sqrt abs min max floor ceil round trunc` |
| libm | `sin cos tan asin acos atan atan2 exp log pow hypot` |
| Constants | `pi`, `e`, numeric literals |
| WHERE | dependency-ordered, cycle-detected named intermediates |
| I/O shape | `(in1, …) -> (out1, …)`; multiple of each |

### How WF64 implements it

`src/let_lang/`:
- `parser.rs` (568 lines) — recursive-descent over the LET grammar
- `codegen.rs` (917 lines) — lowers to MC-flavour Intel asm,
  registers XMM allocation, emits `mov rax, addr; call rax` for libm
- `mod.rs` (405 lines) — orchestrator + integration tests

At Forth invocation, the immediate `LET` word slurps source up to
`END`, calls `compile()`, hands the asm to JASM/MCJIT, gets a
function pointer back, then emits an inline trampoline at HERE that
loads inputs from the FP stack, calls the JIT'd function, and pushes
outputs back.

### Porting to NewFactor

**Strategy: port the parser, throw away the asm codegen, lower to
Factor IR instead.**

Factor already gives us, for free:
- Float arithmetic at full native speed (its compiler unboxes
  through chains of `+ - * /` etc.)
- Every libm function as a Factor word (`math.functions`: `sin cos
  tan sqrt exp log pow ...`)
- Named locals via `::` colon definitions (`:: foo ( a b -- c ) a b
  * 2 + ;`) — exactly what `WHERE` wants
- A JIT — the speed ceiling is whatever Factor produces, which for
  straight-line float code is competitive with direct LLVM-MC output

**Lowering shape:**

```text
LET (x, y) -> (re, im) = a + 1, b * 2
    WHERE a = x*x - y*y
    WHERE b = 2*x*y
END
```

emits (Factor IR, conceptual):

```factor
:: nf-let-7 ( x y -- re im )
    x x * y y * -  :> a!
    2 x * y *      :> b!
    a 1 +
    b 2 * ;
```

…and the Forth-side `LET ... END` produces an `Item::LetCall(7)`
which `emit.rs` lowers to a plain call to `nf-let-7`. Factor's
compiler then unboxes the floats through the body and we get
near-native asm without writing any.

**Sizing the port:**

| Component | LoC | Notes |
|---|---|---|
| Lexer for LET tokens (`( ) , -> = WHERE END` plus operators/ids/numbers) | ~150 | self-contained; doesn't touch the Forth lexer |
| Parser → AST | ~400 | port WF64's parser.rs mostly verbatim |
| WHERE topo-sort + cycle detection | ~100 | port verbatim — backend-agnostic |
| Lowering AST → Factor IR string | ~250 | new; trivial recursive emit, no register allocation, no ABI |
| Integration into emit.rs + parse.rs (recognise `LET … END` at top level and inside `:`) | ~150 | parallels how `S"` is handled |
| Tests (port WF64 integration tests on `Session.eval`) | ~200 | one-to-one with WF64's |
| **Total** | **~1250** | vs WF64's ~1900 — and most of the savings are in NOT writing codegen |

**Estimated effort: 3–4 days.**

### Trade-offs vs WF64's implementation

| Concern | WF64 | NewFactor port |
|---|---|---|
| Peak speed | direct XMM, no boxing | Factor compiler unboxes inside one word, boxes at entry/exit |
| GC pressure | none (raw doubles) | one box per LET return value per call — fine for graphics-frame use, bad if called millions of times/sec |
| Implementation complexity | high (XMM regalloc + libm trampolines) | low (emit text, hand to Factor) |
| Cross-platform | Windows-x64-bound (MCJIT host symbols) | inherits Factor's portability |
| Error messages | bespoke `LetError` | bespoke `LetError` (same code) |

The boxing cost matters less than it sounds: a LET expression typically
has one entry and one exit; everything inside stays unboxed in
Factor's compiler. For graphics shaders / Mandelbrot-style inner
loops we'd want LET inside a hot loop that itself is one Factor
word — boxes only at the loop iteration boundary, which is fine.

For real-time-graphics path B (the shared float buffer we discussed
earlier), LET that writes directly into the buffer never boxes the
output at all — the trampoline does a raw memory store instead of
pushing a Factor heap float. **That's a natural future extension.**

### Recommendation

**Ship it.** The user-visible payoff is large (graphics/physics/audio
code becomes readable), the implementation cost is moderate
(~4 days), and it composes well with the float-FFI / shared-buffer
work that's already on the roadmap. The `WHERE` machinery alone
makes Mandelbrot/Julia kernels write themselves.

---

## 2. Managed strings (the `$`-suffix vocab)

### What it is in WF64

A handle-first, GC-tracked string library that sidesteps Forth's
four traditional broken string forms (counted, address-length pairs,
dictionary-baked literals, PAD). Two types:

- **`String`** — immutable, UTF-8 bytes, tagged pointer on the data
  stack, length encoded in GC header
- **`MutStringBuilder`** — mutable, capacity + length

WF64 surface (~30 words):

```
S$" lit"         compile-time literal
$len $clen       byte / codepoint length
$+               concatenation
$slice           substring extraction
$find $rfind     substring search (fwd / rev)
$contains?       substring presence
$starts? $ends?  prefix / suffix check
$upper $lower    case conversion (UTF-8-aware)
$cmp $ci-eq      compare / case-insensitive eq
$hash            FNV-style 64-bit hash
$repeat          n-fold repeat
$replace $split  substitution / tokenisation
$trim $ltrim $rtrim   whitespace trim
$valid $validate UTF-8 validity check
sb-new sb-len sb-capacity sb-clear         builder lifecycle
sb-append$ sb-append-codepoint sb-append-int sb-append-float
sb>string        builder finalisation
$>addr           legacy interop (lifetime-contract)
>$               legacy → managed (c-addr u → $)
int>$ $>int      number formatting / parsing
char>$ float>$ $>float                     conversion
```

### Why it's needed

WF64's `strings_design.md` makes the case crisply: every traditional
Forth string form requires the programmer to track lifetime and
ownership in their head, on top of stack depth. Modern code that
does anything beyond `S" hello" TYPE` ends up sad. Managed strings
fix this with one immutable handle type plus one builder type.

### Porting to NewFactor

**This is actually a *simpler* port than WF64 did, because Factor
already has the substrate.**

Factor's native types:
- `string` — immutable, Unicode (not just UTF-8), reference-counted
  via the GC
- `sbuf` (StringBuffer) — mutable
- `byte-array` — raw bytes, for the rare cases where you need them

Factor's native operations:
- `length head tail subseq append concat ...`
- `<upper> >upper >lower trim trim-head trim-tail`
- `start subseq?` (substring search), `head? tail?` (starts/ends)
- `split` (tokenisation), `replace`
- `hashcode` (or a dedicated FNV impl if we want WF64-bit-compatibility)
- All UTF-8/16 handling built in

**The port is almost entirely vocab translation** — define
NewFactor's `$len` as resolving to Factor's `length`, `$+` to
`append`, etc. The interesting work is in three places:

1. **`S$" lit"` compile-time literal.** Already half-built: our
   existing `S" lit"` emit-time special form (M2.10) compiles to
   `(nf-addr u)`. `S$"` would compile to a plain Factor string
   value on the stack. **30 lines in `emit.rs`.**

2. **Boundary with legacy (c-addr u) pairs.** `$>addr` and `>$`
   bridge the two worlds. Factor already lets us write
   `string>byte-array` and back; we just expose them under the
   ANS-extension names. **20 lines in `runtime.factor`.**

3. **The `sb-…` builder family.** Map onto Factor's `sbuf`
   primitives: `<sbuf>`, `push`, `>string`, `length>>`. **40 lines.**

**Sizing the port:**

| Component | LoC | Notes |
|---|---|---|
| Vocab translations in `forth.runtime` (one Factor line per `$word`) | ~60 | mostly `: $len  length ;` style |
| `S$"` emit-time special form | ~30 | parallels `S"` |
| Builder family wrappers | ~40 | wrap Factor's `sbuf` |
| Resolver entries in `resolve.rs` | ~30 | one per `$word` |
| Tests | ~150 | port WF64's demo + ttester-style assertions |
| **Total** | **~310** | vs WF64's ~1900 lines of asm + Rust |

**Estimated effort: 1–2 days.**

### Trade-offs

| Concern | WF64 | NewFactor port |
|---|---|---|
| String identity | tagged pointer with our GC tag (`= 4`) | Factor's native `string` type, runtime-typed |
| Lifetime | tracked by WF64 GC | tracked by Factor GC (already the default) |
| Encoding | UTF-8 only | Factor string IS Unicode; UTF-8/16 conversion at boundaries (host I/O is UTF-8) |
| Interop with ANS `(c-addr u)` | explicit `$>addr` with lifetime contract | same; Factor's `string>byte-array` is a copy, no lifetime trap |
| `$hash` | FNV-64 in the kernel | use Factor's `hashcode` OR a tiny Forth-side FNV impl for bit-compatibility |
| Performance | direct memory ops | Factor's string ops are well-optimised; not a hot path |

**One small philosophical question:** should `S$" hello"` produce
a Factor string (whose width is "character"), or a UTF-8 byte
string with the same length as WF64's? Likely the former — Factor's
strings already work everywhere Factor's strings work. If specific
WF64-compat needed, expose `S$utf8" hello"` as a byte-array variant.

### Recommendation

**Ship it.** The user-visible win is enormous — modern string code
finally has a safe path — and the implementation cost is trivial
relative to WF64's effort because Factor pre-built every primitive
we need. **1-2 days, almost zero risk.**

This also retires the `PAD` / lifetime-of-S" footguns that bite users
on every other ANS implementation.

---

## Sequencing

Order of operations against the ANS plan we already have:

1. **First**: M2.x quick-wins (task #38) — surface the latent words
   already in `forth.runtime`. ~30 min, no design risk.
2. **Second**: Managed strings port. 1-2 days. Modest scope,
   immediate user payoff. Doesn't depend on file access.
3. **Third**: Resume ANS Core completeness pass (#39). Half a day.
4. **Fourth**: LET. 3-4 days. Bigger lift, but the WHERE machinery
   alone is worth it.
5. **Then**: back to Phase 3.2 (file access) → 3.3 → 3.4 → test
   suite runner.

LET and managed strings together take ~5 days and ship two of the
most-asked-for "modern Forth" features. They sit between the
quick-wins and the heavier Phase 3.x work as a coherent diversion
that materially improves the language surface before the test-suite
slog begins.

---

## References

- `E:/WF64/src/let_lang/{mod,parser,codegen}.rs` — LET impl
- `E:/WF64/kernel/strings_managed.masm` — WF64 string primitives
- `E:/WF64/lib/core.f` — WF64 Forth-side wrappers (locals + str helpers)
- `E:/WF64/docs/strings_design.md` — managed-string design doc
- `E:/WF64/demos/strings.f` — user-facing demo of the `$` vocab
