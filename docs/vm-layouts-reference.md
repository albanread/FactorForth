# VM data layouts — quick reference for the Rust emitter

From `vm/layouts.hpp` and `vm/objects.hpp` of the current Factor source.
This is the contract our Rust front-end emits against.

## Tagging

```
cell = uintptr_t    (8 bytes on 64-bit)
TAG_MASK = 15       (low 4 bits)
TAG(x)   = x & 15
UNTAG(x) = x & ~15
```

**Type tag values** (from `enum type_tags` in `vm/layouts.hpp`):

| Tag value | Type name | Notes |
|---:|---|---|
| 0 | FIXNUM_TYPE | Immediate.  Value stored in the upper 60 bits.  `tag_fixnum(n) = (n << 4) \| 0`. |
| 1 | F_TYPE | The false object `f`.  `false_object = F_TYPE = 1`.  Immediate. |
| 2 | ARRAY_TYPE | Heap.  Pointer in upper bits. |
| 3 | FLOAT_TYPE | Heap (boxed; compiler often unboxes). |
| 4 | QUOTATION_TYPE | Heap. |
| 5 | BIGNUM_TYPE | Heap. |
| 6 | ALIEN_TYPE | Heap.  Wraps a C pointer. |
| 7 | TUPLE_TYPE | Heap.  User-defined tuple instances. |
| 8 | WRAPPER_TYPE | Heap.  Wraps an object so it pushes-itself instead of executes. |
| 9 | BYTE_ARRAY_TYPE | Heap.  Packed binary. |
| 10 | CALLSTACK_TYPE | Heap. |
| 11 | STRING_TYPE | Heap. |
| 12 | WORD_TYPE | Heap.  A Factor word definition. |
| 13 | DLL_TYPE | Heap.  An open shared library handle. |

`immediate_p(obj) = TAG(obj) <= F_TYPE` — i.e. fixnums and `f` are
immediate; everything else is a heap pointer.

## Cell-level encodings

```rust
// Fixnum: 60-bit signed integer in the upper bits
fn tag_fixnum(n: i64) -> u64 { (n as u64) << 4 }   // tag = 0

// False / nil
const FALSE_OBJECT: u64 = 1;

// Heap pointer with a type tag attached
fn tag_ptr(ptr: usize, type_tag: u8) -> u64 {
    ((ptr as u64) & !0xF) | (type_tag as u64 & 0xF)
}
```

When emitting a quotation array, each cell is:

