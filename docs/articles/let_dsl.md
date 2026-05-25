# The LET DSL: Functional Algebra Inside Forth

Forth is incredibly powerful for machine control and stack manipulation, but it is notoriously painful for algebraic equations. Writing a simple formula to calculate the distance between two points, like $\sqrt{x^2 + y^2}$, requires excessive stack juggling (`dup * swap dup * + sqrt`), which quickly becomes a write-only cognitive tax.

To solve this, NewFactor introduces the **LET DSL** (Domain Specific Language). Borrowed from our legacy prototype (WF64), the `LET` block provides a fully functional, infix-algebra sub-language seamlessly embedded inside your ANS Forth code.

## Syntax and Structure

A `LET` block is explicitly bounded by `END`. Inside this block, the language switches from postfix Forth to standard mathematical infix.

```forth
LET (x, y) -> (dist) = sqrt(x * x + y * y) END
```

### Multiple Returns and Variables

You can take multiple inputs and return multiple outputs. You can also use `WHERE` clauses to define intermediate variables, keeping the equation readable and preventing repeated calculations:

```forth
LET (z_re, z_im, x, y) -> (z_next_re, z_next_im, mag) =
    re, im, rmag
    WHERE re   = z_re * z_re - z_im * z_im + x
    WHERE im   = 2 * z_re * z_im + y
    WHERE rmag = re * re + im * im
END
```
*(This is the actual inner loop of the Mandelbrot fractal renderer.)*

### Built-in Operations
The DSL supports:
*   **Arithmetic:** `+`, `-`, `*`, `/`, `**` (exponentiation).
*   **Comparisons:** `<`, `<=`, `>`, `>=`, `==`, `!=` (returning `1.0` or `0.0`).
*   **Math Intrinsics:** `sqrt`, `abs`, `min`, `max`, `floor`, `ceil`, `round`, `trunc`.
*   **Libm Functions:** `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `log`, `pow`, `hypot`.
*   **Constants:** `pi`, `e`.

## How it works under the hood

When the NewFactor compiler encounters a `LET` block, it doesn't parse an AST to interpret at runtime. Instead, it translates the infix algebra directly into Factor's intermediate representation (IR) using anonymous locals (`[| ... | ]`).

For example, the formula `LET (x, y) -> (out) = x + y END` roughly compiles down to:

```factor
[| nfl-x nfl-y |
    nfl-x nfl-y math:+ 
] call( nfl-x nfl-y -- nfl-out )
```

### Zero-Overhead Performance
This architectural choice is crucial. By lowering to Factor's IR, NewFactor delegates the heavy lifting to Factor's highly optimized JIT compiler. 
Factor natively:
1. Translates the local bindings.
2. Infers the types as unboxed floating-point numbers.
3. Compiles the chain of arithmetic operators into straight-line, native XMM CPU registers.

It produces machine code equivalent to a handwritten C function, completely bypassing the normal overhead of the Forth data stack. 

### Why Stop at Formulas?
Because `LET` blocks compile into such hyper-optimized machine code, they serve as the perfect engine for high-performance loops. They are the backbone of NewFactor's upcoming real-time graphics and physics capabilities, enabling you to write clean mathematical formulas and have them execute at native CPU speeds inside your Forth scripts.