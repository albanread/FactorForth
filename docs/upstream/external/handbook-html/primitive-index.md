# Factor Primitive Index

Fetched from `https://docs.factorcode.org/content/article-primitive-index.html`
on 2026-05-23.  This is **the** target list for our Rust ANS Forth back-end:
every primitive listed here is something the Factor VM already implements
and exports — we wire ANS Forth words to these, not reinvent them.

This is a comprehensive index of primitive operations in Factor, organized alphabetically.

## Stack Manipulation

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `-rot` | `( x y z -- z x y )` | Rotate stack elements |
| `2drop` | `( x y -- )` | Drop two stack items |
| `2dup` | `( x y -- x y x y )` | Duplicate two stack items |
| `2nip` | `( x y z -- z )` | Remove two items below top |
| `3drop` | `( x y z -- )` | Drop three stack items |
| `3dup` | `( x y z -- x y z x y z )` | Duplicate three stack items |
| `4drop` | `( w x y z -- )` | Drop four stack items |
| `4dup` | `( w x y z -- w x y z w x y z )` | Duplicate four stack items |
| `drop` | `( x -- )` | Drop one stack item |
| `dup` | `( x -- x x )` | Duplicate top stack item |
| `dupd` | `( x y -- x x y )` | Duplicate item below top |
| `nip` | `( x y -- y )` | Remove item below top |
| `over` | `( x y -- x y x )` | Copy item below top |
| `pick` | `( x y z -- x y z x )` | Copy third item to top |
| `rot` | `( x y z -- y z x )` | Rotate three stack items |
| `swap` | `( x y -- y x )` | Swap two stack items |
| `swapd` | `( x y z -- y x z )` | Swap two items below top |

## Object Creation

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `<array>` | `( n elt -- array )` | Create array with element |
| `<byte-array>` | `( n -- byte-array )` | Create byte array |
| `<callback>` | `( word return-rewind -- alien )` | Create callback |
| `<displaced-alien>` | `( displacement c-ptr -- alien )` | Create displaced alien pointer |
| `<string>` | `( n ch -- string )` | Create string with character |
| `<tuple-boa>` | `( slots... layout -- tuple )` | Create tuple via BOA constructor |
| `<tuple>` | `( layout -- tuple )` | Create empty tuple |
| `<wrapper>` | `( obj -- wrapper )` | Wrap object |
| `(byte-array)` | `( n -- byte-array )` | Internal byte array creation |

## Alien/FFI Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `alien-address` | `( c-ptr -- addr )` | Get alien address |
| `alien-cell` | `( c-ptr n -- value )` | Read cell from alien |
| `alien-double` | `( c-ptr n -- value )` | Read double from alien |
| `alien-float` | `( c-ptr n -- value )` | Read float from alien |
| `alien-signed-1` | `( c-ptr n -- value )` | Read signed byte from alien |
| `alien-signed-2` | `( c-ptr n -- value )` | Read signed short from alien |
| `alien-signed-4` | `( c-ptr n -- value )` | Read signed int from alien |
| `alien-signed-8` | `( c-ptr n -- value )` | Read signed long from alien |
| `alien-signed-cell` | `( c-ptr n -- value )` | Read signed cell from alien |
| `alien-unsigned-1` | `( c-ptr n -- value )` | Read unsigned byte from alien |
| `alien-unsigned-2` | `( c-ptr n -- value )` | Read unsigned short from alien |
| `alien-unsigned-4` | `( c-ptr n -- value )` | Read unsigned int from alien |
| `alien-unsigned-8` | `( c-ptr n -- value )` | Read unsigned long from alien |
| `alien-unsigned-cell` | `( c-ptr n -- value )` | Read unsigned cell from alien |
| `(dlopen)` | `( path -- dll )` | Load shared library |
| `(dlsym)` | `( name dll -- alien )` | Get symbol from library |
| `dlclose` | `( dll -- )` | Close shared library |
| `dll-valid?` | `( dll -- ? )` | Check if library is valid |
| `free-callback` | `( alien -- )` | Free callback |
| `current-callback` | `( -- n )` | Get current callback |

