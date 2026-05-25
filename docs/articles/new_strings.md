# Managed Strings in NewFactor

If you have programmed in classical ANS Forth, you know that string manipulation is historically one of the language's weakest points. You are forced to juggle `( c-addr u )` pairs on the stack, worry about the transient lifetime of the `PAD` buffer, or manually allocate dictionary space that cannot easily be reclaimed. 

NewFactor fundamentally fixes this by introducing **Managed Strings** (often called the `$`-suffix vocabulary). 

Instead of juggling memory addresses, managed strings are first-class, garbage-collected, immutable objects backed by the Factor VM's highly optimized native string types.

## The Problem with Old Strings

Standard Forth provides `S" hello"`. While NewFactor still supports this for strict ANS compliance, standard strings evaluate to two items on the stack: an address and a length `( c-addr u )`. 
Worse, standard operations that return new strings usually dump their results into a shared, volatile memory area called `PAD`. If you don't use or copy the data quickly, the next string operation will overwrite it.

## The New Factor Approach: `S$"`

In NewFactor, you create a Managed String using `S$"` (note the dollar sign):

```forth
S$" Hello, World!"
```

This evaluates to a **single item** on the data stack: an opaque managed string handle. 

Because it is backed by Factor's runtime:
- **It is Garbage Collected**: You never have to manually `ALLOT` or `FREE` strings. When they fall off the stack and are no longer referenced, the GC reclaims them.
- **It is Unicode-native**: Factor manages the UTF-8/UTF-16 encodings seamlessly.
- **It is Immutable**: Managed strings cannot be mutated in place, guaranteeing safety across complex data flows.

## Core Operations

Managed string words all begin or end with `$`. Because a string is a single stack item, the stack signatures are much cleaner.

| Operation | Word | Example / Stack Effect |
| :--- | :--- | :--- |
| **Length** | `$len` | `S$" hello" $len` → `5` |
| **Concatenation** | `$+` | `S$" foo" S$" bar" $+` → `S$" foobar"` |
| **Substring** | `$slice` | `( $str from to -- $sub )` |
| **Search** | `$find` | `S$" haystack" S$" needle" $find` → `index` |
| **Case Conversion** | `$upper` / `$lower` | `S$" UP" $lower` → `S$" up"` |
| **Comparison** | `$cmp` | `( $str1 $str2 -- -1/0/1 )` |
| **Print** | `$.` | Prints the managed string to output. |

*You also get modern conveniences like `$contains?`, `$starts?`, `$ends?`, `$trim`, and `$split`.*

## Mutable Construction: String Builders

Because Managed Strings are immutable, doing 1,000 concatenations in a loop using `$+` would generate 1,000 intermediate garbage strings. For high-performance mutation and iterative building, NewFactor uses **Builders**.

Builders are mapped to Factor's mutable `<sbuf>` (String Buffer).

```forth
sb-new              \ ( -- builder ) Create a new builder
S$" hello " sb-append$
42 sb-append-int 
sb>string           \ ( builder -- $str ) Finalize to an immutable string
```

**Builder words:** `sb-new`, `sb-len`, `sb-capacity`, `sb-clear`, `sb-append$`, `sb-append-codepoint`, `sb-append-int`, `sb-append-float`, `sb>string`.

## Bridging the Two Worlds

There will be times when you need to pass a Managed String into an old ANS Core word (like `TYPE`), or conversely, take an ANS string `( c-addr u )` returned by a legacy word and convert it to a Managed String.

NewFactor provides explicit bridge words:

- **`>$` ( c-addr u -- $str )**: Converts a legacy address/length pair into a safe, managed string. It copies the underlying bytes, so you don't have to worry about `PAD` lifetimes anymore.
- **`$>addr` ( $str -- c-addr u )**: Converts a managed string into an address/length pair for legacy interop. *Caution: this temporarily exposes the internal memory buffer.*

## Working with Numbers

Numeric conversions also bypass the dreaded `PAD` formatting words (`<# # #S #>`) by producing safe Managed Strings instantly:

- **`int>$`**: `42 int>$` → `S$" 42"`
- **`float>$`**: `3.14 float>$` → `S$" 3.14"`
- **`$>int` / `$>float`**: Parses the managed string back to numeric types.

## Summary

By appending `$` to your workflow, you leave behind pointer arithmetic, memory corruption, and `PAD` overwrites. You gain safe, ergonomic, GC-backed Unicode strings that can be seamlessly handed back and forth to legacy code whenever required.