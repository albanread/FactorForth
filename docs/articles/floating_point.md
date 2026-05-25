# Floating-Point Math in NewFactor

If you are writing scientific code, rendering graphics, or otherwise dealing with floating-point math, NewFactor handles numbers fundamentally differently (and much faster) than classical ANS Forth. 

In a traditional ANS Forth environment, floating-point numbers live on a separate, dedicated "Floating-Point Stack." You use specialized words (`F@`, `F!`, `F+`, `F*`, etc.) that operate exclusively on that dedicated stack. Moving data between the main data stack and the FP stack can be incredibly cumbersome.

NewFactor abolishes the separate FP stack entirely. By compiling down to Factor's runtime and JIT, NewFactor unifies integers and floats on a single data stack while leveraging Factor's immense backend optimizations to deliver near-native floating-point speed.

Here is what you need to know about the NewFactor floating-point model.

## 1. The Unified Data Stack

In NewFactor, there is **only one stack**. When you push a float (e.g., `3.14`), it goes right next to your integers and strings on the data stack.

Because of this unified stack, operations like `DUP`, `SWAP`, `ROT`, and `DEPTH` work transparently on floats without needing an `F` prefix (like `FDUP` or `FSWAP`).

## 2. Polymorphic Math and `F+` Aliases

Factor's math operators (`+`, `-`, `*`, `/`) are natively **polymorphic**—they dynamically accept any combination of integers and floats, upgrading integers to floats automatically during mixed math. 

To maintain compliance with Forth-2012 / ANS Forth, NewFactor still provides the traditional `F` prefixed words:
```forth
4.0 2.5 F+ . \ Prints 6.5
```
However, internally, words like `F+` and `F*` are compiled as direct aliases to Factor's `+` and `*`. This means you can use the standard ANS `F` words for compatibility, but you are actually executing Factor's deeply optimized polymorphic math under the hood.

*Note: Comparisons like `<` and `=` are also polymorphic, meaning `F<` and `F=` are treated as standard `<` and `=` against float stack entries.*

## 3. High-Performance Unboxing

The traditional downside of keeping floats on a generic "object stack" is the penalty of **boxing** (allocating a memory object to hold the 64-bit float). If NewFactor had to allocate heap memory for every single intermediate `F+`, complex algorithms like Mandelbrot fractals would be unusably slow.

Fortunately, we delegate this to the Factor optimizing compiler. 
When you write a compiled definition (`:`) or a `LET` block, Factor's static analyzer looks at the chain of math operations. Instead of allocating memory for every step, the JIT **unboxes the floats into native XMM hardware registers**, computes the entire expression using raw CPU instructions, and only "boxes" the final answer when returning it to the stack.

You get the safety and simplicity of a unified stack with the peak execution speed of direct machine code.

## 4. The `LET` DSL: Algebraic Ergonomics

Writing complex math formulas in Forth (e.g., calculating distance: $\sqrt{x^2 + y^2}$) usually results in an unreadable wall of stack juggling.

NewFactor introduces the `LET` DSL to leverage Factor's unboxing and optimization while providing clean, infix algebra:

```forth
LET (x, y) -> (dist) = sqrt(x * x + y * y) END
```

Behind the scenes, this does not interpret an AST at runtime. It compiles directly into a standalone Factor closure containing the libm `sqrt` function and optimized multiplication words. The runtime overhead is virtually zero, making this the preferred way to write high-performance inner loops.

## Limitations and Future Expansions

**Boxing inside tight loops:** While Factor unboxes temporary values *within* a compiled word or `LET` expression, it still boxes the final result returned to the stack. If you have an outer `DO ... LOOP` that repeatedly calls a math word millions of times per second, that boxing overhead at the word boundaries can add up and trigger GC pressure.

**The Solution:** We are actively rolling out a **shared float buffer** protocol (Milestone #37). This allows a `LET` expression to read and write unboxed `f64` data directly from/to arrays in Rust's memory space, entirely bypassing Factor's heap allocator. This zero-boxing FFI approach will allow NewFactor to power true real-time graphics vertex streams and audio synthesis.

## Summary
* **No `F-stack`**: Floats and integers live together.
* **Polymorphic**: `+` and `F+` are literally the same optimized operation.
* **Fast**: Factor automatically compiles your float math into XMM register operations.
* **Ergonomic**: Use the `LET` macro to write complex algebraic formulas effortlessly.
