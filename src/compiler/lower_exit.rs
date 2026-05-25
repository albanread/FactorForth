//! lower_exit — AST pre-emit pass that rewrites ANS Forth EXIT into
//! structured tail-inlining, so emit.rs can avoid wrapping definition
//! bodies in `[ ... ] continuations:with-return` (which lowers to
//! Factor's `callcc0`) for the common case.
//!
//! ## Why this matters
//!
//! Factor's optimising compiler (`compiler.tree`) treats `callcc0` as
//! a non-local-escape value: its escape-analysis pass flags any
//! quotation passed to `with-return` as "captured", which forces the
//! body off the fast inline-cache + SSA path and into the slower
//! `compiler.cfg`-only path with re-boxed float locals.  EXIT-using
//! definitions written the natural ANS way (`A IF B EXIT THEN C`)
//! pay an order-of-magnitude perf cliff for no semantic reason.
//!
//! ## What the transform does
//!
//! We rewrite the body at the AST level so EXIT disappears entirely
//! from the IR fed to Factor — when it would have fired, the rest of
//! the definition is already structurally absent from that branch.
//!
//! The trick is "tail-inlining": at every IF whose then- or else-arm
//! contains EXIT, we append the post-IF tail of the enclosing
//! sequence into both branches before recursively lowering them.
//! Inside each lowered branch, EXIT is now followed by no code at
//! all (or by code that itself terminates), so we can dead-strip it.
//!
//! Worked example:
//!
//! ```forth
//! : f  A IF B EXIT THEN C ;
//! ```
//!
//! AST body: `[A, IF{then=[B,EXIT]}, C]`.
//!
//! Tail-inline at the IF (post-IF tail is `[C]`):
//!   - then-with-tail: `[B, EXIT, C]`  →  lower  →  `[B]` (EXIT dead-strips C)
//!   - else-with-tail: `[] + [C]` = `[C]`
//!
//! Result: `[A, IF{then=[B], else=Some([C])}]`.  No EXIT.  Emitted
//! Factor: `A [ B ] [ C ] kernel:if` — pure structured control flow,
//! full JIT treatment.
//!
//! ## Scope of correctness
//!
//! Tail-inlining is sound for any EXIT whose enclosing context's
//! "tail" is part of the definition body that we can see and rewrite.
//! That covers:
//!
//!   * EXIT at top level of a `: ... ;` body
//!   * EXIT inside an IF/ELSE branch
//!   * EXIT inside a CASE/OF arm or default
//!   * Arbitrary nesting of the above
//!
//! It is **not** sound across a loop iteration boundary: EXIT inside
//! `DO/LOOP`, `BEGIN/UNTIL`, `BEGIN/WHILE/REPEAT`, or `BEGIN/AGAIN`
//! must break the loop, which a structural rewrite can't express
//! without a recursive call form (Rec 2, future work).  This pass
//! leaves loop bodies untouched; the surviving EXIT keeps the
//! `with-return` wrap as a correctness fallback, paying the JIT
//! slow-path cost only for that single definition.
//!
//! See task #54 for the broader context.

use std::collections::HashMap;

use super::ast::{CaseArm, Expr};
use super::error::Span;
use super::resolve::Target;

/// Lower a definition body.  Returns the rewritten body.  Code paths
/// that unconditionally EXIT no longer contain the EXIT word
/// reference; instead, whatever would have run after them is
/// dead-stripped from those paths.
///
/// Any EXIT that survives the transform is one that's inside a loop
/// body (we leave loops opaque) — emit.rs's `body_uses_exit` will
/// detect that case and wrap with continuations:with-return.
///
/// `word_targets` is the resolver's per-span target map — we look up
/// each `WordRef`'s span there to identify which references resolve
/// to `continuations:return` (the resolver's spelling of ANS EXIT).
pub fn lower_body(body: &[Expr], word_targets: &HashMap<Span, Target>) -> Vec<Expr> {
    let (lowered, _terminates) = lower_seq(body, word_targets);
    lowered
}

