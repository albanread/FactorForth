# Factor Class Index

Fetched from `https://docs.factorcode.org/content/article-class-index.html`
on 2026-05-23.

## Built-in Classes

These are the *14 built-in classes* — every object in the VM has one of these
as its concrete type.  Our Rust emitter must know these tag values exactly
(see `vm/layouts.hpp` for the tag-to-type mapping).

| Class | Category |
|-------|----------|
| `alien` | Built-in |
| `array` | Built-in |
| `bignum` | Built-in |
| `byte-array` | Built-in |
| `callstack` | Built-in |
| `dll` | Built-in |
| `f` | Built-in (parsing word — the false object / sentinel) |
| `fixnum` | Built-in |
| `float` | Built-in |
| `quotation` | Built-in |
| `string` | Built-in |
| `tuple` | Built-in |
| `word` | Built-in |
| `wrapper` | Built-in |

## Tuple Classes

The full index contains several hundred tuple classes, primarily organised
into functional categories:

- **Compiler instruction classes** (`##add`, `##branch`, `##call`, …)
- **Compiler tree node classes** (`#branch`, `#call`, `#if`, …)
- **Graphics and rendering** — *we exclude these from our slim image*
- **Data structures** (`bit-array`, `bloom-filter`, `avl`, `buffer`, …)
- **Foreign function interface** (C struct wrappers like `CXCursor`, `GValue`, …)
- **Domain-specific types** — *most we exclude*

The 14 built-ins are the ones we care about; the tuple zoo is library code
and we ignore most of it.

---

**Document Source:** Factor 0.102 x86.64 (2301, heads/master-7a7f571058, Mar 10 2026 18:04:59)