- `tag_fixnum(n)`           → literal integer `n` pushed at runtime
- `FALSE_OBJECT`            → `f` pushed at runtime
- `tag_ptr(word_ptr, 12)`   → call the word at runtime
- `tag_ptr(string_ptr, 11)` → push the string at runtime (it's literal)
- `tag_ptr(quot_ptr, 4)`    → push another quotation as a literal
- `tag_ptr(wrapper_ptr, 8)` → push a wrapped value (e.g. a `\foo` word reference instead of calling it)

## Object header

```
struct object {
  cell header;       // bits 0=free, 1=forwarding, 2..5=tag, 6..=hashcode
  // payload follows
};
```

Bit 0 (free): set in tenured-space free blocks.  Ignore for live objects.
Bit 1 (forwarding): set during GC compaction when this object has been
moved; `(header & ~3)` then points to the new location.
Bits 2..5: type tag (redundant with the tag in the cell pointing here).
Bits 6..end: hashcode (for identity-based hashing).

**Don't allocate raw memory from Rust and stamp this format yourself.**
Allocate via Factor primitives (`<array>`, `<byte-array>`, etc.) so the
header is correctly set up, the GC card table is correctly updated, and
the type table stays consistent.

## Concrete object layouts

### `array` (type 2)

```cpp
struct array {
    cell header;
    cell capacity;       // tagged fixnum
    cell data[capacity]; // each is a tagged cell
};
```

Size: `sizeof(cell) * (2 + capacity)` rounded up to `data_alignment` (16).

### `quotation` (type 4) — **THE thing we emit**

```cpp
struct quotation {
    cell header;
    cell array;          // tagged pointer to the body array
    cell cached_effect;  // tagged; cache of the inferred stack effect
    cell cache_counter;  // tagged fixnum; bumped on PIC invalidation
    cell entry_point;    // UNTAGGED machine code address
};
```

Size: 5 cells = 40 bytes on 64-bit.

Critical fields:
- `array` — the sequence of cells we built (literals + word refs)
- `entry_point` — set to `lazy_jit_compile`'s address initially; the
  compiler patches it after first JIT.

### `word` (type 12)

```cpp
struct word {
    cell header;
    cell hashcode;        // tagged
    cell name;            // tagged string
    cell vocabulary;      // tagged string
    cell def;             // tagged quotation (the body)
    cell props;           // tagged assoc (metadata)
    cell pic_def;         // tagged; alternative entry for direct non-tail calls
    cell pic_tail_def;    // tagged; alternative entry for direct tail calls
    cell subprimitive;    // tagged; machine code for sub-primitives
    cell entry_point;     // UNTAGGED machine code address
};
```

Size: 10 cells = 80 bytes on 64-bit.

When the Rust emitter wants to *call a word* from inside a quotation, it
puts `tag_ptr(word_address, WORD_TYPE)` into the quotation's body array.
The compiler emits a call to that word's `entry_point`.

### `string` (type 11)

```cpp
struct string {
    cell header;
    cell length;         // tagged fixnum: number of characters
    cell aux;            // tagged: aux byte-array for non-ASCII
    cell hashcode;       // tagged
    uint8_t data[];      // UTF-8 bytes for the ASCII path
};
```

ASCII-only strings can be constructed by writing UTF-8 to `data[]`.
Anything with characters above 0x7F needs the `aux` byte-array set up
properly — easier to construct via the `<string>` primitive or via
`utf8 malloc-string` (the path `eval-callback` uses).

### `wrapper` (type 8)

```cpp
struct wrapper {
    cell header;
    cell object;         // tagged: the wrapped value
};
```

Used when you want a word to be pushed onto the stack *as a value*
rather than called.  In Factor syntax `\ foo` means "the word foo as an
object" — that produces a wrapper around the word.

## What our Rust emitter actually does — sketch

To compile the Forth phrase `42 DUP +`:

1. Look up the Factor primitive for ANS Forth `DUP` — this is the word
   named `"dup"` in the `"kernel"` vocab.  Obtain its tagged word
   pointer via `nf_intern_word(vm, "kernel", "dup")` (a helper we'd add
   to the embedding API).

2. Similarly for `+` — note that ANS `+` doesn't specify integer-vs-float,
   so we'd map it to Factor's polymorphic `+` (in `"math"` vocab), which
   the optimising compiler will then specialise.

3. Build an array of 3 cells:
   ```
   [ tag_fixnum(42),                       // push literal
     tag_ptr(dup_word_ptr, WORD_TYPE),     // call dup
     tag_ptr(plus_word_ptr, WORD_TYPE) ]   // call +
   ```

4. Call `array>quotation` to wrap it in a quotation struct.

5. Cache the resulting quotation handle.

6. Execute via `nf_call_quotation(vm, quot_handle)`.

Subsequent calls to the same Forth phrase reuse step 6 only; the
quotation is JIT-compiled on the first call (lazy) and the machine code
is cached.

## What our Rust emitter must NOT do

- Allocate Factor objects by direct malloc + header stamping.  The GC's
  card table and the data-heap-region maps would be inconsistent.
- Hold a raw `*mut Quotation` across an allocation or eval call (the GC
  may move it).  Keep handles as tagged-cell `u64` values, re-resolve on
  access via VM helpers.
- Assume `entry_point` is stable across `save-image`.  It isn't — the
  next session loading the image gets fresh JIT addresses.

## Open layout questions

- **`vm_parameters` struct** — `init_factor(vm_parameters*)` takes one.
  Need to read `vm/image.hpp` to get its definition; we'll need to pass
  it from Rust.  (Defaults exist; only `image_path` typically matters.)
- **`context` struct** — `vm->ctx->push(cell)` is how primitives interact
  with the data stack.  Need to read `vm/contexts.hpp` to find the exact
  push/pop method signatures so we can either wrap them in `nf_*` or
  call them via the existing `VM_C_API context*` symbols.
- **`code_block`** — JIT'd code blocks have their own format.  We don't
  emit these directly (the compiler does), but we may need to inspect
  them for diagnostics.

All three are 5-minute reads when we get there.