/// Recursive workhorse.  Walks `seq` and returns:
///   * the rewritten sequence
///   * `terminates` = `true` iff every path through `seq`
///     unconditionally EXITs (so callers can dead-strip anything
///     they would otherwise have appended).
fn lower_seq(seq: &[Expr], r: &HashMap<Span, Target>) -> (Vec<Expr>, bool) {
    let mut out = Vec::with_capacity(seq.len());
    for (i, e) in seq.iter().enumerate() {
        if is_exit_word(e, r) {
            // Top-level EXIT in this sequence.  Drop everything we
            // would have appended after it; tell the caller this
            // path terminates so they can do the same.
            return (out, true);
        }
        match e {
            Expr::If { then_body, else_body, span } => {
                let any_exit = body_uses_exit(then_body, r)
                    || else_body.as_deref().is_some_and(|b| body_uses_exit(b, r));
                if !any_exit {
                    // No EXIT under this IF.  Recurse so nested
                    // IFs further down still get lowered, but don't
                    // tail-inline (keeps IR compact).
                    let (then_l, _) = lower_seq(then_body, r);
                    let else_l = else_body.as_ref().map(|b| lower_seq(b, r).0);
                    out.push(Expr::If {
                        then_body: then_l,
                        else_body: else_l,
                        span: *span,
                    });
                    continue;
                }
                // At least one branch contains EXIT.  Tail-inline.
                let tail = &seq[i + 1..];
                let mut then_seq = then_body.clone();
                then_seq.extend_from_slice(tail);
                let (then_l, then_term) = lower_seq(&then_seq, r);
                let mut else_seq = else_body.clone().unwrap_or_default();
                else_seq.extend_from_slice(tail);
                let (else_l, else_term) = lower_seq(&else_seq, r);
                out.push(Expr::If {
                    then_body: then_l,
                    else_body: Some(else_l),
                    span: *span,
                });
                // Tail has been consumed by the branches; this whole
                // construct's termination is "both arms terminated".
                return (out, then_term && else_term);
            }
            Expr::Case { arms, default, span } => {
                let any_exit = arms.iter().any(|a| body_uses_exit(&a.body, r))
                    || default.as_deref().is_some_and(|d| body_uses_exit(d, r));
                if !any_exit {
                    let new_arms: Vec<_> = arms.iter().map(|a| CaseArm {
                        match_expr: a.match_expr.clone(),
                        body: lower_seq(&a.body, r).0,
                        span: a.span,
                    }).collect();
                    let new_default = default.as_ref().map(|d| lower_seq(d, r).0);
                    out.push(Expr::Case {
                        arms: new_arms,
                        default: new_default,
                        span: *span,
                    });
                    continue;
                }
                // Tail-inline the post-CASE tail into every arm and
                // (if present) the default.  When source had no
                // default we synthesise one from the tail alone —
                // this preserves "no match falls through to whatever
                // followed the case" semantics.
                let tail = &seq[i + 1..];
                let mut new_arms = Vec::with_capacity(arms.len());
                let mut all_terminate = true;
                for arm in arms {
                    let mut arm_seq = arm.body.clone();
                    arm_seq.extend_from_slice(tail);
                    let (arm_l, arm_term) = lower_seq(&arm_seq, r);
                    if !arm_term { all_terminate = false; }
                    new_arms.push(CaseArm {
                        match_expr: arm.match_expr.clone(),
                        body: arm_l,
                        span: arm.span,
                    });
                }
                let mut default_seq = default.clone().unwrap_or_default();
                default_seq.extend_from_slice(tail);
                let (default_l, default_term) = if default_seq.is_empty() {
                    // No source default and no tail to inline — leave
                    // CASE without a default.  In this case `all_terminate`
                    // cannot be claimed (the no-match path runs no code,
                    // so it doesn't EXIT either).
                    (Vec::new(), false)
                } else {
                    lower_seq(&default_seq, r)
                };
                if !default_term { all_terminate = false; }
                out.push(Expr::Case {
                    arms: new_arms,
                    default: if default_l.is_empty() { None } else { Some(default_l) },
                    span: *span,
                });
                return (out, all_terminate);
            }
            // Loops are opaque to lower_exit.  Tail-inlining across
            // an iteration boundary would change semantics ("EXIT
            // breaks the loop and the word" becomes "EXIT skips
            // the rest of one iteration").  Leave the body alone;
            // if it contains EXIT, emit.rs will keep its
            // with-return wrap as a correctness fallback.
            Expr::BeginUntil { .. }
            | Expr::BeginAgain { .. }
            | Expr::BeginWhileRepeat { .. }
            | Expr::DoLoop { .. } => {
                out.push(e.clone());
            }
            _ => out.push(e.clone()),
        }
    }
    (out, false)
}

