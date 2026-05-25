# Language reference

FactorForth implements 95%+ of ANS Forth Core plus several
extensions.  This document lists what's available, organised by
topic.  See [Forth tutorial](forth-tutorial.md) for an
introduction to the basics.

Words marked **`new`** are FactorForth extensions not present
in stock ANS Forth.

## Stack manipulation

| word    | effect                  |
|---------|-------------------------|
| dup     | ( a -- a a )            |
| ?dup    | ( a -- a a \| 0 -- 0 )  |
| drop    | ( a -- )                |
| swap    | ( a b -- b a )          |
| over    | ( a b -- a b a )        |
| rot     | ( a b c -- b c a )      |
| -rot    | ( a b c -- c a b )      |
| nip     | ( a b -- b )            |
| tuck    | ( a b -- b a b )        |
| 2dup    | ( a b -- a b a b )      |
| 2drop   | ( a b -- )              |
| 2swap   | ( a b c d -- c d a b )  |
| 2over   | ( a b c d -- a b c d a b ) |
| pick    | ( ... n -- ... n-th )   |
| depth   | ( -- n )                |

Return-stack: `>r  r>  r@  2>r  2r>  2r@  rdrop`.

## Arithmetic

| word    | effect                  |
|---------|-------------------------|
| +       | ( a b -- a+b )          |
| -       | ( a b -- a-b )          |
| *       | ( a b -- a*b )          |
| /       | ( a b -- a/b ; floored ) |
| mod     | ( a b -- a%b ; floored ) |
| /mod    | ( a b -- a%b a/b )      |
| negate  | ( n -- -n )             |
| abs     | ( n -- \|n\| )          |
| min max | ( a b -- min/max )      |
| 1+ 1-   | ( n -- n+1 / n-1 )      |
| 2* 2/   | ( n -- n*2 / n/2 )      |
| lshift rshift | ( x n -- x<<n / x>>n ) |

Floats: `f+ f- f* f/  f.  fnegate  fabs  fsqrt  fsin  fcos  ftan  fln  fexp  fmin  fmax`.

## Comparisons

Return ANS Forth flags: `-1` for true, `0` for false.

| word | effect             |
|------|--------------------|
| =    | ( a b -- ? )       |
| <>   | ( a b -- ? )       |
| <    | ( a b -- ? )       |
| >    | ( a b -- ? )       |
| <=   | ( a b -- ? )       |
| >=   | ( a b -- ? )       |
| 0=   | ( n -- ? )         |
| 0<   | ( n -- ? )         |
| 0>   | ( n -- ? )         |
| and  | ( a b -- a&b )     |
| or   | ( a b -- a\|b )    |
| xor  | ( a b -- a^b )     |
| invert | ( a -- ~a )      |
| true | ( -- -1 )          |
| false| ( -- 0 )           |

## Control flow

```
if ... then
if ... else ... then
begin ... again              \ infinite
begin ... until              \ exit when flag true
begin ... while ... repeat
do ... loop                  \ counted, increment by 1
do ... +loop                 \ counted, increment by top of stack
case <n> of ... endof ... endcase
leave                        \ exit current do-loop
exit                         \ return from word
```

## Definitions

```
:    name body ;             \ colon definition
:noname body ; ( -- xt )     \ anonymous quotation
constant name                \ ( n "name" -- )
variable name                \ create a one-cell variable
create name [allot bytes]    \ define a named data buffer
does> ... ;                  \ template body for create
```

**Tick / execute:**
```
' name                       \ ( -- xt )  get execution token
execute                      \ ( xt -- )  call the xt
```

## Memory

| word    | effect                   |
|---------|--------------------------|
| @       | ( addr -- n )            |
| !       | ( n addr -- )            |
| +!      | ( n addr -- )            |
| c@      | ( c-addr -- byte )       |
| c!      | ( byte c-addr -- )       |
| cell+   | ( addr -- addr' )        |
| cells   | ( n -- n*8 )             |
| chars   | ( n -- n )               |
| allot   | ( n -- )                 |
| here    | ( -- addr )              |

Cells are 8 bytes (64-bit).

## I/O

| word    | effect                          |
|---------|---------------------------------|
| .       | ( n -- ; print decimal )        |
| u.      | ( u -- ; unsigned )             |
| .r      | ( n w -- ; right-justified )    |
| emit    | ( c -- ; print char )           |
| type    | ( c-addr u -- ; print string )  |
| ." ... " | ( -- ; print inline )          |
| s" ... " | ( -- c-addr u ; string lit )   |
| cr      | ( -- ; emit newline )           |
| space   | ( -- ; emit space )             |
| spaces  | ( n -- ; emit n spaces )        |
| key     | ( -- c ; read one char )        |

Pictured number output: `<#  #  #s  sign  hold  #>`.

## Strings (FactorForth extension - $-suffix vocab) `new`

| word    | effect                          |
|---------|---------------------------------|
| S$" ..."| ( -- $ ; managed string literal )|
| $.      | ( $ -- ; print )                |
| $cat    | ( $ $ -- $ ; concatenate )      |
| $len    | ( $ -- n )                      |
| $=      | ( $ $ -- ? )                    |
| $cmp    | ( $ $ -- n ; <0,0,>0 )          |
| $substr | ( $ start len -- $ )            |
| $upper  | ( $ -- $ ; uppercase copy )     |
| $lower  | ( $ -- $ ; lowercase copy )     |
| $trim   | ( $ -- $ ; strip whitespace )   |
| >$      | ( c-addr u -- $ ; from raw )    |
| $>      | ( $ -- c-addr u ; to raw )      |

See [managed-strings.md](managed-strings.md) for details.

## LET algebra (FactorForth extension) `new`

```
LET (inputs) -> (outputs) =
    <infix expression>
END
```

Inside the expression: `+ - * /`, parentheses, `sqrt sin cos
tan ln exp`, names from the inputs list.  See
[let-algebra.md](let-algebra.md).

## File access

| word              | effect                           |
|-------------------|----------------------------------|
| included          | ( c-addr u -- ; load + run file )|
| s" path" included | shortcut form                    |

## Other

- `\` and `( ... )` comments.
- `cells chars allot here` for raw memory work.
- `' name execute` for vectored dispatch (XTs as first-class).

## Differences from stock ANS

- Cells are 8 bytes (64-bit), not implementation-defined.
- `mod` is **floored** (matches Python, math), not truncated.
- Strings have **two** representations: managed `$` and raw
  `c-addr u`.  Pick what matches your task.
- `LET` and `S$"` are FactorForth extensions.

See `docs/ANS_GAP_ANALYSIS.md` (in the source repo) for a
detailed conformance breakdown.
