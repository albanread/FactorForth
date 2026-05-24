//! `newfactor-ui` — the GUI binary.
//!
//! Reuses `wf64::igui` (Direct2D + DirectWrite MDI front-end) verbatim via
//! a path dependency on the WF64 project.  The session backend is the
//! embedded Factor VM driving an ANS Forth front-end from this crate.
//!
//! Phase 0 placeholder: prints a banner and exits.  Real implementation
//! lands in Phase 3 once the compiler and session modules exist.

fn main() {
    println!(
        "{} {} — Phase 0 placeholder.",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    println!("GUI integration lands in Phase 3.  See PLAN.md.");
}
