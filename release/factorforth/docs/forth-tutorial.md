# Forth tutorial

Forth is a stack-based language.  Instead of `f(a, b, c)`, you
push `a b c` and call `f`.  Instead of `a = 5`, you say `5
constant a`.  Instead of `if (x > 0) ...`, you say `x 0 > if ...
then`.

This tutorial covers everything you need to be productive.
Skim if you've used Forth before; if not, type each example at
the FactorForth prompt as you read.

## 1. The data stack

Most operations take their inputs from and put their outputs on
the **data stack** — a last-in-first-out store of integers (and
sometimes floats, strings, addresses).

```
> 5 7
> .s
<2> 5 7
```

`.s` shows the stack contents (depth + values) without
modifying it.  The depth `<2>` is the count.

Push, push, pop into print:

```
> 10 32 + .
42
```

Three operations:
1. `10` pushes 10
2. `32` pushes 32  (stack: 10 32)
3. `+`  pops both, pushes their sum  (stack: 42)
4. `.`  pops and prints

## 2. Stack shufflers

Forth makes you arrange the stack manually.  The basic shufflers
are:

| word  | effect             | what it does                       |
|-------|--------------------|------------------------------------|
| dup   | ( a -- a a )       | copy the top item                  |
| drop  | ( a -- )           | discard the top item               |
| swap  | ( a b -- b a )     | exchange top two                   |
| over  | ( a b -- a b a )   | copy second item to top            |
| rot   | ( a b c -- b c a ) | rotate top three left              |
| nip   | ( a b -- b )       | drop the second item               |
| tuck  | ( a b -- b a b )   | put a copy of top under second     |

The `( ... -- ... )` notation is a **stack effect**.  Inputs
left of `--`, outputs right.

## 3. Defining words

A word is a named procedure.  Define one with `:`...`;`:

```
> : square dup * ;
> 6 square .
36
```

Conventionally we annotate the effect:

```
> : square ( n -- n^2 ) dup * ;
```

The compiler reads this as a comment; humans read it as
documentation.

Words can call other words:

```
> : quad ( n -- n^4 ) square square ;
> 3 quad .
81
```

## 4. Conditionals

The basic shape is `... if ... then`:

```
> : pos? ( n -- ) 0 > if ." positive" then ;
> 5 pos?
positive
> -1 pos?
>
```

`."` (dot-quote) prints a string until the closing `"`.

With an else branch:

```
> : sign ( n -- )
    dup 0 > if ." +" drop
    else 0 < if ." -" else ." 0" then then
  ;
```

Note that `if`...`then` is the postfix bracket — the **then** is
where the *if* block ends.  No `endif`.

## 5. Loops

Counted loop with `do`...`loop`:

```
> : count-down ( n -- ) 0 swap do i . -1 +loop ;
> 5 count-down
5 4 3 2 1 0
```

Inside the loop, `i` is the current index.  `j` is the next outer
loop's index (Forth allows nesting).

Indefinite loop with `begin`...`until`:

```
> : countdown ( n -- )
    begin
        dup .
        1 - dup 0 =
    until
    drop
  ;
> 3 countdown
3 2 1 0
```

Or `begin`...`while`...`repeat`:

```
> : countup ( n -- )
    1 begin dup 2 pick <= while
        dup .
        1 +
    repeat
    2drop
  ;
> 4 countup
1 2 3 4
```

## 6. Variables and constants

```
> 100 constant max-count
> variable counter
> 0 counter !
> 1 counter +!
> 1 counter +!
> counter @ .
2
> max-count .
100
```

- `constant` defines a word that always pushes its value.
- `variable` defines a named cell.  `!` stores, `@` fetches,
  `+!` adds to the stored value.

## 7. Numbers and printing

Integers are 64-bit signed.  Print with:

- `.`   — decimal, with trailing space
- `u.`  — unsigned decimal
- `.r`  — right-justified in a width
- `hex` — switch base to 16
- `decimal` — back to base 10
- `cr`  — newline

```
> 255 hex . decimal .
ff 255
> -7 .r cr     ( prints "-7" right-justified )
```

## 8. Strings

Two flavours:

**`."` for inline printing** (the most common):

```
> : greet ( -- ) ." hello, world!" cr ;
> greet
hello, world!
```

**`S$"` for managed string values** (FactorForth extension):

```
> S$" hello" $.
hello
> S$" Hello, " S$" world" $cat $.
Hello, world
```

`$.` prints, `$cat` concatenates.  The `$` suffix marks the
managed-string vocab — separate from ANS Forth's raw c-addr/u
strings, which still exist if you need them.

## 9. LET algebra

Forth's postfix is great for some things and a chore for others.
Pure math reads better in infix.  FactorForth ships a LET DSL
that lets you write algebra naturally:

```
> : hypot ( a b -- c )
    LET (a b) -> (c) =
        sqrt (a * a + b * b)
    END
  ;
> 3 4 hypot f.
5.0
```

The DSL has `+ - * / sqrt sin cos tan ln exp` and the usual
precedence rules.  The compiler lowers it to stack ops for you.

## 10. Where to go from here

- The IDE's Editor pane (Tools menu) lets you write multi-line
  programs without losing context.
- `language-reference.md` lists every word FactorForth ships.
- `ide-guide.md` covers keyboard shortcuts.
- Read other people's Forth code — *Starting Forth* by Leo Brodie
  is the classic introduction, freely available online.

Welcome to Forth.
