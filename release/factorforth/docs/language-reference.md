# Language reference

Factor4th implements 95%+ of ANS Forth Core plus several
extensions.  This document lists what's available, organised by
topic.  See [Forth tutorial](forth-tutorial.md) for an
introduction to the basics.

Words marked **`new`** are Factor4th extensions not present
in stock ANS Forth.

## Literals

| form              | example                       | value                                |
|-------------------|-------------------------------|--------------------------------------|
| decimal           | `42`  `-7`                    | base-10 integer                      |
| hex               | `$ff`  `0xCAFE`               | base-16 integer                      |
| binary            | `%1010`                       | base-2 integer (= 10)                |
| explicit decimal  | `#42`                         | base-10 regardless of `BASE`         |
| float             | `1.5`  `2.5e`  `3e0`  `-1.25` | IEEE double                          |
| character **`new`** | `'a'`  `','`  `' '`         | the character's byte code (`'a'` = 97) |

### Character literals **`new`**

`'<c>'` pushes a single character's byte code as an integer — `'a'`
is `97`, `' '` is `32`, `','` is `44`. It is exactly sugar for the
number, so it composes anywhere an integer does:

```forth
'A' emit                 \ prints A
S" a,b,c" >string ',' split    \ split on a comma (see streams)
'0' CONSTANT zero-digit
```

Backslash escapes reach the characters you can't comfortably type
between two quotes:

| literal | code | character        |
|---------|------|------------------|
| `'\n'`  | 10   | newline          |
| `'\t'`  | 9    | tab              |
| `'\r'`  | 13   | carriage return  |
| `'\0'`  | 0    | NUL              |
| `'\s'`  | 32   | space            |
| `'\e'`  | 27   | ESC              |
| `'\\'`  | 92   | backslash        |
| `'\''`  | 39   | single quote     |
| `'\"'`  | 34   | double quote     |

The closing quote is what distinguishes a character literal from `'`
the **tick** (get-execution-token): `' foo` is tick + `foo`, while
`'f'` is the code for `f`. An unrecognised escape such as `'\x'` is
left as an ordinary word, so it surfaces as an unknown-word error
rather than a surprising value.

## Stack manipulation

| word    | effect                  |
|---------|-------------------------|
| dup     | ( a -- a a )            |
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
| depth   | ( -- n )                |

Return-stack: `>r  r>  r@  2>r  2r>  rdrop`.

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

Floats: arithmetic `f+ f- f* f/`, comparison `f< f> f=` (ANS `-1`/`0`
flags), memory `f@ f!`.  Transcendental functions (`sqrt`, `sin`,
`cos`, `tan`, `ln`, `exp`) are available **inside LET expressions**
(see [LET algebra](let-algebra.md)), not as standalone Forth words.

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
| emit    | ( c -- ; print char )           |
| type    | ( c-addr u -- ; print string )  |
| ." ... " | ( -- ; print inline )          |
| s" ... " | ( -- c-addr u ; string lit )   |
| cr      | ( -- ; emit newline )           |
| space   | ( -- ; emit space )             |
| spaces  | ( n -- ; emit n spaces )        |
| key     | ( -- c ; read one char )        |

Pictured number output: `<#  #  #s  sign  hold  #>`.

## Strings (Factor4th extension - $-suffix vocab) `new`

| word        | effect                            |
|-------------|-----------------------------------|
| S$" ..."    | ( -- $ ; managed string literal )  |
| >$          | ( c-addr u -- $ ; from raw )       |
| $>addr      | ( $ -- c-addr u ; to raw )         |
| int>$       | ( n -- $ ; number to string )      |
| $>int       | ( $ -- n ; parse to int )          |
| $len        | ( $ -- n ; length )                |
| $.          | ( $ -- ; print )                   |
| $+          | ( $a $b -- $ ; concatenate )       |
| $slice      | ( $ start len -- $ ; substring )   |
| $upper      | ( $ -- $ ; uppercase copy )        |
| $lower      | ( $ -- $ ; lowercase copy )        |
| $cmp        | ( $a $b -- n ; <0 / 0 / >0 — `$cmp 0=` for equality ) |
| $find       | ( hay needle -- index )            |
| $contains?  | ( hay needle -- ? )                |
| $starts?    | ( $ prefix -- ? )                  |
| $ends?      | ( $ suffix -- ? )                  |

See [managed-strings.md](managed-strings.md) for the full vocab.

## LET algebra (Factor4th extension) `new`

```
LET (inputs) -> (outputs) =
    <infix expression>
END
```

Inside the expression: `+ - * /`, parentheses, `sqrt sin cos
tan ln exp`, names from the inputs list.  See
[let-algebra.md](let-algebra.md).

## File access

| word              | effect                            |
|-------------------|-----------------------------------|
| included          | ( c-addr u -- ; load + run file ) |
| s" path" included | shortcut form                     |
| needs path        | load once — no-op if already loaded |

`NEEDS path` is the include-once directive: it reads, compiles, and
splices in the named file the first time it's seen, and does nothing on
a repeat. The path is a single blank-delimited token (use `INCLUDED`
for paths with spaces) and resolves relative to the file doing the
`NEEDS` — so a library can list its own dependencies at the top:

```forth
NEEDS lib/core.f          \ pulled in once, however many files ask for it
```

Unlike `INCLUDED` (a runtime word), `NEEDS` is resolved by the compiler
*before* the rest of the file: the included definitions are part of the
same compilation unit, so code after the `NEEDS` can use them directly.

## Other

- `\` and `( ... )` comments.
- `cells chars allot here` for raw memory work.
- `' name execute` for vectored dispatch (XTs as first-class).

## Not yet implemented

These standard ANS words aren't shipped yet — use the alternative:

| word            | use instead                                       |
|-----------------|---------------------------------------------------|
| `?DUP`          | `dup` then a plain `IF` (`dup IF … THEN`)          |
| `PICK` `ROLL`   | a `VALUE` or the return stack to stash items      |
| `.R`            | the pictured-number words (`<# … #>`)             |
| `2R@`           | `2R> 2DUP 2>R`                                     |
| `TRUE` `FALSE`  | the literals `-1` and `0`                         |
| `U<` `U>`       | signed `<` / `>` (no unsigned compare yet)        |
| `COUNT` `MOVE` `KEY?` | — (not yet available)                       |
| `:NONAME`       | a named `:` definition + `'` to get its xt        |

Float transcendentals (`FSQRT`, `FSIN`, …) are **not** Forth words;
the functions `sqrt` / `sin` / `cos` / `tan` / `ln` / `exp` are
available inside `LET` expressions instead.

## Differences from stock ANS

- Cells are 8 bytes (64-bit), not implementation-defined.
- `mod` is **floored** (matches Python, math), not truncated.
- Strings have **two** representations: managed `$` and raw
  `c-addr u`.  Pick what matches your task.
- `LET` and `S$"` are Factor4th extensions.

See `docs/ANS_GAP_ANALYSIS.md` (in the source repo) for a
detailed conformance breakdown.
