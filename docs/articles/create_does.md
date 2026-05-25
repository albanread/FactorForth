# Demystifying CREATE and DOES> in NewFactor

If you are coming to NewFactor from a traditional ANS Forth environment, one of the biggest under-the-hood shifts you'll encounter is how we handle data definition and memory layout. 

In a classic Forth system, `CREATE` drops the current dictionary pointer (`HERE`) onto the stack, and `ALLOT` increments it. `DOES>` patches the execution behavior of the most recently defined word. In NewFactor, because we compile down to Factor's highly-optimizing JIT and rely on its garbage collector, we don't have a single linear `HERE` pointer. 

Instead, we treat `CREATE`, `ALLOT`, and `DOES>` as **compile-time templates backed by managed opaque buffers.** Here is what you need to know to use them effectively.

## How CREATE and ALLOT Work

When you write a line like this in NewFactor:

```forth
CREATE mybuf 10 CELLS ALLOT
```

The Rust frontend parses this entire sequence as a single unit. It doesn't execute `CREATE` and then later execute `ALLOT`. Instead, the compiler sees "a request for a managed buffer named `mybuf` sized to 10 cells".

Underneath, NewFactor emits a Factor byte-array (`<buffer>`) and a named accessor. When you evaluate `mybuf`, it doesn't push a raw 64-bit integer pointer; it pushes an opaque Factor `nf-addr`.

Because of this, standard operations like `@` and `!` work exactly as expected, but you cannot arbitrarily cast this address back to an integer, nor will it overflow into adjacent dictionary space.

## DOES> as a Macro Template

NewFactor implements `CREATE ... DOES>` not as a runtime closure, but as a compile-time macro expansion. 

Consider a standard defining word:

```forth
: ARRAY ( n -- )
    CREATE CELLS ALLOT
    DOES> swap CELLS + ;

10 ARRAY myarr
```

Here is what happens:
1. **Template Capture:** When the compiler parses `: ARRAY ... ;`, it notices both `CREATE` and `DOES>`. It registers `ARRAY` as a **Template**. The code between `CREATE` and `DOES>` is captured as the constructor (e.g., `CELLS ALLOT`). The code after `DOES>` is captured as the runtime evaluation body.
2. **Template Expansion:** When the compiler later sees `10 ARRAY myarr`, it pattern-matches this exact shape: `<literal-int> <template-name> <new-word-name>`.
3. **Synthesis:** It calculates the required size using the constructor rules at compile-time (10 * 8 bytes = 80 bytes), creates a new Factor buffer for `myarr`, and generates an accessor word whose body is a direct copy of the `DOES>` body.

*Note on address math:* Factor doesn't allow standard math (`+`) on opaque buffers. However, the NewFactor compiler intercepts words like `+` inside a `DOES>` body and safely rewrites them to `forth.runtime:nf-addr+`. To you, the code looks like normal Forth, but the backend keeps memory perfectly safe.

## Strengths of this Approach

*   **Memory Safety & GC:** Because every `CREATE` evaluates to an isolated Factor buffer, a buffer overrun will trigger a clean Factor bounds-check exception, rather than corrupting your dictionary or crashing the IDE process.
*   **Speed:** Factoring dictionary allocations out of the execution loop allows the JIT to aggressively optimize array accesses and avoid closure overhead at runtime.
*   **Simplified Tooling:** `nf-addr` pointers integrate flawlessly into Factor's inspector and our `S"` string management.

## Limitations and Differences from ANS

Because NewFactor resolves memory sizes and layouts structurally at compile-time, there are a few strict limitations compared to traditional ANS Forth:

1. **Static Sizes Only:** When instantiating a defining word (like `ARRAY`), the size argument **must** be a literal integer in the source text. You cannot calculate the size dynamically at runtime (e.g., `get-size ARRAY myarr` will fail to compile).
2. **No Dynamic ALLOT:** `ALLOT` is only valid immediately following a `CREATE` parsing phrase. You cannot arbitrarily call `10 ALLOT` later in your code to expand an existing buffer or to bump a global memory pointer.
3. **Isolated Memory:** In traditional Forth, `CREATE a 10 ALLOT CREATE b 10 ALLOT` might imply that `b` is exactly 10 bytes after `a`. In NewFactor, `a` and `b` are completely distinct, independent GC buffers. Pointer arithmetic crossing from one buffer into another is not possible.

For 95% of standard definitions—arrays, tables, mapped structs, and custom variable types—NewFactor's template approach works cleanly, seamlessly, and significantly faster than emulating a contiguous byte array.
