# Crash Recovery and Error Translation

A classic Forth system operates largely without a net. A stack underflow, an invalid memory access, or an unhandled exception often results in a complete program abort—or worse, a silent memory corruption. `ABORT` and `QUIT` provide some top-level recovery, but they are relatively blunt instruments.

NewFactor takes a fundamentally different approach. Because the NewFactor IDE and tools rely on a persistent, embedded worker process, the system is designed to survive interactive errors and isolate failures. 

This resilience is achieved through a multi-tiered crash and error recovery architecture.

## Tier 1: The `recover` Sandbox

In NewFactor, every interactive evaluation (from the REPL or the IDE) is wrapped in Factor's robust exception handling primitives. Specifically, `eval` invocations operate within a `recover` quotation sandbox.

When you type something invalid—like calling a method on the wrong type (`42 $len`) or attempting an out-of-bounds array access (`100 xs @`)—the NewFactor runtime catches the tuple or generic-dispatch error. 

Instead of failing the worker thread:
1. The error message is captured via a bound `error-stream`.
2. The runtime invokes `nf-format-error` (or logs it via `eval>string`).
3. The evaluation session resets safely, and the process remains alive.

**Test Guarantee:** A core validation test (`err_does_not_kill_session`) explicitly triggers a no-method crash, then evaluates `21 21 + .` to ensure the session survived and correctly outputs `42`.

## Tier 2: Error Translation (WIP)

Legacy Forth code expects standardized `THROW` codes (-1 for ABORT, -4 for stack underflow, etc.) and concise single-line error messages. Factor, conversely, provides extremely detailed, multi-line stack traces full of internal VM types that can be intimidating to a classic Forth developer.

To bridge this, NewFactor includes a translation layer (`nf-format-error`). This layer catches detailed Factor VM exceptions and transforms them:
*   `{ KERNEL-ERROR n ... }` structures are mapped to ANS integer codes.
*   Tuple-based errors (`bounds-error`, `no-method`) are caught and formatted as succinct, single-line messages.

*Note: As of M2.11, while the translation layer code is present in the `forth.runtime` vocabulary, routing these formatted exceptions cleanly over the C ABI boundary (`alien-callback`) back to Rust is actively being refined to prevent `check-datastack` assertion crashes.*

## Tier 3: Hardware Trap Isolation

Some errors cannot be caught at the language level because they are trapped directly by the host CPU and OS. For example:
*   **Divide-by-zero:** `1 0 /`
*   **Severe Underflow:** Evaluating `drop` on a completely empty stack (reading off-the-end memory).

In standard Factor, `params_set_signals` translates OS signals (`STATUS_INTEGER_DIVIDE_BY_ZERO` on Windows) back into standard language exceptions. Because NewFactor explicitly embeds the VM (`nf_init_factor`) and bypasses normal stand-alone `factor.exe` signal handler (SEH) installations, these instructions currently trigger OS-level process deaths.

While these are still rough edges under active mitigation in the VM's `factor.cpp` setup, the philosophy is to ultimately wrap or trap these edge cases so that even hardware faults surface cleanly into Tier 1 error states.

### Summary

NewFactor aims for **zero fatal crashes**. While you can still write code that fails mathematically or logically, the goal is for the environment to catch, translate, and explain the failure securely—without ever dropping you to the desktop.