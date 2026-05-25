//! lower_recurse — pre-resolve AST pass that binds ANS `RECURSE` to
//! the enclosing `:` definition's own name.
//!
//! ## Why a desugar (and not a runtime word)
//!
//! In a classic threaded Forth, `RECURSE` is an immediate word
//! because the dictionary entry for the word being compiled doesn't
//! become visible until `;` runs.  NewFactor parses and resolves
//! definitions whole, so by the time the resolver walks a body it
//! could trivially know the enclosing word's name — *if* it were
//! told.  This pass tells it: every `WordRef { name: "recurse" }`
//! inside a `Definition` body is rewritten to a `WordRef` pointing
//! at the definition's own name.  The resolver then handles it as
//! an ordinary self-call (which works because pass-1 has already
//! registered the def's name in `user_words`).
//!
//! ## Stack-effect requirement
//!
//! ANS doesn't formally require it, but in practice every recursive
//! Forth word carries a `( a -- b )` annotation, and Factor's strict
//! effect inference *needs* one: without it the synth path falls
//! back to row-variables (`( ..a -- ..b )`) and Factor's compiler
//! rejects the IR.  We turn that into an upfront, clear sema-level
//! error so the user gets "RECURSE requires a stack-effect
//! annotation" instead of an inscrutable Factor message.
//!
//! ## TCO interaction with EXIT
//!
//! Factor's JIT performs Tail Call Optimisation on recursive calls
//! at tail position — *unless* the word is wrapped in
//! `continuations:with-return`, which forces the call frame to
//! survive for potential non-local-exit unwinding.  `lower_exit`
//! (task #54) removed the wrap for the common cases of EXIT, so
//! RECURSE without EXIT-inside-a-loop gets full TCO today.  RECURSE
//! coexisting with EXIT inside a DO/LOOP still falls back to
//! with-return (TCO disabled, stack overflow on deep recursion);
//! task #55 lifts that restriction by rewriting the loop into a
//! tail-recursive helper word.

use super::ast::{CaseArm, Definition, Expr, Item, Program};
use super::error::Span;

/// One source location where a recursive word lacked the required
/// stack-effect annotation.  Hoisted into `Program`'s caller (sema)
/// so it can surface alongside other compile errors.
#[derive(Clone, Debug)]
pub struct MissingRecurseEffect {
    pub word_name: String,
    pub at: Span,
}

impl std::fmt::Display for MissingRecurseEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RECURSE requires a stack-effect annotation on `{}` — \
                   add `( ... -- ... )` after the name", self.word_name)
    }
}

/// Run the desugar over a whole `Program`.  Returns the rewritten
/// program plus a list of any definitions that used RECURSE without
/// a declared stack effect (sema should escalate these to errors).
pub fn lower_program(mut prog: Program) -> (Program, Vec<MissingRecurseEffect>) {
    let mut missing = Vec::new();
    for item in prog.items.iter_mut() {
        if let Item::Definition(d) = item {
            let saw_recurse = rewrite_def(d);
            if saw_recurse && d.effect.is_none() {
                missing.push(MissingRecurseEffect {
                    word_name: d.name.clone(),
                    at: d.name_span,
                });
            }
        }
    }
    (prog, missing)
}

/// Rewrite every `Expr::WordRef { name: "recurse" }` inside `d.body`
/// to a WordRef pointing at `d.name`.  Returns true if at least one
/// such rewrite happened (so the caller can check for the missing-
/// effect-annotation condition).
fn rewrite_def(d: &mut Definition) -> bool {
    let target_name = d.name.clone();
    let mut saw = false;
    d.body = rewrite_seq(&d.body, &target_name, &mut saw);
    saw
}

fn rewrite_seq(seq: &[Expr], target: &str, saw: &mut bool) -> Vec<Expr> {
    seq.iter().map(|e| rewrite_expr(e, target, saw)).collect()
}

fn rewrite_expr(e: &Expr, target: &str, saw: &mut bool) -> Expr {
    match e {
        Expr::WordRef { name, span } if name.eq_ignore_ascii_case("recurse") => {
            *saw = true;
            Expr::WordRef {
                name: target.to_string(),
                span: *span,
            }
        }
        Expr::If { then_body, else_body, span } => Expr::If {
            then_body: rewrite_seq(then_body, target, saw),
            else_body: else_body.as_ref().map(|b| rewrite_seq(b, target, saw)),
            span: *span,
        },
        Expr::BeginUntil { body, span } => Expr::BeginUntil {
            body: rewrite_seq(body, target, saw),
            span: *span,
        },
        Expr::BeginAgain { body, span } => Expr::BeginAgain {
            body: rewrite_seq(body, target, saw),
            span: *span,
        },
        Expr::BeginWhileRepeat { pred, body, span } => Expr::BeginWhileRepeat {
            pred: rewrite_seq(pred, target, saw),
            body: rewrite_seq(body, target, saw),
            span: *span,
        },
        Expr::DoLoop { is_qdo, body, loop_kind, span } => Expr::DoLoop {
            is_qdo: *is_qdo,
            body: rewrite_seq(body, target, saw),
            loop_kind: *loop_kind,
            span: *span,
        },
        Expr::Case { arms, default, span } => Expr::Case {
            arms: arms.iter().map(|a| CaseArm {
                match_expr: rewrite_seq(&a.match_expr, target, saw),
                body:       rewrite_seq(&a.body,       target, saw),
                span: a.span,
            }).collect(),
            default: default.as_ref().map(|d| rewrite_seq(d, target, saw)),
            span: *span,
        },
        // Non-control-flow nodes pass through unchanged.
        _ => e.clone(),
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{lex, parse};

    fn lower(src: &str) -> (Program, Vec<MissingRecurseEffect>) {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        lower_program(prog)
    }

    #[test]
    fn recurse_rebound_to_def_name() {
        let (prog, missing) = lower(": fact ( n -- f ) dup 1 < if drop 1 else dup 1 - recurse * then ;");
        assert!(missing.is_empty(), "expected no missing-effect errors: {missing:?}");
        let body = prog.items.iter().find_map(|it| match it {
            Item::Definition(d) => Some(&d.body),
            _ => None,
        }).unwrap();
        // Walk the body and find any WordRef named "recurse" — should be none.
        fn finds_recurse(seq: &[Expr]) -> bool {
            seq.iter().any(|e| match e {
                Expr::WordRef { name, .. } => name.eq_ignore_ascii_case("recurse"),
                Expr::If { then_body, else_body, .. } => {
                    finds_recurse(then_body)
                    || else_body.as_ref().is_some_and(|b| finds_recurse(b))
                }
                _ => false,
            })
        }
        assert!(!finds_recurse(body), "recurse should be rewritten away: {body:?}");
        // And there should be a WordRef named "fact" in the recursive position.
        fn finds(seq: &[Expr], target: &str) -> bool {
            seq.iter().any(|e| match e {
                Expr::WordRef { name, .. } => name == target,
                Expr::If { then_body, else_body, .. } => {
                    finds(then_body, target)
                    || else_body.as_ref().is_some_and(|b| finds(b, target))
                }
                _ => false,
            })
        }
        assert!(finds(body, "fact"), "self-call should target 'fact': {body:?}");
    }

    #[test]
    fn recurse_without_annotation_flagged() {
        let (_prog, missing) = lower(": fact dup 1 < if drop 1 else dup 1 - recurse * then ;");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].word_name, "fact");
    }

    #[test]
    fn non_recursive_def_unchanged() {
        let (_prog, missing) = lower(": square dup * ;");
        assert!(missing.is_empty());
    }
}
