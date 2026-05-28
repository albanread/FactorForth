//! lower_qdup — pre-resolve AST pass that desugars ANS `?DUP IF`
//! into `DUP IF ... ELSE DROP THEN`.
//!
//! ## Why this is a desugar, not a runtime word
//!
//! ANS `?DUP` has effect `( x -- 0 | x x )` — polymorphic, with the
//! number of stack items produced depending on the input value.
//! Factor's `compiler.tree.propagation` uses SSA-shaped dataflow and
//! refuses to compile any word whose branches leave a different
//! number of items on the stack.  There is no Factor-side body for
//! `?DUP` that the JIT will accept.
//!
//! The good news: ~all real-world Forth code uses `?DUP` immediately
//! before an `IF`, precisely so the IF can consume the polymorphic
//! top.  At the AST level we can rewrite the pair into a balanced
//! shape that Factor's checker accepts and the JIT compiles to
//! zero-cost machine code:
//!
//! ```text
//!   ?DUP IF t THEN          →  DUP IF t ELSE DROP THEN
//!   ?DUP IF t ELSE e THEN   →  DUP IF t ELSE DROP e THEN
//! ```
//!
//! Why it's semantically identical:
//!
//!   * Input `x ≠ 0` — `DUP` produces `x x`; `IF` consumes the top
//!     truthy `x`, runs `t` with `x` still on the stack.  ANS `?DUP IF`
//!     behaves the same way.
//!   * Input `0` — `DUP` produces `0 0`; `IF` consumes the top `0`
//!     (falsy), runs `ELSE` which is `DROP <original-else>`.  The
//!     `DROP` removes the remaining `0`, leaving the stack as it
//!     was before `?DUP`.  ANS `?DUP IF` also leaves the stack with
//!     the `0` consumed (since `IF` consumed it without dup'ing).
//!     Matches.
//!
//! ## Scope
//!
//! Detects `Expr::WordRef { name: "?dup" }` followed *immediately*
//! by `Expr::If` in any expression sequence (def body, IF arm, CASE
//! arm, loop body, top-level).  Recursive — `?DUP IF` nested inside
//! another structure gets the same treatment.
//!
//! A bare `?DUP` not followed by `IF` is left untouched, which means
//! resolve will fail with "unknown word ?dup".  Standalone `?DUP`
//! has no Factor-compilable shape; the error is the right outcome.
//! (If a real demo needs it, we'd extend this pass with the
//! tail-swallow trick from `lower_exit` to wrap the enclosing
//! block's remainder.)

use std::sync::atomic::{AtomicU32, Ordering};

use super::ast::{CaseArm, Expr, Item, Program};
use super::error::{Pos, Span};

/// Counter used to fabricate unique `Span`s for synthesised WordRefs.
///
/// Background: `resolve.rs` keys `word_targets` by `Span`, so two
/// WordRefs that share a span have one of their resolved targets
/// silently overwrite the other.  Our peephole emits *two* new
/// WordRefs (`dup` and `drop`); they need distinct spans even though
/// they share an origin token.  We pick byte_offsets from the
/// upper end of the u32 range — well past any real source position
/// the parser will ever produce — and count down.  Line/col are
/// carried over from the original token for diagnostics.
static SYNTH_COUNTER: AtomicU32 = AtomicU32::new(0xFFFF_0000);

fn synth_span(orig: Span) -> Span {
    let n = SYNTH_COUNTER.fetch_sub(1, Ordering::Relaxed);
    Span {
        start: Pos { line: orig.start.line, col: orig.start.col, byte_offset: n },
        end:   Pos { line: orig.end.line,   col: orig.end.col,   byte_offset: n },
    }
}

