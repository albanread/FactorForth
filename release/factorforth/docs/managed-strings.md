# Managed strings

ANS Forth's string model is **`c-addr u`** — a raw memory
address plus a length.  You allocate a buffer with `create`, you
write bytes to it, you keep track of how long the contents are.
The compiler doesn't help; if you forget the length you get
garbage.

This works fine for tight embedded code where you control every
byte.  For everyday use — splitting input, concatenating
messages, comparing user-typed names — it's a chore.

Factor4th ships a **managed string** vocab (the `$-suffix`
words) that gives you string values you can pass around like
numbers.  The runtime handles allocation, length, GC.

## The two worlds

| world      | type           | example use            |
|------------|----------------|------------------------|
| ANS raw    | `c-addr u`     | hardware-adjacent code |
| Managed    | `$` (string)   | application-level text |

Both work; you can convert between them.  Most application code
should use managed strings.

## Vocabulary

### Construction & conversion

| word             | effect                                |
|------------------|---------------------------------------|
| `S$" hello"`     | ( -- $ ; managed-string literal )     |
| `>$`             | ( c-addr u -- $ ; from raw )          |
| `$>addr`         | ( $ -- c-addr u ; to raw )            |
| `int>$`          | ( n -- $ ; number to managed string ) |
| `$>int`          | ( $ -- n ; parse to integer )         |

(`n>$` also exists, but it yields a **raw** `c-addr u`, not a managed
`$` — use `int>$` for a managed string.)

### Inspection

| word             | effect                              |
|------------------|-------------------------------------|
| `$len`           | ( $ -- n ; length )                 |
| `$hash`          | ( $ -- n ; hash code )              |
| `$.`             | ( $ -- ; print )                    |
| `$.cr`           | ( $ -- ; print then newline )       |

### Manipulation

| word             | effect                                  |
|------------------|-----------------------------------------|
| `$+`             | ( $a $b -- $ab ; concatenate )          |
| `$slice`         | ( $ start len -- $ ; substring )        |
| `$upper`         | ( $ -- $ ; uppercase copy )             |
| `$lower`         | ( $ -- $ ; lowercase copy )             |

### Comparison

| word             | effect                              |
|------------------|-------------------------------------|
| `$cmp`           | ( $a $b -- n ; <0 / 0 / >0 )        |

There's no dedicated `$=`; test equality with `$cmp 0=`.

### Searching

| word             | effect                              |
|------------------|-------------------------------------|
| `$find`          | ( hay needle -- index )             |
| `$contains?`     | ( hay needle -- ? )                 |
| `$starts?`       | ( $ prefix -- ? )                   |
| `$ends?`         | ( $ suffix -- ? )                   |

## Examples

```forth
\ Build a greeting.
S$" Hello, "  S$" world!"  $+  $.
\ -> Hello, world!

\ Conditional prefix.
: greet ( name$ -- )
    S$" Hi, " swap $+ S$" !" $+ $. cr
;
S$" Alice" greet
\ -> Hi, Alice!

\ User-provided test.  There's no $=, so compare with $cmp 0=.
: shouting? ( $ -- ? )
    dup $upper $cmp 0=
;
S$" QUIET"   shouting? .       \ -> -1
S$" not so"  shouting? .       \ -> 0

\ Conversion both ways.
S$" data"  $>addr  type        \ raw c-addr u from managed
\ -> data
s" raw"  >$  $.                \ managed from raw
\ -> raw
```

## Memory and lifetimes

Managed strings are **immutable** — every operation returns a
new string.  This is by design: it makes them safe to pass
across word boundaries without thinking about ownership.
Storage is handled by Factor's GC.

If you build a lot of throwaway strings in a loop, the GC will
clean up.  Don't manually allocate; don't manually free.

## When to use raw `c-addr u` instead

- Interop with C / external libraries that expect pointer+len.
- Performance-critical inner loops where you can't afford to
  allocate (Factor's GC is fast, but allocation isn't free).
- Bit-level manipulation: editing individual bytes in place.

For everything else, use managed.

## Compatibility note

`S$" ... "` is a Factor4th extension.  Stock ANS code that
uses `S" ... "` continues to work — that returns `c-addr u`,
the raw form.  If you're porting code from another Forth, you
don't have to migrate to `$`; the raw form remains supported.
