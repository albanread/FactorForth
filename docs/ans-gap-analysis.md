# NewFactor ANS Forth Gap Analysis (2026-05-24)

Static inventory of NewFactor's surface vs ANS Forth 2012 word sets,
cross-referenced against the Gerry Jackson / John Hayes test suite
shipped at `E:/wf32/forth2012-test-suite-master/`.

## Status legend

| Mark | Meaning |
|------|---------|
| ✅ | Implemented and reachable from user code (in resolve.rs or parse.rs) |
| 🟡 | Defined in `forth.runtime` but **not in the resolver** — user code can't see it. **Quick win:** add an entry to `builtin_table()`. |
| 🟠 | Stub-only (e.g. `HERE` returns 0, `ALLOT` drops). Surface present, semantics incomplete. |
| ❌ | Absent entirely |
| 🚫 | Intentionally won't ship (PAD, BLOCK) — modern equivalents exist or the model is incompatible |
| ⏳ | Planned in an existing milestone |

---

## 1. ANS Core Word Set

### 1.1 Stack manipulation

| Word | Status | Notes |
|------|--------|-------|
| `DUP` `DROP` `SWAP` `OVER` `ROT` `>R` `R>` `R@` | ✅ ✅ ✅ ✅ ✅ 🟡 🟡 🟡 | r-stack trio defined in runtime, missing resolver entries |
| `?DUP` | 🟡 | runtime has it, not exposed |
| `DEPTH` | 🟡 | runtime has it, not exposed |
| `2DUP` `2DROP` `2SWAP` `2OVER` | ❌ | Factor `kernel` has all; one-liner adds |

### 1.2 Arithmetic and comparison

| Word | Status | Notes |
|------|--------|-------|
| `+` `-` `*` `/` `MOD` `NEGATE` `ABS` `MIN` `MAX` | ✅ | `MOD` resolves to truncated; floored exists in runtime — **bug to fix** |
| `1+` `1-` `2*` `2/` | ❌ | trivial adds |
| `/MOD` `*/` `*/MOD` | ❌ | small adds |
| `=` `<>` `<` `>` `0=` `0<` `0>` | ✅ | `=` `<>` return Factor `t/f`, not ANS `-1/0`. **Audit before running tester.fr** — `T{ }T` will compare them. |
| `0<>` `U<` `U>` | ❌ | trivial adds |
| `AND` `OR` `XOR` `INVERT` | ✅ | |
| `LSHIFT` `RSHIFT` | ❌ | small adds |

### 1.3 Memory

| Word | Status | Notes |
|------|--------|-------|
| `@` `!` `+!` `C@` `C!` | ✅ | per the narrow-variable peephole model |
| `CELL+` `CELLS` `CHAR+` `CHARS` | ✅ | |
| `2@` `2!` | ❌ | small adds |
| `HERE` `ALLOT` | 🟠 | stubs — real allocation happens through `CREATE`/`ARRAY`/`FARRAY`/`CBUFFER` defining words at sema time. Test suite expects them to actually advance the dictionary. |
| `,` `C,` (compile literal into dictionary) | ❌ | Implied-by-CREATE flow makes these awkward |
| `MOVE` `ERASE` | ❌ | small adds |
| `ALIGN` `ALIGNED` | ❌ | likely no-op given Factor heap alignment |

### 1.4 Constants and variables

| Word | Status | Notes |
|------|--------|-------|
| `VARIABLE` `CONSTANT` | ✅ | with narrow-vs-wide narrowing analysis |
| `FCONSTANT` | ✅ | |
| `2CONSTANT` `2VARIABLE` | ❌ | small adds |
| `CREATE` `DOES>` | ✅ | template-instance model (M2.9b) |
| `>BODY` | ❌ | absent; we don't expose dictionary headers as addresses |

### 1.5 Defining words / dictionary

