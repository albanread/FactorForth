# Managed strings

ANS Forth's string model is **`c-addr u`** — a raw memory
address plus a length.  You allocate a buffer with `create`, you
write bytes to it, you keep track of how long the contents are.
The compiler doesn't help; if you forget the length you get
garbage.

This works fine for tight embedded code where you control every
byte.  For everyday use — splitting input, concatenating
messages, comparing user-typed names — it's a chore.

FactorForth ships a **managed string** vocab (the `$-suffix`
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

### Construction

| word             | effect                              |
|------------------|-------------------------------------|
| `S$" hello"`     | ( -- $ ; literal )                  |
| `>$`             | ( c-addr u -- $ ; from raw )        |
| `n>$`            | ( n -- $ ; convert number to string ) |
| `$"  ... "`      | (parses as `>$`)                    |

### Inspection

| word             | effect                              |
|------------------|-------------------------------------|
| `$len`           | ( $ -- n )                          |
| `$.`             | ( $ -- ; print )                    |
| `$>`             | ( $ -- c-addr u ; to raw )          |

### Manipulation

| word             | effect                                  |
|------------------|-----------------------------------------|
| `$cat`           | ( $a $b -- $ab )                        |
| `$substr`        | ( $ start len -- $ )                    |
| `$upper`         | ( $ -- $ ; uppercase copy )             |
| `$lower`         | ( $ -- $ ; lowercase copy )             |
| `$trim`          | ( $ -- $ ; strip leading/trailing ws )  |
| `$reverse`       | ( $ -- $ ; reversed copy )              |

### Comparison

| word             | effect                              |
|------------------|-------------------------------------|
| `$=`             | ( $a $b -- ? )                      |
| `$<>`            | ( $a $b -- ? )                      |
| `$cmp`           | ( $a $b -- n ; <0 == 0 > 0 )        |

### Searching

| word             | effect                              |
|------------------|-------------------------------------|
| `$index`         | ( haystack needle -- pos \| -1 )    |
| `$contains`      | ( hay needle -- ? )                 |
| `$starts-with`   | ( $ prefix -- ? )                   |
| `$ends-with`     | ( $ suffix -- ? )                   |

## Examples

```forth
\ Build a greeting.
S$" Hello, "  S$" world!"  $cat  $.
\ -> Hello, world!

\ Conditional prefix.
: greet ( name$ -- )
    S$" Hi, " swap $cat S$" !" $cat $. cr
;
S$" Alice" greet
\ -> Hi, Alice!

\ User-provided test.
: shouting? ( $ -- ? )
    dup $upper $=
;
S$" QUIET"   shouting? .       \ -> -1
S$" not so"  shouting? .       \ -> 0

\ Conversion both ways.
S$" data"  $>  type           \ raw type from managed
\ -> data
s" raw"  >$  $.               \ managed from raw
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

`S$" ... "` is a FactorForth extension.  Stock ANS code that
uses `S" ... "` continues to work — that returns `c-addr u`,
the raw form.  If you're porting code from another Forth, you
don't have to migrate to `$`; the raw form remains supported.
