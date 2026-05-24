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
//! * Phase 3+: `runtime` module (host extern "C" callbacks for iGui via wf64).

#![doc(html_no_source)]

// Modules added in order as phases land.
pub mod compiler;
// pub mod session;   // Phase 3
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
