# M2.x #39 — ANS Core completeness pass (2026-05-24)

Eighteen new ANS Core words exposed through the resolver, each
with a `T{ <code> -> <expected> }T`-style lock-in test.

## Shipped

| Category | Words |
|---|---|
| Arithmetic | `1+` `1-` `2*` `2/` |
| Division | `/MOD` `*/` `*/MOD` (all floored, consistent with our `MOD`) |
| Bit-shifts | `LSHIFT` `RSHIFT` |
| Stack pairs | `2DUP` `2DROP` `2SWAP` `2OVER` |
| Stack misc | `-ROT` |
| Cell-pair memory | `2@` `2!` |
| Memory clear | `ERASE` (= `0 FILL`) |
| Comparator | `0<>` |

19 assertions in `tests/session_ans_core.rs` lock these in,
covering both positive and negative operand ranges.

## What Factor surprisingly *didn't* have

A couple of values that turned up during the work:

- **Factor has no `2*`** — only `2/` (defined as `-1 shift` in
  `core/math/math.factor`). Wrapped as `ans2*` = `1 shift`.
- **Factor has no `2SWAP`** — implemented via `::` locals
  (`:: ans2swap ( a b c d -- c d a b )  c d a b ;`).
- **Factor's `2OVER` has different semantics than ANS** —
  Factor: `( x y z -- x y z x y )` (3-deep, equivalent to
  `over over`). ANS: `( a b c d -- a b c d a b )` (4-deep,
  copies items 3-4). Wrapped properly.
- **Factor's `pick` is fixed-position** — picks the 3rd item.
  ANS `pick` is parameterized (`n PICK` duplicates the `(n+1)`th).
  Different machinery entirely — deferred to #46.

## Floored division throughout

Our `MOD` is floored (M2.x #42).  For consistency, `/MOD`, `*/`,
and `*/MOD` are also floored:

```factor
:: ans/mod ( a b -- r q )
    a b floored-mod :> r
    a r - b /i      :> q
    r q ;
```

The remainder is computed via `floored-mod`; the quotient is
`(a - r) / b` which is always exactly divisible (so any
truncating `/i` gives the right answer).

`*/mod` is the same idea with `a*b` as the dividend, automatic
bignum promotion handling overflow in the intermediate multiply
(no manual widening needed).

## Lessons that fed into journal/docs

- **Resolver entry + effect entry + test = the contract.** Three
  places must agree. Missing the effect entry → row-vars
  fallback → Factor inference fail. Missing the test → silent
  rot when Factor's word shifts semantics out from under us
  (which is exactly what almost happened with `2over`).
- **Always check Factor's actual signature** before pointing the
  resolver at a `kernel:foo`. Names match the symbol, not the
  semantics. Found `2over` and `pick` both have ANS-mismatching
  arities and would have silently passed Factor's own tests
  with wrong runtime semantics.
- **`cbuffer` is indexed-access** — `<n> bufname` returns the
  address of byte `n`, not the buffer base. Tests had to use
  `0 buf` for the base. This was less of a Core bug than a
  test-author confusion, but worth documenting for the next
  person.
- **`::` locals are the right tool** for stack-shuffle words
  that defy stack-language idioms. `:: ans2swap ( a b c d -- c
  d a b )  c d a b ;` is unambiguous and the body is literally
  the stack picture. Factor's locals work here even though
  they failed for `?DUP` earlier — locals don't expose
  polymorphic effects, just renaming, so Factor's inference
  has no objections.

## Deferred to #46 (the stragglers)

These were in the original M2.x #39 plan but need real impl
work beyond resolver entries:

- `U<` `U>` — 64-bit unsigned compare via mask-then-compare
- `COUNT` — extract length from counted string; needs a model
  decision since our strings are nf-addr+u pairs
- `KEY?` — non-blocking input peek; needs new `rt_key_ready`
  FFI extern
- `EXIT` — Factor's `return` inside a colon def
- `.S` — non-destructive data-stack dump (`get-datastack`-based)
- `MOVE` — overlap-aware copy (CMOVE is forward-only)
- `PICK` — parameterized stack access via `get-datastack`
- `ROLL` — parameterized stack rotation

Each is small individually but they don't compose into a clean
batch like #39's wrappers did. Filed as #46.

## Test results

```
session_smoke         5/5  ok
session_io            5/5  ok
session_floats        3/3  ok
session_quickwins     7/7  ok
session_ans_booleans 19/19 ok
session_ans_core     19/19 ok   (NEW)
─────────────────────────────────
Session-based total  58/58 ok
smoke_runtime (leg.) 40/40 ok   (run separately, #31 fragility)
Grand total          98/98 ok
```

## Files touched

- `src/compiler/resolve.rs` — 18 new entries
- `src/compiler/effect.rs` — 17 effect entries (skipping `pick`)
- `factor/forth/runtime/runtime.factor` — 14 new wrappers
- `tests/session_ans_core.rs` — 19 lock-in tests
- `images/nf-mandelbrot.image` — rebuilt

## What's next

Per the layered plan:

1. **#43 managed strings** — independent, big visible win
2. **#35 / #33 / #32** — test-suite substrate (error
   translation, DEFER/IS, file access)
3. **#41 test runner** — when the substrate is in place

Stragglers (#46) can land in parallel between any of these
when the appetite suits.