## Alien Write Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `set-alien-cell` | `( value c-ptr n -- )` | Write cell to alien |
| `set-alien-double` | `( value c-ptr n -- )` | Write double to alien |
| `set-alien-float` | `( value c-ptr n -- )` | Write float to alien |
| `set-alien-signed-1` | `( value c-ptr n -- )` | Write signed byte to alien |
| `set-alien-signed-2` | `( value c-ptr n -- )` | Write signed short to alien |
| `set-alien-signed-4` | `( value c-ptr n -- )` | Write signed int to alien |
| `set-alien-signed-8` | `( value c-ptr n -- )` | Write signed long to alien |
| `set-alien-signed-cell` | `( value c-ptr n -- )` | Write signed cell to alien |
| `set-alien-unsigned-1` | `( value c-ptr n -- )` | Write unsigned byte to alien |
| `set-alien-unsigned-2` | `( value c-ptr n -- )` | Write unsigned short to alien |
| `set-alien-unsigned-4` | `( value c-ptr n -- )` | Write unsigned int to alien |
| `set-alien-unsigned-8` | `( value c-ptr n -- )` | Write unsigned long to alien |
| `set-alien-unsigned-cell` | `( value c-ptr n -- )` | Write unsigned cell to alien |

## Fixnum Arithmetic

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `fixnum+` | `( x y -- z )` | Add fixnums |
| `fixnum+fast` | `( x y -- z )` | Add fixnums (fast variant) |
| `fixnum-` | `( x y -- z )` | Subtract fixnums |
| `fixnum*` | `( x y -- z )` | Multiply fixnums |
| `fixnum*fast` | `( x y -- z )` | Multiply fixnums (fast variant) |
| `fixnum/i` | `( x y -- z )` | Integer divide fixnums |
| `fixnum/i-fast` | `( x y -- z )` | Integer divide fixnums (fast) |
| `fixnum/mod` | `( x y -- z w )` | Divide and modulo fixnums |
| `fixnum/mod-fast` | `( x y -- z w )` | Divide and modulo (fast) |
| `fixnum-mod` | `( x y -- z )` | Modulo fixnums |
| `fixnum-fast` | `( x y -- z )` | Generic fixnum operation (fast) |

## Fixnum Bitwise Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `fixnum-bitand` | `( x y -- z )` | Bitwise AND on fixnums |
| `fixnum-bitnot` | `( x -- y )` | Bitwise NOT on fixnum |
| `fixnum-bitor` | `( x y -- z )` | Bitwise OR on fixnums |
| `fixnum-bitxor` | `( x y -- z )` | Bitwise XOR on fixnums |
| `fixnum-shift` | `( x y -- z )` | Bit shift on fixnums |
| `fixnum-shift-fast` | `( x y -- z )` | Bit shift (fast variant) |

## Fixnum Comparisons

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `fixnum<` | `( x y -- ? )` | Less than comparison |
| `fixnum<=` | `( x y -- z )` | Less than or equal comparison |
| `fixnum>` | `( x y -- ? )` | Greater than comparison |
| `fixnum>=` | `( x y -- ? )` | Greater than or equal comparison |

## Fixnum Conversions

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `fixnum>bignum` | `( x -- y )` | Convert fixnum to bignum |
| `fixnum>float` | `( x -- y )` | Convert fixnum to float |

## Bignum Arithmetic

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `bignum+` | `( x y -- z )` | Add bignums |
| `bignum-` | `( x y -- z )` | Subtract bignums |
| `bignum*` | `( x y -- z )` | Multiply bignums |
| `bignum/i` | `( x y -- z )` | Integer divide bignums |
| `bignum/mod` | `( x y -- z w )` | Divide and modulo bignums |
| `bignum-mod` | `( x y -- z )` | Modulo bignums |
| `bignum-gcd` | `( x y -- z )` | Greatest common divisor |

## Bignum Bitwise Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `bignum-bit?` | `( x n -- ? )` | Test bit in bignum |
| `bignum-bitand` | `( x y -- z )` | Bitwise AND on bignums |
| `bignum-bitnot` | `( x -- y )` | Bitwise NOT on bignum |
| `bignum-bitor` | `( x y -- z )` | Bitwise OR on bignums |
| `bignum-bitxor` | `( x y -- z )` | Bitwise XOR on bignums |
| `bignum-shift` | `( x y -- z )` | Bit shift on bignums |
| `bignum-log2` | `( x -- n )` | Log base 2 of bignum |

## Bignum Comparisons

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `bignum<` | `( x y -- ? )` | Less than comparison |
| `bignum<=` | `( x y -- ? )` | Less than or equal comparison |
| `bignum>` | `( x y -- ? )` | Greater than comparison |
| `bignum>=` | `( x y -- ? )` | Greater than or equal comparison |
| `bignum=` | `( x y -- ? )` | Equality comparison |