fn is_exit_word(e: &Expr, r: &HashMap<Span, Target>) -> bool {
    if let Expr::WordRef { span, .. } = e {
        if let Some(t) = r.get(span) {
            return matches!(t, Target::QualifiedBuiltin {
                vocab: "continuations", factor_name: "return" });
        }
    }
    false
}

/// True when any reference inside `body` resolves to Factor's
/// `continuations:return` — the resolver's lowering of ANS EXIT.
/// Mirrors emit.rs::body_uses_exit; kept local so this module is
/// self-contained.
pub fn body_uses_exit(body: &[Expr], r: &HashMap<Span, Target>) -> bool {
    body.iter().any(|e| expr_uses_exit(e, r))
}

fn expr_uses_exit(e: &Expr, r: &HashMap<Span, Target>) -> bool {
    if is_exit_word(e, r) { return true; }
    match e {
        Expr::If { then_body, else_body, .. } => {
            body_uses_exit(then_body, r) ||
            else_body.as_deref().is_some_and(|b| body_uses_exit(b, r))
        }
        Expr::BeginUntil    { body, .. } => body_uses_exit(body, r),
        Expr::BeginAgain    { body, .. } => body_uses_exit(body, r),
        Expr::BeginWhileRepeat { pred, body, .. } => body_uses_exit(pred, r) || body_uses_exit(body, r),
        Expr::DoLoop        { body, .. } => body_uses_exit(body, r),
        Expr::Case { arms, default, .. } => {
            arms.iter().any(|a| body_uses_exit(&a.body, r)) ||
            default.as_deref().is_some_and(|d| body_uses_exit(d, r))
        }
        _ => false,
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{lex, parse};

    /// Lower a single `:` definition's body and return the lowered
    /// body together with the original word_targets (so callers can
    /// inspect EXIT presence on the result).
    ///
    /// `build_sema` already runs `lower_body` on every definition,
    /// so we rebuild from the AST without sema and lower manually
    /// to test the transform in isolation.
    fn lower_one_def(src: &str) -> (Vec<Expr>, HashMap<Span, Target>) {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        let resolved = crate::compiler::resolve::resolve(prog).unwrap();
        let def = resolved.program.items.iter().find_map(|it| match it {
            crate::compiler::ast::Item::Definition(d) => Some(d.clone()),
            _ => None,
        }).expect("test source must contain a `:` definition");
        let lowered = lower_body(&def.body, &resolved.word_targets);
        (lowered, resolved.word_targets)
    }

    /// `EXIT` at top of body → tail dead-stripped.
    #[test]
    fn top_level_exit_drops_tail() {
        let (lowered, targets) = lower_one_def(": w 42 exit 99 ;");
        assert_eq!(lowered.len(), 1, "expected single literal after EXIT strip: {lowered:?}");
        assert!(!body_uses_exit(&lowered, &targets));
    }

    /// `IF B EXIT THEN C` → `IF B ELSE C THEN`, no EXIT survives.
    #[test]
    fn if_with_exit_tail_inlines_into_else() {
        let (lowered, targets) = lower_one_def(": w 1 if 2 exit then 3 ;");
        assert!(!body_uses_exit(&lowered, &targets),
            "EXIT must be gone after tail-inline: {lowered:?}");
        let if_node = lowered.iter().find_map(|e| match e {
            Expr::If { else_body, .. } => else_body.clone(),
            _ => None,
        }).expect("expected an If in lowered body");
        assert!(!if_node.is_empty(), "else_body should hold tail-inlined `3`");
    }

    /// `BEGIN ... IF ... EXIT ... THEN UNTIL` keeps EXIT (loop opacity).
    #[test]
    fn exit_inside_loop_survives() {
        let (lowered, targets) = lower_one_def(": w begin 1 if 2 exit then 0 until ;");
        assert!(body_uses_exit(&lowered, &targets),
            "EXIT inside loop must survive for with-return fallback: {lowered:?}");
    }

    /// Both branches EXIT → tail is dead; whole thing terminates.
    #[test]
    fn both_branches_exit_kills_tail() {
        let (lowered, targets) = lower_one_def(": w 1 if 2 exit else 3 exit then 99 ;");
        assert!(!body_uses_exit(&lowered, &targets));
        assert_eq!(lowered.len(), 2, "expected literal + IF only, got {lowered:?}");
    }
}
