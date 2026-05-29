# Numerics — vec2 and complex

CoreProtocols **Layer 2**: small numeric *value* classes that share an
arithmetic protocol. `vec2` is a 2-D vector (the graphics toys want
it); `complex` is a complex number. Components are floats.

The method bodies are written in **LET**, the infix-algebra DSL, so they
read like the mathematics — `ax + bx, ay + by` rather than a column of
stack shuffles. See [LET algebra](let-algebra.md) for the notation.

Load Layer 0 first (the `show` methods build on it):

```forth
NEEDS lib/core.f
NEEDS lib/numerics.f
```

---

## The arithmetic protocol

Four generics that *both* types implement. `v+` / `v-` / `vscale`
return the same type as their input; `vmag` returns a scalar.

| word     | stack effect    | meaning                              |
|----------|-----------------|--------------------------------------|
| `v+`     | `( a b -- c )`  | component-wise add                   |
| `v-`     | `( a b -- c )`  | component-wise subtract              |
| `vscale` | `( v k -- c )`  | multiply every component by scalar `k` |
| `vmag`   | `( v -- n )`    | magnitude (vec2) / modulus (complex) |

**Multiple dispatch is the point.** `v+` and `v-` key on the classes of
*both* arguments. `vec2 vec2 v+` and `complex complex v+` select
different methods — one verb, two backings, no privileged receiver. And
a mismatch (`vec2 complex v+`) simply finds no method, rather than
silently doing the wrong thing.

```forth
\ the same verb on two types, both reaching vmag = 10
2.0e 3.0e <vec2>     4.0e 5.0e <vec2>     v+  vmag .   \ 10.0
2.0e 3.0e <complex>  4.0e 5.0e <complex>  v+  vmag .   \ 10.0
```

---

## vec2 — a 2-D vector

`CLASS: vec2 SLOT: x SLOT: y`. Construct with the boa constructor
`<vec2> ( x y -- v )`; read with `vec2>x` / `vec2>y`.

Beyond the shared protocol, vec2 adds the **dot product** (a scalar, so
it isn't part of the same-type-in/out protocol):

| word  | stack effect   | meaning           |
|-------|----------------|-------------------|
| `dot` | `( a b -- n )` | `ax*bx + ay*by`   |

```forth
1.0e 2.0e <vec2>  3.0e 4.0e <vec2>  v+  VALUE r
r vec2>x .                       \ 4.0
r vec2>y .                       \ 6.0

3.0e 4.0e <vec2> vmag .          \ 5.0
1.0e 2.0e <vec2> 3.0e 4.0e <vec2> dot .   \ 11.0
3.0e 4.0e <vec2> 2.0e vscale     \ (6.0, 8.0)
3.0e 4.0e <vec2> show            \ (3.0 , 4.0 )
```

---

## complex — a complex number

`CLASS: complex SLOT: re SLOT: im`. Construct with `<complex> ( re im
-- z )`; read with `complex>re` / `complex>im`. It shares `v+` / `v-` /
`vscale` / `vmag` (modulus) and adds the genuinely complex operations:

| word   | stack effect   | meaning                            |
|--------|----------------|------------------------------------|
| `c*`   | `( a b -- c )` | full product `(ac-bd)+(ad+bc)i`    |
| `conj` | `( z -- z' )`  | conjugate `re - im·i`              |

```forth
\ (1+2i) + (3+4i) = 4+6i  — the shared v+, dispatched to complex
1.0e 2.0e <complex>  3.0e 4.0e <complex>  v+  show   \ 4.0 + 6.0 i

\ (1+2i)(3+4i) = -5+10i
1.0e 2.0e <complex>  3.0e 4.0e <complex>  c*  show   \ -5.0 + 10.0 i

3.0e 4.0e <complex> conj show    \ 3.0 + -4.0 i
3.0e 4.0e <complex> vmag .       \ 5.0   (modulus)
```

---

## A note on equality

You don't need to write `equals?` for these: Layer 0's default is
structural, so two `vec2`s (or two `complex`es) with equal components
already compare equal. Override it only if you want a looser notion —
say, comparing within a tolerance.

---

Back to [Home](index.md) | [CoreProtocols (design)](coreprotocols.md) |
[Collections](collections.md) | [LET algebra](let-algebra.md)