## Bignum Conversions

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `bignum>fixnum` | `( x -- y )` | Convert bignum to fixnum |
| `bignum>fixnum-strict` | `( x -- y )` | Convert bignum to fixnum (strict) |
| `bignum>bignum` | `( x -- y )` | Bignum conversion |

## Float Arithmetic

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `float+` | `( x y -- z )` | Add floats |
| `float-` | `( x y -- z )` | Subtract floats |
| `float*` | `( x y -- z )` | Multiply floats |
| `float/f` | `( x y -- z )` | Divide floats |

## Float Comparisons

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `float<` | `( x y -- ? )` | Less than comparison |
| `float<=` | `( x y -- ? )` | Less than or equal comparison |
| `float>` | `( x y -- ? )` | Greater than comparison |
| `float>=` | `( x y -- ? )` | Greater than or equal comparison |
| `float=` | `( x y -- ? )` | Equality comparison |
| `float-u<` | `( x y -- ? )` | Unordered less than |
| `float-u<=` | `( x y -- ? )` | Unordered less than or equal |
| `float-u>` | `( x y -- ? )` | Unordered greater than |
| `float-u>=` | `( x y -- ? )` | Unordered greater than or equal |

## Float Conversions

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `fixnum>float` | `( x -- y )` | Convert fixnum to float |
| `float>fixnum` | `( x -- y )` | Convert float to fixnum |
| `float>bignum` | `( x -- y )` | Convert float to bignum |
| `float>bits` | `( x -- n )` | Get bit representation of float |
| `bits>float` | `( n -- x )` | Create float from bits |
| `double>bits` | `( x -- n )` | Get bit representation of double |
| `bits>double` | `( n -- x )` | Create double from bits |

## Miscellaneous Arithmetic

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `both-fixnums?` | `( x y -- ? )` | Check if both are fixnums |

## Object Inspection

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `eq?` | `( obj1 obj2 -- ? )` | Identity equality test |
| `tag` | `( object -- n )` | Get object tag |
| `slot` | `( obj m -- value )` | Read object slot |
| `set-slot` | `( value obj n -- )` | Write object slot |
| `(clone)` | `( obj -- newobj )` | Clone object |

## Array Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `resize-array` | `( n array -- new-array )` | Resize array |

## Byte Array Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `resize-byte-array` | `( n byte-array -- new-byte-array )` | Resize byte array |

## String Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `resize-string` | `( n str -- newstr )` | Resize string |
| `string-nth-fast` | `( n string -- ch )` | Get string character (fast) |
| `set-string-nth-fast` | `( ch n string -- )` | Set string character (fast) |

## Memory Management

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `gc` | `( -- )` | Run garbage collection |
| `minor-gc` | `( -- )` | Run minor GC |
| `compact-gc` | `( -- )` | Run compacting GC |
| `size` | `( obj -- n )` | Get object size in bytes |
| `all-instances` | `( -- array )` | Get all instances |

## File I/O

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(fopen)` | `( path mode -- alien )` | Open file |
| `fclose` | `( alien -- )` | Close file |
| `fgetc` | `( alien -- byte/f )` | Read character |
| `fputc` | `( byte alien -- )` | Write character |
| `fflush` | `( alien -- )` | Flush file buffer |
| `fread-unsafe` | `( n buf alien -- count )` | Read bytes (unsafe) |
| `fwrite` | `( data length alien -- )` | Write bytes |
| `fseek` | `( offset whence alien -- )` | Seek in file |
| `ftell` | `( alien -- n )` | Get file position |
| `(file-exists?)` | `( path -- ? )` | Check if file exists |

## System Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(exit)` | `( n -- * )` | Exit with code |
| `die` | `( -- )` | Exit immediately |
| `nano-count` | `( -- ns )` | Get nanosecond counter |
| `enable-ctrl-break` | `( -- )` | Enable Ctrl-Break |
| `disable-ctrl-break` | `( -- )` | Disable Ctrl-Break |

## Thread/Context Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(sleep)` | `( nanos -- )` | Sleep for nanoseconds |
| `(set-context)` | `( obj context -- obj' )` | Set execution context |
| `(set-context-and-delete)` | `( obj context -- * )` | Set context and delete |
| `(start-context)` | `( obj quot -- obj' )` | Start context |
| `(start-context-and-delete)` | `( obj quot -- * )` | Start context and delete |
| `context-object` | `( n -- obj )` | Get context object |
| `context-object-for` | `( n context -- obj )` | Get context object for context |
| `set-context-object` | `( obj n -- )` | Set context object |
| `datastack-for` | `( context -- array )` | Get datastack for context |
| `retainstack-for` | `( context -- array )` | Get retainstack for context |

