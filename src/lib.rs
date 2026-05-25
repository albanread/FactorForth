//! NewFactor — a Rust ANS Forth compiler targeting Factor's VM.
//!
//! See `MANIFESTO.md` for the architectural intent and `PLAN.md` for the
//! phase-by-phase delivery schedule.  This is the library crate root; it
//! re-exports the compiler pipeline and the embedded VM session abstraction
//! for use by the `newfactor` CLI binary and the `newfactor-ui` GUI binary.
//!
//! Public surface, in stages:
//!
//! * Phase 0 (now): empty stubs; verifies the project builds against `wf64`
//!   and the Win32 dependencies.
//! * Phase 1: `session` module (embedded Factor VM lifecycle).
//! * Phase 2: `compiler` module (ANS Forth → canonical Factor IR).
//! * Phase 3+: `runtime` module (host extern "C" callbacks for iGui via wf64).//!
//! ## Articles
//! 
//! Deep dives into NewFactor's architecture and design variations:
//! * [The CREATE/DOES> Implementation](docs/articles/create_does.md)
//! * [Floating Point and the Unified Stack](docs/articles/floating_point.md)
//! * [Managed Strings and the `$str` Vocab](docs/articles/new_strings.md)
//! * [Synthesized Stack Effect Inference](docs/articles/effect_inference.md)
//! * [The LET DSL for Infix Algebra](docs/articles/let_dsl.md)
//! * [Crash Recovery & Error Translation](docs/articles/crash_recovery.md)
//! * [Embedding the Factor VM in Rust](docs/articles/embedding_factor.md)
//!
//! ## Supported ANS Forth Words
//! 
//! The following words are fully implemented and reachable from user code:
//! 
//! * **Stack:** `DUP` `DROP` `SWAP` `OVER` `ROT`
//! * **Arithmetic:** `+` `-` `*` `/` `MOD` `NEGATE` `ABS` `MIN` `MAX`
//! * **Comparison:** `=` `<>` `<` `>` `0=` `0<` `0>`
//! * **Logic:** `AND` `OR` `XOR` `INVERT`
//! * **Memory:** `@` `!` `+!` `C@` `C!` `CELL+` `CELLS` `CHAR+` `CHARS`
//! * **Definitions:** `:` `;` `VARIABLE` `CONSTANT` `FCONSTANT` `CREATE` `DOES>`
//! * **Control Flow:** `IF` `ELSE` `THEN` `BEGIN` `UNTIL` `WHILE` `REPEAT` `AGAIN` `DO` `?DO` `LOOP` `+LOOP` `LEAVE` `UNLOOP` `CASE` `OF` `ENDOF` `ENDCASE`
//! * **I/O:** `.` `CR` `EMIT` `SPACE` `SPACES` `TYPE` `KEY` `ACCEPT`
//! * **Formatting:** `<#` `#` `#S` `SIGN` `HOLD` `#>`
//! * **Strings/Buffers:** `S" ..."` `." ..."` `CMOVE` `FILL` `BL`
//! * **Radix:** `HEX` `DECIMAL` `OCTAL` `BINARY`
//! * **Float:** `F@` `F!` `FCONSTANT`
#![doc(html_no_source)]

// Modules added in order as phases land.
pub mod compiler;
#[cfg(target_os = "windows")]
pub mod session;
// pub mod runtime;   // Phase 3
// pub mod ffi;       // Phase 3

/// Crate identifier — Phase 0 placeholder so `cargo check` has something
/// concrete to compile.  Replaced by real surface in Phase 1.
pub const NAME: &str = "newfactor";

#[cfg(test)]
mod smoke {
    #[test]
    fn crate_name_is_set() {
        assert_eq!(super::NAME, "newfactor");
    }
}
