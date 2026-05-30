# Locals — `{: ... :}`

Factor4th supports Forth-2012 locals: named, lexical bindings that
the body can reference instead of juggling values on the data stack.
A local is captured **once per call**, so it is the right tool for
re-entrant algorithms — recursive descents, generic methods called
from inside a user `each`, anything where a `VALUE` would be
clobbered by an inner activation.

This page covers what's accepted, how it lowers, and the one
extension Factor4th adds on top of the standard: the `_`
anonymous-discard marker.

## Surface syntax

The block opens with `{:` and closes with `:}`. Between them, an
unrestricted sequence of names. Two positions are accepted:

### Head locals — right after the effect annotation

```forth
: hypot ( a b -- h ) {: a b :}
    a a *  b b *  +  sqrt ;
```

The block consumes the named slots from the data stack — the
rightmost name binds the topmost value — and runs the body with
those names in scope. The original arguments are **gone from the
data stack** once the block executes; everything the body needs has
to come from the locals or from values produced inside the body.

The names live for the whole body and are available across `IF`,
`DO`, `BEGIN`, and friends.

### Mid-body locals — anywhere in the body

```forth
: take ( c n -- d ) {: c n :}
    new-darray {: dst :}
    n c size min 0 ?do
        i c at  dst d-push
    loop
    dst ;
```

A `{: ... :}` block in the middle of a body consumes its names from
the data stack at that point and brings them into scope for the
**rest** of the body. Mid-body locals don't disappear at the end of
an enclosing `IF` or `DO` — they last until the colon definition
ends.

A common pattern: declare the args as head locals, then bind a
freshly created collection or an intermediate result as a mid-body
local right after producing it (the `new-darray {: dst :}` idiom
above).

## Why locals exist

Before locals, the idiom was the module-level `VALUE`:

```forth
0 VALUE foo-c
0 VALUE foo-xt
: foo ( c xt -- ... )
    TO foo-xt  TO foo-c
    ...
```

That worked, but every `VALUE` is **global**. A user-defined method
that calls `foo` from inside the xt body of an outer `foo` will
silently overwrite the outer activation's `foo-c` and `foo-xt`.
Locals are stored in Factor's per-call frame, so the inner call's
bindings don't touch the outer's. Re-entrant by construction.

This is why the collection algorithms in `lib/collections.f` use
locals — a `cmp` method that recursively sorts another collection
during `sort` will still see its own `key` and `j`.

## The `_` discard placeholder

Factor4th extends the standard syntax with one addition: a local
named exactly `_` is consumed from the stack but **not bound** to a
name. Multiple `_`s in the same block are independent (no collision)
and each consumes its own slot.

```forth
\ Object catch-all method: the effect demands `x`, the body ignores it.
METHOD: show ( x:object -- ) {: _ :} ." <object>" ;

\ Multi-method: only care about the outer args.
: take-outer ( a b c -- d ) {: a _ c :} a c + ;

\ Mid-body: skip past a flag we don't need.
: parse-pair ( x flag y -- pair ) {: x _ y :} x y pair> ;
```

### Why `_` and not a placeholder name?

You could write `{: a unused c :}` and just not reference `unused`,
but a real binding makes its way into the compiler's locals scope —
spelling errors elsewhere in the body might accidentally resolve to
it, and the intent ("I'm ignoring this") is not signalled.

`_` is a real keyword: the resolver knows to skip it, so a
mistyped `_` in the body fails with the normal "undefined word"
error rather than silently binding.

### What `_` is **not**

- It is **not** a placeholder that leaves the value on the data
  stack. The slot is consumed, exactly like a named local.
- It is **not** a wildcard match — `{:` doesn't pattern-match, it
  just binds.

If you want a value to **stay** on the data stack, don't declare it.
Locals consume from the top down, so any value that sits below the
declared block remains where it was.

## How it lowers

Both forms emit Factor's `locals` vocab:

| Forth                       | Factor                                       |
|-----------------------------|----------------------------------------------|
| `: f ( a b -- c ) {: a b :} body ;` | `:: f ( a b -- c ) body ;`                  |
| `... {: x y :} ...`         | `... :> y :> x ...` (rightmost binds top first) |
| `{: _ b :}` (head)          | `:: f ( _dN b -- ... ) ...`                  |
| `{: a _ :}` (mid-body)      | `:> _dM :> a` (rightmost-first; `_dM` consumes the top) |

The `_dN` placeholders are fresh per occurrence so two `_`s in the
same block don't collide; user code has no way to spell them.

## In `METHOD:` bodies

The same `{: ... :}` head and mid-body forms work inside `METHOD:`
definitions:

```forth
METHOD: show ( x:object -- )  {: _ :}
    ." <object>" ;

METHOD: take-edges ( a b c -- pair )  {: a _ c :}
    a c pair> ;
```

Because Factor's `multi-methods:METHOD:` is a parsing word that
doesn't, by itself, open a locals scope for `:>`, the emitter
routes any method-with-locals through a generated `::` **helper
word**.  The METHOD: line shrinks to a plain call into the helper:

```factor
:: z-show-mh1 ( _d1 -- ) "<object>" print-string ;
multi-methods:METHOD: z-show { object } z-show-mh1 ;
```

The helper name (`-mh1`, `-mh2`, …) is generated automatically and
not visible from user code — `SEE show` still shows the original
method source.  Re-entrancy and locals semantics are exactly the
same as for `:` defs, including the `_` discard marker.

This is the cleanest expression of catch-all methods: the object
default for `show`, the `drop`-the-input pattern in `new-like`
specialisations, and any other method whose effect dictates an arg
the body ignores.

## Effect-system interaction

The locals declaration is the authoritative input count.

- The declared `( a b -- c )` annotation, if present, is checked
  against the locals count: declaring three inputs and only two
  locals is a warning.
- The body is inferred as if it starts with an empty data stack
  (since the inputs are already in locals).
- `_` counts as one input slot, same as a named local.

A definition with locals can sit next to one without — the locals
form is per-definition, not per-file.

## See also

- [Stack effects](stack-effects.md) — how the compiler infers and
  checks `( a b -- c )`
- [Classes and methods](classes.md) — where locals show up most:
  multi-method dispatch fixes your effect, locals let you ignore
  the args you don't need