/// Run the desugar over a whole `Program`, returning a new program
/// where every `?DUP IF` pair has been rewritten.  Idempotent on
/// programs with no `?DUP`.
pub fn lower_program(mut prog: Program) -> Program {
    for item in prog.items.iter_mut() {
        match item {
            Item::Definition(d) => {
                d.body = lower_seq(&d.body);
            }
            Item::TopLevel { exprs, .. } => {
                *exprs = lower_seq(exprs);
            }
            Item::Constant(c) => {
                if let super::ast::ConstValue::Computed(exprs) = &mut c.value {
                    *exprs = lower_seq(exprs);
                }
            }
            Item::Template(t) => {
                t.constructor = lower_seq(&t.constructor);
                t.does_body   = lower_seq(&t.does_body);
            }
            Item::Value(v) => {
                v.initial = lower_seq(&v.initial);
            }
            Item::Method(m) => {
                // Method body flows through the same desugar pipeline
                // as `:` definition bodies.
                m.body = lower_seq(&m.body);
            }
            // Class / Generic / RawFactor have no Forth-side body to walk.
            Item::Class(_) | Item::Generic(_) | Item::RawFactor(_) => {}
            Item::TemplateInstance(ti) => {
                ti.does_body = lower_seq(&ti.does_body);
            }
            // Variables, CREATEs, Collections carry no body to walk.
            Item::Variable(_) | Item::Create(_) | Item::Collection(_) => {}
        }
    }
    prog
}

/// Walk a sequence of expressions, rewriting `?DUP IF` pairs and
/// recursing into nested control flow.
fn lower_seq(seq: &[Expr]) -> Vec<Expr> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        let e = &seq[i];
        // Peephole: `?dup` immediately followed by IF.
        if is_qdup_word(e) {
            if let Some(Expr::If { then_body, else_body, span: if_span }) = seq.get(i + 1) {
                let qdup_span = e.span();
                // Two synthesised WordRefs (`dup` outside the IF,
                // `drop` inside the ELSE) need distinct spans so
                // resolve's word_targets map doesn't collapse them
                // onto the same key.  See `synth_span` for why.
                let dup_span  = synth_span(qdup_span);
                let drop_span = synth_span(qdup_span);
                let then_l = lower_seq(then_body);
                let else_l_inner = else_body.as_ref().map(|b| lower_seq(b)).unwrap_or_default();
                // Build new ELSE: prepend DROP to consume the duped value.
                let mut new_else = Vec::with_capacity(1 + else_l_inner.len());
                new_else.push(Expr::WordRef {
                    name: "drop".to_string(),
                    span: drop_span,
                });
                new_else.extend(else_l_inner);
                // Emit DUP + the rewritten IF.
                out.push(Expr::WordRef {
                    name: "dup".to_string(),
                    span: dup_span,
                });
                out.push(Expr::If {
                    then_body: then_l,
                    else_body: Some(new_else),
                    span: *if_span,
                });
                i += 2; // consumed both ?dup and the IF
                continue;
            }
            // Standalone ?dup (no IF immediately after) — leave it
            // so resolve fails with "unknown word ?dup".  The error
            // is right: standalone ?dup is uncompilable.
            out.push(e.clone());
            i += 1;
            continue;
        }
        // Not a ?dup; recurse into nested control flow if any.
        out.push(rewrite_nested(e));
        i += 1;
    }
    out
}

fn is_qdup_word(e: &Expr) -> bool {
    matches!(e, Expr::WordRef { name, .. } if name.eq_ignore_ascii_case("?dup"))
}

