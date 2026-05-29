# Stack effects

Every Forth word has a **stack effect** — what it pops and what
it pushes.  Reading and writing effects is the single most
useful Forth skill.  Factor4th infers every word's effect at
compile time: a `( … )` comment that disagrees with the code is
reported as a **warning** (the compile proceeds, using the real
inferred effect), while a body whose branches can't be balanced
is rejected outright before it runs.  Either way you hear about
it at compile time, not as a runtime mystery.

## Notation

```
( before -- after )
```

- Items left of `--` are popped (consumed).
- Items right of `--` are pushed (produced).
- Top of stack is the **rightmost** item on each side.

Examples:

```
+       ( a b -- a+b )       \ pops two, pushes one
dup     ( a -- a a )         \ duplicates top
drop    ( a -- )             \ consumes one, produces none
.       ( n -- )             \ pops, prints (side effect, no result)
swap    ( a b -- b a )       \ rearranges, no net change in count
2dup    ( a b -- a b a b )   \ duplicates top two
```

## Reading effects

When you see:

```
: hypot ( a b -- c )
    LET (a b) -> (c) =  sqrt (a * a + b * b)  END
;
```

The `( a b -- c )` tells you:
- Call `hypot` with two values on the stack (call them `a b`).
- After it runs the stack has one value (the hypotenuse).

The names `a b c` are just documentation — they're not real
variables (well, in LET they are, but in plain Forth they're
just hints).

## Writing effects

When you define a word, write the effect right after the name:

```
: shout ( c-addr u -- )  type ." !" cr ;
```

The compiler infers the effect from the **code** and compares it
to your comment; if they disagree it warns, then emits the real
(inferred) effect anyway.  The comment is documentation — keep it
honest, because humans rely on it even though the compiler trusts
the code.

## Conditional effects

`IF`/`ELSE`/`THEN` requires both branches to have the **same**
net stack effect.  This is the compiler's main effect-check.

```
: signum ( n -- -1|0|1 )
    dup 0> if drop 1
    else dup 0< if drop -1
    else drop 0 then
    then
;
```

Each branch consumes the dup'd copy and produces one value, so
the net effect is balanced.  If you forget the `drop` in one
branch, the compiler flags it.

## Loop effects

`DO`/`LOOP` is `( limit index -- )`.  Inside the loop, you can
push and pop, but the *net* effect of the loop body must be
zero — otherwise the stack drifts each iteration.

```
: sum-to-n ( n -- sum )
    0 swap                  \ ( sum-acc n )
    1+ 1 ?do                \ ( sum-acc )
        i +                 \ accumulate; net effect is ( sum -- sum' )
    loop
;
```

`i` pushes the loop index (so net of `i +` is zero: pop the
accumulator, push it+i).  The loop overall consumes nothing
extra and produces nothing extra; only the surrounding
`0 swap` + final accumulator matter.

## Effect rows: `..a` and `..b`

For words that pass through arbitrary stack values — like
`call` or `execute` — Factor4th uses Factor's row-variable
notation:

```
execute   ( ..a xt -- ..b )
```

Means: "consumes some stack of shape `..a` plus an xt, produces
some stack of shape `..b`".  Row vars accept any actual stack
change.

You won't write row vars yourself often; they're for cases
where the effect genuinely depends on what's being called.

## When the check fails

A common error:

```
> : oops if 1 then ;
warning: stack effect mismatch:
    branch effect ( -- 1 ) does not match other branch effect ( -- )
```

The IF branch leaves a value; the implicit ELSE branch leaves
nothing.  Fix:

```
> : ok ( ? -- n ) if 1 else 0 then ;
```

Or keep both branches balanced by carrying a value through — here
each path leaves exactly one number:

```
> : pos-or-zero ( n -- n ) dup 0< if drop 0 then ;
```

## Cheat-sheet

| primitive | effect              |
|-----------|---------------------|
| `dup`     | ( a -- a a )        |
| `drop`    | ( a -- )            |
| `swap`    | ( a b -- b a )      |
| `rot`     | ( a b c -- b c a )  |
| `over`    | ( a b -- a b a )    |
| `nip`     | ( a b -- b )        |
| `tuck`    | ( a b -- b a b )    |
| `2dup`    | ( a b -- a b a b )  |
| `2drop`   | ( a b -- )          |
| `+`       | ( a b -- a+b )      |
| `-`       | ( a b -- a-b )      |
| `*`       | ( a b -- a*b )      |
| `/`       | ( a b -- a/b )      |
| `=`       | ( a b -- ? )        |
| `<`       | ( a b -- ? )        |
| `>`       | ( a b -- ? )        |
| `if/then` | ( ? -- )            |
| `do/loop` | ( lim idx -- )      |
| `i`       | ( -- idx )          |
