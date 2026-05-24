//! `newfactor` — headless CLI for running ANS Forth files through the
//! embedded Factor VM.
//!
//! Phase 0: prints a banner and exits.  Real CLI lands in Phase 2 once the
//! compiler exists and Phase 3 once the embedded VM session is wired.
//!
//! Intended usage (post-Phase-3):
//!
//! ```text
//!   newfactor demos/gcd.f
//!   newfactor --eval "2 3 + ."
//!   newfactor --image images/nf-mandelbrot.image demos/gfx-mandelbrot.f
//! ```
//!
//! For the GUI version with iGui, see the `newfactor-ui` binary.

fn main() {
    println!("{} {} — Phase 0 placeholder.", newfactor::NAME, env!("CARGO_PKG_VERSION"));
    println!("Real CLI lands in Phase 2.  See PLAN.md.");
}