## Callstack Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `callstack-bounds` | `( -- start end )` | Get callstack bounds |
| `callstack-for` | `( context -- array )` | Get callstack for context |
| `callstack>array` | `( callstack -- array )` | Convert callstack to array |
| `set-callstack` | `( callstack -- * )` | Set current callstack |
| `set-datastack` | `( array -- )` | Set datastack |
| `set-retainstack` | `( array -- )` | Set retainstack |
| `innermost-frame-executing` | `( callstack -- obj )` | Get executing word in frame |
| `innermost-frame-scan` | `( callstack -- n )` | Get frame scan offset |
| `set-innermost-frame-quotation` | `( n callstack -- )` | Set frame quotation |

## Code Generation and Execution

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(call)` | `( quot -- )` | Call quotation (internal) |
| `(execute)` | `( word -- )` | Execute word (internal) |
| `c-to-factor` | `( -- )` | Return from C to Factor |
| `jit-compile` | `( quot -- )` | JIT compile quotation |
| `lazy-jit-compile` | `( -- )` | Lazy JIT compilation |
| `quotation-code` | `( quot -- start end )` | Get quotation code range |
| `quotation-compiled?` | `( quot -- ? )` | Check if quotation compiled |
| `word-code` | `( word -- start end )` | Get word code range |
| `word-optimized?` | `( word -- ? )` | Check if word optimized |
| `array>quotation` | `( array -- quot )` | Create quotation from array |

## Generic Dispatch

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `lookup-method` | `( object methods -- method )` | Look up method |
| `inline-cache-miss` | `( generic methods index cache -- )` | Handle cache miss |
| `inline-cache-miss-tail` | `( generic methods index cache -- )` | Handle cache miss (tail) |
| `mega-cache-lookup` | `( methods index cache -- )` | Mega cache lookup |
| `mega-cache-miss` | `( methods index cache -- method )` | Mega cache miss |

## Word/Symbol Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(word)` | `( name vocab hashcode -- word )` | Create word |
| `(identity-hashcode)` | `( obj -- code )` | Get identity hash code |
| `compute-identity-hashcode` | `( obj -- )` | Compute identity hash |

## Local Variable Operations

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `get-local` | `( n -- obj )` | Get local variable |
| `load-local` | `( obj -- )` | Load local variable |
| `load-locals` | `( ... n -- )` | Load multiple locals |
| `drop-locals` | `( n -- )` | Drop local variables |

## Debugging and Profiling

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `check-datastack` | `( array in# out# -- ? )` | Check datastack signature |
| `strip-stack-traces` | `( -- )` | Remove stack traces |
| `signal-handler` | `( -- )` | Handle signal |
| `leaf-signal-handler` | `( -- )` | Handle signal in leaf |
| `unwind-native-frames` | `( -- )` | Unwind native frames |
| `fpu-state` | `( -- )` | Get FPU state |
| `set-fpu-state` | `( -- )` | Set FPU state |
| `get-samples` | `( -- samples/f )` | Get profiler samples |
| `set-profiling` | `( n -- )` | Enable profiling |
| `dispatch-stats` | `( -- stats )` | Get dispatch statistics |
| `reset-dispatch-stats` | `( -- )` | Reset dispatch statistics |

## Memory Control

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(save-image)` | `( path1 path2 then-die? -- )` | Save image |
| `(callback-room)` | `( -- allocator-room )` | Get callback memory room |
| `(code-room)` | `( -- allocator-room )` | Get code memory room |
| `(data-room)` | `( -- data-room )` | Get data memory room |
| `(code-blocks)` | `( -- array )` | Get code blocks |
| `disable-gc-events` | `( -- events )` | Disable GC events |
| `enable-gc-events` | `( -- )` | Enable GC events |

## Formatting

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `(format-float)` | `( n fill width precision format locale -- byte-array )` | Format float |

## Code Heap Modification

| Primitive | Stack Effect | Purpose |
|-----------|--------------|---------|
| `modify-code-heap` | `( alist update-existing? reset-pics? -- )` | Modify code heap |
| `become` | `( old new -- )` | Become (identity swap) |

---

**Document Source:** Factor 0.102 x86.64 (2301, heads/master-7a7f571058, Mar 10 2026 18:04:59)