| Word | Status | Notes |
|------|--------|-------|
| `:` `;` | ✅ | |
| `IMMEDIATE` | ❌ | no immediate-word machinery; macros are an open design question |
| `LITERAL` | ❌ | tied to IMMEDIATE; the way we compile `[ ... ]` is parser-level not user-extensible |
| `POSTPONE` | ❌ | same family |
| `EXIT` | ❌ | small add (Factor's `return`) |
| `RECURSE` | ❌ | small add (self-reference is a sema concern) |
| `'` (tick) `[']` `EXECUTE` `COMPILE,` | ❌ 🟡 | `EXECUTE` defined in runtime, no resolver entry; tick needs name-lookup machinery |

### 1.6 Control flow

| Word | Status | Notes |
|------|--------|-------|
| `IF` `ELSE` `THEN` | ✅ | |
| `BEGIN` `UNTIL` `WHILE` `REPEAT` `AGAIN` | ✅ | |
| `DO` `?DO` `LOOP` `+LOOP` `LEAVE` `UNLOOP` `I` `J` | ✅ | |
| `CASE` `OF` `ENDOF` `ENDCASE` | ✅ | M2.6 |

### 1.7 I/O

| Word | Status | Notes |
|------|--------|-------|
| `.` `CR` `EMIT` `SPACE` `SPACES` `TYPE` `KEY` `ACCEPT` | ✅ | All routed through host streams (Phase 3.1) |
| `KEY?` | ❌ | needs non-blocking `peek` on `rt_read_char` — small Rust change |
| `U.` | 🟡 | runtime has it |
| `.R` `U.R` | ❌ | small adds (formatted) |
| `.S` | ❌ | dump data stack — toolstest expects it |

### 1.8 Pictured numeric output

| Word | Status | Notes |
|------|--------|-------|
| `<#` `#` `#S` `SIGN` `HOLD` `#>` | ✅ | M2.10b — pictured DSL without PAD |
| `n>$` | ✅ (extension) | non-ANS convenience |

### 1.9 Strings

| Word | Status | Notes |
|------|--------|-------|
| `S" …"` `." …"` | ✅ | emit-time special forms |
| `C" …"` | ❌ | emit falls through to Factor string (deferred) |
| `CMOVE` `FILL` `BL` | ✅ | |
| `CMOVE>` | ❌ | reverse-direction CMOVE — small add |
| `COUNT` | ❌ | small add (extract from `(c-addr u)`-style counted string) |
| `[CHAR]` `CHAR` | ❌ | parser-level, needs work |
| `PAD` | 🚫 | model has no PAD by design |

### 1.10 Numeric I/O

| Word | Status | Notes |
|------|--------|-------|
| `HEX` `DECIMAL` `OCTAL` `BINARY` | ✅ | |
| `BASE` | ❌ | not exposed as user variable; we'd need to surface Factor's `number-base` |
| `>NUMBER` | ❌ | parser-level numeric conversion; ttester uses it via `EVALUATE`-like paths |

### 1.11 Programming-tools (Core Ext / Tools)

| Word | Status | Notes |
|------|--------|-------|
| `.S` `?` `DUMP` `WORDS` `SEE` | ❌ | tools word set; useful in REPL but not blocking tests |

---

## 2. Other ANS Word Sets

### 2.1 Double-precision (`doubletest.fth`)

| Word | Status |
|------|--------|
| `S>D` `D>S` | 🟡 (defined as identity in runtime — single-cell host) |
| `D+` `D-` `D*` `DABS` `DNEGATE` `D=` `D<` `D0=` | ❌ |
| `2CONSTANT` `2VARIABLE` `2LITERAL` | ❌ |

On a 64-bit host where `cell == double-cell` (we are one) much of the
double set collapses to single-cell ops. Worth shipping as aliases.

### 2.2 Floating-point (`fp/` tests)

| Word | Status |
|------|--------|
| `F@` `F!` | ✅ |
| `F+` `F-` `F*` `F/` `F<` `F>` `F=` | 🟡 (defined as aliases of int versions; need resolver entries) |
| `D>F` `F>D` | 🟡 |
| `FDUP` `FDROP` `FSWAP` `FOVER` `FROT` | ❌ |
| `F.` `FE.` `FS.` `F>S` | ❌ |
| `FSIN` `FCOS` `FSQRT` `FLN` `FEXP` `F**` | ❌ (Factor `math.functions` has them — wrap and expose) |
| `FVARIABLE` `FCONSTANT` | ✅ partial — `FCONSTANT` done, `FVARIABLE` not |
| Separate FP stack | n/a | Factor uses unified stack; our compile already handles this |

### 2.3 Exception (`exceptiontest.fth`)

| Word | Status | Notes |
|------|--------|-------|
| `CATCH` `THROW` `ABORT` `ABORT" …"` | ❌ | M2.11 territory; Factor has `recover` (≈ CATCH) and `throw` directly |

### 2.4 Memory-allocation (`memorytest.fth`)

| Word | Status |
|------|--------|
| `ALLOCATE` `FREE` `RESIZE` | ❌ |

Practical wrappers around Factor's byte-array allocation. Not hard.

### 2.5 File-access (`filetest.fth`)

| Word | Status |
|------|--------|
| `OPEN-FILE` `CLOSE-FILE` `READ-FILE` `WRITE-FILE` `R/O` `R/W` `W/O` `BIN` `READ-LINE` `WRITE-LINE` `FILE-POSITION` `REPOSITION-FILE` `FILE-SIZE` `FILE-STATUS` `DELETE-FILE` `RENAME-FILE` `INCLUDED` `INCLUDE-FILE` | ❌ | ⏳ Phase 3.2 (task #32) |

`INCLUDED` is the gate to running the canonical test suite.

### 2.6 Locals (`localstest.fth`)

| Word | Status |
|------|--------|
| `LOCALS\|` `{` (the local-declaration syntax) | ❌ |

WF32's runner warns this currently fails; we're not the worst class
of citizen here. Factor has `::` but the ANS syntax is different.

### 2.7 Search-order (`searchordertest.fth`)

| Word | Status |
|------|--------|
| `VOCABULARY` `WORDLIST` `SET-CURRENT` `GET-CURRENT` `SEARCH-WORDLIST` `FIND` | ❌ |

Factor has vocabularies but the ANS surface is different. Big API
to map — defer.

### 2.8 Block (`blocktest.fth`)

| Status | 🚫 |
|--------|-----|

WF32 disables this in its runner. ANS block words assume 1K
fixed-size storage units backed by raw I/O — no modern Forth ships
this seriously. Won't ship.

---

## 3. Quick wins (resolver-only adds)

These words are already defined in `factor/forth/runtime/runtime.factor`
but missing from `builtin_table()` in `src/compiler/resolve.rs`.
**One-line additions per word; immediate user-visible win.**

```
?DUP DEPTH
>R R> R@ RDROP 2>R 2R>
U.
S>D D>S
F+ F- F* F/ F< F> F=
D>F F>D
EXECUTE
```

Estimated effort: ~30 minutes. Should be a single commit named
"M2.x: surface latent runtime words in resolver."

---

## 4. Small wins (≤ a line of Factor each)

Words trivially expressible in terms of existing Factor primitives,
worth bundling into one "ANS Core completeness" pass:

```
1+ 1- 2* 2/ /MOD */ */MOD
0<> U< U>
LSHIFT RSHIFT
2@ 2! 2DUP 2DROP 2SWAP 2OVER
COUNT CMOVE>
-ROT PICK
MOVE ERASE
KEY?
EXIT
.S
```

Estimated effort: half a day.

---

## 5. Test-suite integration plan

The canonical Forth 2012 test suite uses `T{ <code> -> <expected> }T`,
defined in `ttester.fs`. Running it requires:

1. **`INCLUDED`** (Phase 3.2 / task #32). Without file I/O we can't
   pull in `prelimtest.fth`, `tester.fr`, etc.
2. **`>NUMBER`** or `EVALUATE` (ttester parses expected values from
   source). `EVALUATE` is the simpler path — Factor's `eval` already
   does the work.
3. **Boolean conventions.** `T{ ... -> ... -1 }T` expects ANS
   `-1` for true; our `=` returns Factor's `t`. **Either ship a
   pre-test compatibility shim** (`: TRUE -1 ; : FALSE 0 ;` plus
   rewriting `=` to return `-1`/`0`) **or** patch the comparator
   resolutions. Easier path: add a `boolean>flag` wrap at emit time
   for every comparator. Audit before running.
4. **The `T{ }T` parser**. It's plain ANS Forth — should run once
   the words it depends on do.

Suggested staging:
- **M3.0.1**: ship quick-wins (Section 3) + small-wins (Section 4)
- **Phase 3.2**: file access — unblocks `INCLUDED`
- **M3.0.2**: comparator-flag convention pass
- **M3.0.3**: stand up the test runner with `prelimtest.fth` + the
  `core.fr` tester only; iterate on failures.

A pass/fail/nyimp tabulator on top of `T{ }T` is a 30-line addition
to `ttester.fs` — override `ERROR` to categorise (incorrect / wrong
count / undefined-word-abort) and emit a final summary table.

---

## 6. Open questions

- **Boolean convention.** Do we ship Forth-true-is-`-1` everywhere
  (and pay the conversion at every comparison), or use a sema-side
  policy where comparisons in conditional context don't convert?
  The latter is cheaper at runtime but the test suite checks the
  raw stack value.

- **`IMMEDIATE` / `POSTPONE` family.** Skipping these means we
  can't host the `T{ }T` parser if it relies on immediate words.
  Worth checking ttester.fs's actual word definitions before
  committing to a path.

- **What's an acceptable "nyimp" rate** before we declare the
  ANS subset shipped? My read of the M2 manifesto is "applications
  Forth not micros" — locals/blocks/search-order absent is fine.
  Core + Core-Ext + Exception + Memory + File + Double + Float
  is the realistic gate.

---

## References

- `E:/wf32/forth2012-test-suite-master/` — canonical test suite
- `E:/wf32/anstests32.bat` — known-good invocation
- `E:/NewFactor/src/compiler/resolve.rs:100-223` — current builtin table
- `E:/NewFactor/factor/forth/runtime/runtime.factor` — Factor side
- ANS Forth 1994 / Forth 2012 standard documents