/// Apply `lower_seq` recursively to every expression-sequence held
/// inside a control-flow node.  Non-control-flow nodes pass through.
fn rewrite_nested(e: &Expr) -> Expr {
    match e {
        Expr::If { then_body, else_body, span } => Expr::If {
            then_body: lower_seq(then_body),
            else_body: else_body.as_ref().map(|b| lower_seq(b)),
            span: *span,
        },
        Expr::BeginUntil { body, span } => Expr::BeginUntil {
            body: lower_seq(body),
            span: *span,
        },
        Expr::BeginAgain { body, span } => Expr::BeginAgain {
            body: lower_seq(body),
            span: *span,
        },
        Expr::BeginWhileRepeat { pred, body, span } => Expr::BeginWhileRepeat {
            pred: lower_seq(pred),
            body: lower_seq(body),
            span: *span,
        },
        Expr::DoLoop { is_qdo, body, loop_kind, span } => Expr::DoLoop {
            is_qdo: *is_qdo,
            body: lower_seq(body),
            loop_kind: *loop_kind,
            span: *span,
        },
        Expr::Case { arms, default, span } => Expr::Case {
            arms: arms.iter().map(|a| CaseArm {
                match_expr: lower_seq(&a.match_expr),
                body:       lower_seq(&a.body),
                span: a.span,
            }).collect(),
            default: default.as_ref().map(|d| lower_seq(d)),
            span: *span,
        },
        _ => e.clone(),
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{lex, parse};

    fn lower_one_def(src: &str) -> Vec<Expr> {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        let lowered = lower_program(prog);
        lowered.items.into_iter().find_map(|it| match it {
            Item::Definition(d) => Some(d.body),
            _ => None,
        }).expect("test source must contain a `:` definition")
    }

    /// `?DUP IF t THEN` → `DUP IF t ELSE DROP THEN`.
    #[test]
    fn qdup_if_then_only() {
        let body = lower_one_def(": w ?dup if 99 then ;");
        // Expect: WordRef(dup), If{then=[99], else=Some([drop])}.
        assert_eq!(body.len(), 2, "expected dup + if, got {body:?}");
        match &body[0] {
            Expr::WordRef { name, .. } => assert_eq!(name, "dup"),
            other => panic!("expected dup, got {other:?}"),
        }
        match &body[1] {
            Expr::If { else_body: Some(eb), .. } => {
                assert!(matches!(&eb[0], Expr::WordRef { name, .. } if name == "drop"),
                    "else should start with drop: {eb:?}");
            }
            other => panic!("expected If with else, got {other:?}"),
        }
    }

    /// `?DUP IF t ELSE e THEN` → `DUP IF t ELSE DROP e THEN`.
    #[test]
    fn qdup_if_then_else() {
        let body = lower_one_def(": w ?dup if 1 else 2 then ;");
        match &body[1] {
            Expr::If { else_body: Some(eb), .. } => {
                assert!(matches!(&eb[0], Expr::WordRef { name, .. } if name == "drop"));
                // Followed by the original ELSE body's `2`.
                assert!(matches!(&eb[1], Expr::Lit(_)),
                    "drop should be followed by original else body: {eb:?}");
            }
            other => panic!("expected If with else, got {other:?}"),
        }
    }

    /// `?DUP` nested inside an outer IF also gets rewritten.
    #[test]
    fn qdup_inside_outer_if() {
        let body = lower_one_def(": w 1 if ?dup if 2 then then ;");
        match &body[1] {
            Expr::If { then_body, .. } => {
                // Inner: should now be [dup, If{then=[2], else=Some([drop])}].
                assert_eq!(then_body.len(), 2, "{then_body:?}");
                assert!(matches!(&then_body[0], Expr::WordRef { name, .. } if name == "dup"));
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    /// `?DUP` inside a DO/LOOP body gets rewritten.
    #[test]
    fn qdup_inside_loop() {
        let body = lower_one_def(": w 10 0 do i ?dup if drop then loop ;");
        match &body[2] {
            Expr::DoLoop { body: lb, .. } => {
                // Should contain: i, dup, If{...}.  Find the dup.
                assert!(lb.iter().any(|e|
                    matches!(e, Expr::WordRef { name, .. } if name == "dup")
                ), "{lb:?}");
            }
            other => panic!("expected DoLoop, got {other:?}"),
        }
    }

    /// Bare `?DUP` without a following IF is left alone (will fail
    /// resolve with "unknown word").
    #[test]
    fn standalone_qdup_unchanged() {
        let body = lower_one_def(": w ?dup ;");
        assert!(matches!(&body[0], Expr::WordRef { name, .. } if name == "?dup"));
    }
}
