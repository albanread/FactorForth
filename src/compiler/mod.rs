//! Compiler — the ANS Forth front end.
//!
//! Stages, in order:
//!
//!   1. **Lex** (`lex.rs`) — source text → token stream.
//!   2. **Parse** (`parse.rs`, Phase 2.2) — token stream → AST.
//!   3. **Resolve** (`resolve.rs`, Phase 2.3) — AST + dictionary →
//!      annotated AST with Factor target names.
//!   4. **Effect** (`effect.rs`, Phase 2.7) — stack-effect inference
//!      and checking.
//!   5. **Emit** (`emit.rs`, Phase 2.3+) — annotated AST → canonical
//!      Factor source (IR).
//!
//! The IR is fed to the embedded Factor VM via `session::eval` (Phase 3)
//! which calls into the patched factor.dll's `nf_eval_string`.
//!
//! Errors flow through `error.rs` with source positions; users see ANS-
//! style messages, never Factor frames.

pub mod ast;
pub mod dump;
pub mod effect;
pub mod emit;
pub mod error;
pub mod lex;
pub mod parse;
pub mod resolve;
pub mod sema;

pub use ast::{CaseArm, Definition, Expr, Item, Literal, LoopKind, Program, StackEffect};
pub use effect::{infer, Effect, EffectError, Inferred};
pub use emit::{emit, EmitOpts};
pub use error::{CompileError, Pos, Span};
pub use lex::{lex, NumBase, StringKind, Tok, Token};
pub use parse::{parse, ParseError};
pub use resolve::{resolve, ResolveError, Resolved, Target};
pub use sema::{build as build_sema, EscapeReason, EscapeState, Sema, UserWord};

/// Top-level convenience: ANS Forth source string → Factor IR string.
///
/// Pipeline: lex → parse → resolve → effect-check → emit.  Errors
/// stringify via each stage's `Display`.  Returns the IR ready to
/// feed to `nf_eval_string`.
///
/// Effect mismatches (M2.7) are hard errors here — a declared
/// `( a -- b )` that doesn't match the body's behaviour stops the
/// compile.  Bodies containing control flow currently yield an
/// `Effect::Unknown` for which no check is performed; the declared
/// annotation is trusted for caller-side typing.
///
/// This is the simplest possible driver — Phase 3 will wrap it in
/// a `Session` that owns the embedded VM and supports incremental
/// `compile_and_eval` calls with carry-over state.
pub fn compile(source: &str) -> Result<String, String> {
    let toks = lex(source).map_err(|e| e.to_string())?;
    let prog = parse(&toks).map_err(|e| e.to_string())?;
    let sema = build_sema(prog).map_err(|e| e.to_string())?;
    if let Some(first) = sema.effect_errors.first() {
        // Report the first; subsequent ones often cascade from the
        // first and would distract the user.  Future work: collect
        // up to N and present a summary.
        return Err(first.to_string());
    }
    Ok(emit(&sema, &EmitOpts::default()))
}
