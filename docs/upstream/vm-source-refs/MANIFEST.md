# VM source manifest

These files are not copied here — they live in their canonical location at
`E:\factor-src\vm\`.  This index lists each file's top-of-file comment so
we can grep "where is X documented in the VM source" without opening 146 files.

| File | KB | Top-of-file blurb |
|------|---:|-------------------|
| `aging_collector.cpp` | 2.3 | Is there another forwarding pointer? |
| `aging_space.hpp` | 0.6 | *(no header comment)* |
| `alien.cpp` | 4.7 | gets the address of an object representing a C pointer, with the intention of storing the pointer across code which may potentially GC. |
| `allot.hpp` | 2.9 | It is up to the caller to fill in the object's fields in a meaningful fashion! |
| `arrays.cpp` | 2.7 | Allocates memory |
| `arrays.hpp` | 0.8 | Allocates memory |
| `assert.hpp` | 0.5 | *(no header comment)* |
| `atomic.hpp` | 0.6 | *(no header comment)* |
| `atomic-cl-32.hpp` | 1.5 | *(no header comment)* |
| `atomic-cl-64.hpp` | 1.4 | *(no header comment)* |
| `atomic-gcc.hpp` | 1.4 | *(no header comment)* |
| `bignum.cpp` | 63.2 | Special cases win when these small constants are cached. |
| `bignum.hpp` | 2 | Copyright (C) 1989-1992 Massachusetts Institute of Technology Portions copyright (C) 2004-2009 Slava Pestov |
| `bignumint.hpp` | 4 | -*-C-*- |
| `bitwise_hacks.hpp` | 2.1 | *(no header comment)* |
| `booleans.hpp` | 0.1 | Cannot allocate |
| `bump_allocator.hpp` | 0.9 | offset of 'here' and 'end' is hardcoded in compiler backends |
| `byte_arrays.cpp` | 1.9 | Allocates memory |
| `byte_arrays.hpp` | 0.7 | Allocates memory |
| `callbacks.cpp` | 3.7 | *(no header comment)* |
| `callbacks.hpp` | 1.8 | The callback heap is used to store the machine code that alien-callbacks actually jump to when C code invokes them. |
| `callstack.cpp` | 4 | Allocates memory (allot) |
| `callstack.hpp` | 3.7 | This is a little tricky. The iterator may allocate memory, so we keep the callstack in a GC root and use relative offsets Allocates memory |
| `code_blocks.cpp` | 12.4 | Cold generic word call sites point to quotations that call the inline-cache-miss and inline-cache-miss-tail primitives. |
| `code_blocks.hpp` | 3.2 | The compiled code heap is structured into blocks. |
| `code_heap.cpp` | 7.3 | *(no header comment)* |
| `code_heap.hpp` | 2.1 | The actual memory area |
| `code_roots.hpp` | 0.4 | *(no header comment)* |
| `compaction.cpp` | 5.4 | *(no header comment)* |
| `contexts.cpp` | 7.7 | *(no header comment)* |
| `contexts.hpp` | 2 | Context object count and identifiers must be kept in sync with: core/kernel/kernel.factor |
| `cpu-arm.32.hpp` | 0.1 | *(no header comment)* |
| `cpu-arm.64.cpp` | 1.2 | *(no header comment)* |
| `cpu-arm.64.hpp` | 1.2 | *(no header comment)* |
| `cpu-ppc.hpp` | 2.1 | In the instruction sequence: |
| `cpu-x86.32.hpp` | 0.3 | Must match the calculation in word jit-signal-handler-prolog in basis/bootstrap/assembler/x86.factor |
| `cpu-x86.64.hpp` | 0.2 | Must match the calculation in word jit-signal-handler-prolog in basis/bootstrap/assembler/x86.factor |
| `cpu-x86.cpp` | 3.3 | Fault came from the VM or foreign code. We don't try to fix the call stack from *sp and instead use the last saved "good value" which we get from ctx->callstack_top. Then launch the handler without... |
| `cpu-x86.hpp` | 1.9 | In the instruction sequence: |
| `data_heap.cpp` | 4.4 | *(no header comment)* |
| `data_heap.hpp` | 1.2 | Borrowed reference to a factor_vm::nursery |
| `data_heap_checker.cpp` | 2.8 | A tool to debug write barriers. Call check_data_heap() to ensure that all cards that should be marked are actually marked. |
| `data_roots.hpp` | 0.6 | *(no header comment)* |
| `debug.cpp` | 15.2 | *(no header comment)* |
| `debug.hpp` | 1.1 | To chop the directory path of the __FILE__ macro. |
| `dispatch.cpp` | 3.4 | *(no header comment)* |
| `dispatch.hpp` | 0.3 | *(no header comment)* |
| `entry_points.cpp` | 1.4 | First time this is called, wrap the c-to-factor sub-primitive inside of a callback stub, which saves and restores non-volatile registers per platform ABI conventions, so that the Factor compiler ca... |
| `errors.cpp` | 4.5 | *(no header comment)* |
| `errors.hpp` | 0.8 | Runtime errors must be kept in sync with: basis/debugger/debugger.factor core/kernel/kernel.factor |
| `factor.cpp` | 5.2 | Compile code in boot image so that we can execute the startup quotation Allocates memory |
| `factor.hpp` | 0.5 | image |
| `ffi_test.c` | 10 | C99 features |
| `ffi_test.h` | 8.5 | C99 features |
| `fixup.hpp` | 0.5 | *(no header comment)* |
| `float_bits.hpp` | 0.6 | Some functions for converting floating point numbers to binary representations and vice versa |
| `free_list.hpp` | 8.3 | *(no header comment)* |
| `full_collector.cpp` | 3.5 | *(no header comment)* |
| `gc.cpp` | 5 | *(no header comment)* |
| `gc.hpp` | 1 | These are the phases of the gc cycles we record the times of. |
| `gc_info.hpp` | 1.2 | gc_info should be kept in sync with: basis/compiler/codegen/gc-maps/gc-maps.factor basis/vm/vm.factor |
| `generic_arrays.hpp` | 1.6 | Allocates memory |
| `image.cpp` | 12.7 | *(no header comment)* |
| `image.hpp` | 1.5 | base address of data heap when image was saved |
| `inline_cache.cpp` | 7.2 | Find the call target. |
| `inline_cache.hpp` | 0.1 | *(no header comment)* |
| `instruction_operands.cpp` | 3.6 | Load a value from a bitfield of an ARM/RISC-V instruction |
| `instruction_operands.hpp` | 3.5 | arg is a literal table index, holding a pair (symbol/dll) |
| `io.cpp` | 5.6 | Simple wrappers for ANSI C I/O functions, used for bootstrapping. |
| `io.hpp` | 0.3 | Safe IO functions that does not throw Factor errors. |
| `jit.cpp` | 4 | Simple code generator used by: - quotation compiler (quotations.cpp), - megamorphic caches (dispatch.cpp), - polymorphic inline caches (inline_cache.cpp) |
| `jit.hpp` | 1.4 | *(no header comment)* |
| `layouts.hpp` | 7.9 | Must match leaf-stack-frame-size in basis/bootstrap/layouts.factor |
| `mach_signal.cpp` | 8.6 | Fault handler information.  macOS version. Copyright (C) 1993-1999, 2002-2003  Bruno Haible <clisp.org at bruno> |
| `mach_signal.hpp` | 3 | Fault handler information.  macOS version. Copyright (C) 1993-1999, 2002-2003  Bruno Haible <clisp.org at bruno> Copyright (C) 2003  Paolo Bonzini <gnu.org at bonzini> |
| `main-unix.cpp` | 0.2 | *(no header comment)* |
| `main-windows.cpp` | 0.7 | *(no header comment)* |
| `mark_bits.hpp` | 4.7 | *(no header comment)* |
| `master.hpp` | 3.6 | C headers |
| `math.cpp` | 12.7 | can't happen |
| `math.hpp` | 2.5 | Allocates memory |
| `mvm.cpp` | 0.7 | arg must be new'ed because we're going to delete it! |
| `mvm.hpp` | 0.4 | *(no header comment)* |
| `mvm-none.cpp` | 0.2 | *(no header comment)* |
| `mvm-unix.cpp` | 0.4 | *(no header comment)* |
| `mvm-windows.cpp` | 0.4 | *(no header comment)* |
| `nursery_collector.cpp` | 1.8 | The while-loop is a needed micro-optimization. |
| `object_start_map.cpp` | 2.3 | First card should start with an object |
| `object_start_map.hpp` | 0.5 | *(no header comment)* |
| `objects.cpp` | 3.4 | Size of the object pointed to by a tagged pointer |
| `objects.hpp` | 3.8 | Special object count and identifiers must be kept in sync with: core/kernel/kernel.factor basis/bootstrap/image/image.factor |
| `os-freebsd.cpp` | 0.7 | From SBCL |
| `os-freebsd.hpp` | 0.2 | *(no header comment)* |
| `os-freebsd-x86.32.hpp` | 1.2 | *(no header comment)* |
| `os-freebsd-x86.64.hpp` | 1 | *(no header comment)* |
| `os-genunix.cpp` | 0.8 | You must free() the result yourself. |
| `os-genunix.hpp` | 0.2 | *(no header comment)* |
| `os-linux.cpp` | 0.8 | readlink is called in a loop with increasing buffer sizes in case someone tries to run Factor from a incredibly deeply nested path. |
| `os-linux.hpp` | 0 | *(no header comment)* |
| `os-linux-arm.32.cpp` | 0.8 | XXX: why doesn't this work on Nokia n800? It should behave identically to the below assembly. result = syscall(__ARM_NR_cacheflush,start,start + len,0); |
| `os-linux-arm.32.hpp` | 0.5 | *(no header comment)* |
| `os-linux-arm.64.hpp` | 1.5 | *(no header comment)* |
| `os-linux-ppc.32.hpp` | 0.9 | *(no header comment)* |
| `os-linux-ppc.64.hpp` | 1.2 | *(no header comment)* |
| `os-linux-x86.32.hpp` | 1.6 | glibc lies about the contents of the fpstate the kernel provides, hiding the FXSR environment |
| `os-linux-x86.64.hpp` | 0.9 | *(no header comment)* |
| `os-macos.hpp` | 0.5 | *(no header comment)* |
| `os-macos-arm.64.hpp` | 1.8 | *(no header comment)* |
| `os-macos-x86.32.hpp` | 2.4 | Fault handler information.  macOS version. Copyright (C) 1993-1999, 2002-2003  Bruno Haible <clisp.org at bruno> Copyright (C) 2003  Paolo Bonzini <gnu.org at bonzini> |
| `os-macos-x86.64.hpp` | 2.6 | Fault handler information.  macOS version. Copyright (C) 1993-1999, 2002-2003  Bruno Haible <clisp.org at bruno> Copyright (C) 2003  Paolo Bonzini <gnu.org at bonzini> |
| `os-unix.cpp` | 14.5 | *(no header comment)* |
| `os-unix.hpp` | 1.3 | *(no header comment)* |
| `os-windows.cpp` | 12.3 | msec |
| `os-windows.hpp` | 2.8 | for cygwin |
| `os-windows-arm.64.cpp` | 0.1 | *(no header comment)* |
| `os-windows-arm.64.hpp` | 0.2 | *(no header comment)* |
| `os-windows-x86.32.cpp` | 0.2 | 32-bit Windows SEH set up in basis/bootstrap/assembler/x86.32.windows.factor |
| `os-windows-x86.32.hpp` | 1 | The ExtendedRegisters field of the x86.32 CONTEXT structure uses this layout; however, this structure is only made available from winnt.h on x86.64 |
| `os-windows-x86.64.cpp` | 2.3 | *(no header comment)* |
| `os-windows-x86.64.hpp` | 0.3 | Must match the stack-frame-size constant in basis/bootstap/assembler/x86.64.windows.factor |
| `platform.hpp` | 2.1 | *(no header comment)* |
| `primitives.cpp` | 0.4 | *(no header comment)* |
| `primitives.hpp` | 4.1 | Generated with PRIMITIVE in primitives.cpp |
| `quotations.cpp` | 12 | Simple non-optimizing compiler. |
| `quotations.hpp` | 1.4 | Allocates memory |
| `run.cpp` | 0.7 | *(no header comment)* |
| `run.hpp` | 0.1 | *(no header comment)* |
| `safepoints.cpp` | 1.9 | Ctrl-Break throws an exception, interrupting the main thread, same as the "t" command in the factorbug debugger. But for Ctrl-Break to work we don't require the debugger to be activated, or even en... |
| `sampling_profiler.cpp` | 5.8 | This is like the growable_array class, except the whole of it exists on the Factor heap. growarr = growable array. |
| `sampling_profiler.hpp` | 1.4 | Active thread during sample |
| `segments.hpp` | 1 | segments set up guard pages to check for under/overflow. size must be a multiple of the page size |
| `slot_visitor.hpp` | 19 | Size sans alignment. |
| `strings.cpp` | 3.3 | Allocates memory |
| `tagged.hpp` | 1.3 | *(no header comment)* |
| `tenured_space.hpp` | 1 | *(no header comment)* |
| `to_tenured_collector.cpp` | 0.9 | Copy live objects from aging space to tenured space. |
| `to_tenured_collector.hpp` | 0.9 | Is there another forwarding pointer? |
| `tuples.cpp` | 0.9 | push a new tuple on the stack, filling its slots with f Allocates memory |
| `utilities.cpp` | 0.9 | Fill in a PPC function descriptor |
| `utilities.hpp` | 1.4 | Poor mans range-based for loops. |
| `vm.cpp` | 1.5 | *(no header comment)* |
| `vm.hpp` | 25.4 | Id of the main thread we run in. Used for Ctrl-Break handling. |
| `words.cpp` | 2.4 | Compile a word definition with the non-optimizing compiler. Allocates memory |
| `write_barrier.hpp` | 1.1 | card marking write barrier. a card is a byte storing a mark flag, and the offset (in cells) of the first object in the card. |
| `zstd.cpp` | 0.8 | Copyright (C) 2022-2024 nomennescio See https://factorcode.org/license.txt for BSD license. |
| `zstd.hpp` | 0.4 | Copyright (C) 2022-2024 nomennescio See https://factorcode.org/license.txt for BSD license. |

## Zig VM mirror — `E:\factor-src\src\\`

A Zig reimplementation of the same VM.  Linux/macOS-tuned; needs Windows porting work.
Layouts comptime-asserted to match C++ structs, so this is also a useful cross-check.

| File | KB | Top-of-file blurb |
|------|---:|-------------------|
| `bignum.zig` | 82.4 | bignum.zig - Arbitrary precision integers integrated with the Factor VM. Contains shared representation/helpers and VM-facing arithmetic/allocation. |
| `bump_allocator.zig` | 2.8 | bump_allocator.zig - Simple bump allocator for nursery offset of 'here' and 'end' is hardcoded in compiler backends |
| `c_api.zig` | 24.3 | c_api.zig - C API functions exported for Factor compiled code These functions are called by Factor code via dlsym relocations  Factor's compiled code expects these functions to be available at runt... |
| `callbacks.zig` | 8.8 | *(no header comment)* |
| `callstack.zig` | 7.7 | *(no header comment)* |
| `callstack_lookup.zig` | 3.6 | callstack_lookup.zig - Shared fast lookup helpers for callstack walking  Centralizes code-block owner and callsite lookup caches used by GC phases. |
| `card_scan.zig` | 18 | card_scan.zig - Card scanning for generational GC Scans dirty cards in tenured/aging space to find cross-generational references. Also handles code heap root scanning, large object scanning, and ag... |
| `code_blocks.zig` | 36.8 | *(no header comment)* |
| `code_heap.zig` | 20.3 | code_heap.zig - Code heap management Manages the JIT-compiled code heap: allocation, block tracking, remembered sets for GC, scan flags, and mark bits. |
| `compact.zig` | 31.7 | compact.zig - Compaction phase for Factor VM garbage collector Handles mark-compact GC: moving marked objects, fixing up pointers, updating callstacks, code blocks, and instruction operands.  Extra... |
| `contexts.zig` | 8.9 | Stack reserved space for overflow handling When the callstack fills up, we chop off this many bytes to have space to work with macOS 64 bit needs more than 8192. See issue #1419. |
| `cpu.zig` | 2.2 | / True for any x86 family (32-bit or 64-bit). |
| `data_heap.zig` | 14.6 | *(no header comment)* |
| `debugger.zig` | 43 | debugger.zig - Low-level debugger (Factor Error Protocol / factorbug) Extracted from vm.zig. Provides interactive debugging, stack printing, heap walking, and object inspection. |
| `execution.zig` | 16.5 | *(no header comment)* |
| `fixnum.zig` | 6.3 | Maximum fixnum value (signed) |
| `float.zig` | 0.9 | float.zig - Boxed float helpers for the Factor VM. |
| `free_list.zig` | 17.9 | Free list allocator constants |
| `gc.zig` | 44.1 | *(no header comment)* |
| `growable.zig` | 9.9 | / Growable array for building arrays incrementally |
| `icache.zig` | 4 | icache.zig - Instruction cache flush functionality  ARM platforms require explicit instruction cache flush after modifying code. x86/x86-64 has coherent instruction caches, so no flush is needed.  ... |
| `image.zig` | 58.5 | image.zig - Factor boot image loading and saving Image format documentation |
| `inline_cache.zig` | 15.9 | PIC type - determined by the types of objects being dispatched on |
| `io.zig` | 8.4 | C library function declarations |
| `jit.zig` | 56 | JIT template indices (stored in special_objects) |
| `layouts.zig` | 19.6 | layouts.zig - Core type definitions for Factor VM |
| `mach_signal.zig` | 24.9 | Only compile this module on macOS |
| `main.zig` | 23.3 | main.zig - Factor VM entry point |
| `mark.zig` | 26.9 | mark.zig - Mark phase for Factor VM garbage collector Handles both the full mark phase (mark+copy from nursery/aging to tenured) and the simple mark phase (mark-only after collectToTenured).  Extra... |
| `mark_bits.zig` | 13.9 | Mark bits are stored at data_alignment (16-byte) granularity Each bit represents whether a 16-byte aligned block is part of a live object |
| `mutex.zig` | 0.4 | *(no header comment)* |
| `object_start_map.zig` | 13.1 | Map stores offsets at card granularity Each entry is the offset (in bytes) from the card start to the object start A value of 0xFF means "look at the previous card" |
| `objects.zig` | 4 | objects.zig - Special object indices |
| `primitives.zig` | 28.9 | primitives.zig - Factor VM primitives dispatch hub Imports all primitive sub-modules and wires up the dispatch table. |
| `safepoints.zig` | 21.1 | Atomic flags for safepoint conditions These are checked in handle_safepoint() |
| `segments.zig` | 5.8 | segments.zig - Memory segment management |
| `signals.zig` | 39.5 | signals.zig - Signal handling infrastructure for Factor VM Implements Unix signal handlers for SIGSEGV, SIGBUS, SIGFPE, SIGINT, etc. |
| `slot_visitor.zig` | 9.9 | *(no header comment)* |
| `spill_slots.zig` | 1.6 | spill_slots.zig - Shared spill-slot traversal for callstack GC roots  Handles the derived-pointer protocol: 1) subtract base pointers from derived pointers 2) visit GC roots selected by the callsit... |
| `sweep.zig` | 10.1 | sweep.zig - Sweep phase for full GC Extracted from gc.zig. Walk tenured space, find unmarked regions, and rebuild the free list. Also handles code heap sweeping and card/deck clearing. |
| `trampolines.zig` | 0.5 | ARM64 relocation types only; panic on non-ARM64 targets or invocation. |
| `vm.zig` | 43.5 | *(no header comment)* |
| `write_barrier.zig` | 13.5 | Convert address to card index |
