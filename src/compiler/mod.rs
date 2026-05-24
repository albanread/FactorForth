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

pub use ast::{
    CaseArm, CollectionDef, CollectionKind, ConstFlavour, ConstValue,
    ConstantDef, CreateDef, Definition, Expr, Item, Literal, LoopKind,
    Program, StackEffect, VariableDef,
};
pub use effect::{infer, Effect, EffectError, Inferred};
pub use emit::{emit, EmitOpts};
pub use error::{CompileError, Pos, Span};
pub use lex::{lex, NumBase, StringKind, Tok, Token};
pub use parse::{parse, ParseError};
pub use resolve::{resolve, ResolveError, Resolved, Target};
pub use sema::{build as build_sema, EscapeReason, EscapeState, Sema, UserWord};

/// Top-level convenience: ANS Forth source string → Factor IR string.
///
/// Pipeline: lex → parse → sema (resolve + effect + escape) → emit.
/// Errors stringify via each stage's `Display`.  Returns the IR
/// ready to feed to `nf_eval_string`.
///
/// **Effect diagnostics are warnings, not errors.**  A declared
/// `( a -- b )` annotation that doesn't match the body's inferred
/// behaviour produces a `Mismatch` in `sema.effect_errors` but the
/// compile still emits valid IR — with the synthesised effect,
/// which is correct by construction.  Callers (the CLI, the
/// future IDE) decide whether to surface, ignore, or escalate.
/// This matches Forth's permissive culture: the user can write
/// programs whose effects are ambiguous on purpose; we tell them
/// what we see and let them proceed.
///
/// Use [`compile_with_diagnostics`] when you want to see the
/// warnings programmatically.  This function discards them.
///
/// This is the simplest possible driver — Phase 3 will wrap it in
/// a `Session` that owns the embedded VM and supports incremental
/// `compile_and_eval` calls with carry-over state.
pub fn compile(source: &str) -> Result<String, String> {
    compile_with_diagnostics(source).map(|(ir, _warnings)| ir)
}

/// Compile and return the IR plus any effect-diagnostic warnings.
/// Stage errors (lex, parse, resolve) are still hard failures —
/// they prevent us from producing any IR at all.  Effect issues
/// are not: the synth produces correct IR regardless.
pub fn compile_with_diagnostics(
    source: &str,
) -> Result<(String, Vec<EffectError>), String> {
    let toks = lex(source).map_err(|e| e.to_string())?;
    let prog = parse(&toks).map_err(|e| e.to_string())?;
    let sema = build_sema(prog).map_err(|e| e.to_string())?;
    let warnings = sema.effect_errors.clone();
    let ir = emit(&sema, &EmitOpts::default());
    Ok((ir, warnings))
}
