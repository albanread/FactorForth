//! Emit — `Resolved` AST → canonical Factor source string.
//!
//! The output is what gets handed to `nf_eval_string`.  It is
//! deliberately straightforward:
//!
//! 1. A `USING:` line listing every vocab any word in the program
//!    needs.  Always includes `kernel` and `io`; resolve.vocabs_needed
//!    contributes the rest.
//!
//! 2. Each `: name ( effect ) body ;` definition, in source order.
//!
//! 3. A trailing run of top-level expressions (load-time code).
//!
//! 4. A final ` flush` so any buffered I/O surfaces before
//!    nf_eval_string returns.
//!
//! Factor's `( a b -- c )` stack effect syntax is identical to
//! ANS's, so we emit effect annotations verbatim.
//!
//! Numbers, strings, and floats use Factor's own literal syntax —
//! also identical for the common cases (decimal, hex with `HEX:`
//! prefix, floats with `.`/`e`).  We emit hex via Factor's
//! `HEX: nnnn` syntax for prefixed integers to match the source
//! intent.

use std::fmt::Write;

use super::ast::{CaseArm, Definition, Expr, Item, Literal, LoopKind, Program};
use super::lex::StringKind;
use super::resolve::{vocabs_needed, Resolved, Target};

/// Emit options.  Defaults are tuned for "send to embedded VM".
#[derive(Clone, Debug)]
pub struct EmitOpts {
    /// Append `flush` at the very end so I/O round-trips through
    /// nf_eval_string.  Default true.  Set false when the IR is
    /// for diagnostic display only.
    pub flush_at_end: bool,
}

impl Default for EmitOpts {
    fn default() -> Self { EmitOpts { flush_at_end: true } }
}

/// Top-level emit entry point.
pub fn emit(r: &Resolved, opts: &EmitOpts) -> String {
    let mut out = String::with_capacity(256);
    emit_using_line(r, &mut out);
    // `:` definitions need a target vocab.  Factor's `scratchpad`
    // vocab exists in any bootstrapped image and is the conventional
    // home for interactive / eval'd definitions.  Without this, a
    // colon definition in eval'd source errors with "Not in a
    // vocabulary; IN: form required".
    out.push_str("IN: scratchpad\n");
    let mut wrote_def = false;
    let mut wrote_top = false;
    for item in &r.program.items {
        match item {
            Item::Definition(d) => {
                if wrote_top { out.push('\n'); wrote_top = false; }
                emit_definition(d, r, &mut out);
                out.push('\n');
                wrote_def = true;
            }
            Item::TopLevel { exprs, .. } => {
                if wrote_def && !wrote_top { out.push('\n'); }
                emit_exprs(exprs, r, &mut out);
                wrote_top = true;
            }
        }
    }
    if opts.flush_at_end {
        if !out.ends_with(' ') && !out.ends_with('\n') { out.push(' '); }
        out.push_str("flush");
    }
    out
}

fn emit_using_line(r: &Resolved, out: &mut String) {
    let vocabs = vocabs_needed(r);
    out.push_str("USING:");
    for v in vocabs { out.push(' '); out.push_str(v); }
    out.push_str(" ;\n");
}

fn emit_definition(d: &Definition, r: &Resolved, out: &mut String) {
    write!(out, ": {} ", d.name).unwrap();
    if let Some(eff) = &d.effect {
        out.push('(');
        for s in &eff.inputs { out.push(' '); out.push_str(s); }
        if eff.inputs.is_empty() { out.push(' '); }
        out.push_str(" --");
        for s in &eff.outputs { out.push(' '); out.push_str(s); }
        out.push_str(" ) ");
    }
    emit_exprs(&d.body, r, out);
    out.push_str(" ;");
}

fn emit_exprs(exprs: &[Expr], r: &Resolved, out: &mut String) {
    let mut first = true;
    for e in exprs {
        if !first { out.push(' '); }
        first = false;
        emit_expr(e, r, out);
    }
}

fn emit_expr(e: &Expr, r: &Resolved, out: &mut String) {
    match e {
        Expr::Lit(Literal::Int { value, .. }) => {
            write!(out, "{value}").unwrap();
        }
        Expr::Lit(Literal::Float { value, .. }) => {
            // Factor's float literal syntax matches Rust's for the
            // common forms.  Force at least one decimal digit so a
            // bare `3.0` doesn't become `3` and get re-parsed as int.
            if value.fract() == 0.0 && value.is_finite() {
                write!(out, "{value:.1}").unwrap();
            } else {
                write!(out, "{value}").unwrap();
            }
        }
        Expr::Lit(Literal::Str { value, kind, .. }) => {
            match kind {
                StringKind::DotQuote => {
                    // ANS `." x"` is "emit x at runtime".  Translate
                    // to `"x" forth.runtime:type` — type writes a
                    // counted string and is the right ANS semantic.
                    out.push('"');
                    out.push_str(&factor_escape(value));
                    out.push_str("\" forth.runtime:type");
                }
                StringKind::SQuote | StringKind::CQuote => {
                    // For now treat both as raw string literal on the
                    // data stack.  When forth.runtime grows S" and C"
                    // proper handling, replace this.
                    out.push('"');
                    out.push_str(&factor_escape(value));
                    out.push('"');
                }
            }
        }
        Expr::WordRef { span, name } => {
            match r.word_targets.get(span) {
                Some(t) => out.push_str(&t.to_factor_token()),
                None => {
                    // resolve would've errored — defensive fallback.
                    out.push_str(name);
                }
            }
        }

        // ── Control flow ───────────────────────────────────────────
        //
        // ANS structures are recursive; Factor's combinator form is
        // pure (no side effects in the structure itself, just the
        // body quotations).  Emit:
        //
        //   IF t [ELSE e] THEN  →  [ t ] [ e ] if         (else absent
        //                          [ t ] when             → use `when`)
        //   BEGIN b UNTIL       →  [ b zero? ] loop
        //                          (loop while body returns true →
        //                           ANS UNTIL loops while flag = 0)
        //   BEGIN p WHILE b REPEAT → [ p ] [ b ] while
        //   BEGIN b AGAIN       →  [ b t ] loop
        //                          (infinite — LEAVE/EXIT lands later)

        Expr::If { then_body, else_body, .. } => {
            out.push_str("[ ");
            emit_exprs(then_body, r, out);
            out.push_str(" ] ");
            if let Some(eb) = else_body {
                out.push_str("[ ");
                emit_exprs(eb, r, out);
                out.push_str(" ] if");
            } else {
                out.push_str("when");
            }
        }
        Expr::BeginUntil { body, .. } => {
            // ANS: continue while flag == 0; Factor loop: continue
            // while body returns t.  Body produces flag; we want
            // `flag zero?` as the loop continuation.
            out.push_str("[ ");
            emit_exprs(body, r, out);
            out.push_str(" math:zero? ] kernel:loop");
        }
        Expr::BeginWhileRepeat { pred, body, .. } => {
            // Avoid Factor's `while` here.  `while` has strict
            // stack-effect inference that requires pred to produce
            // a clean boolean above an unchanged ..a — for ANS
            // predicates like a bare `dup` (which extends the stack
            // rather than consuming-and-flagging), the inference
            // diverges and the compiler hangs at eval time.
            //
            // Instead, emit via `loop` directly with an explicit
            // zero-test branch:
            //
            //   [ <pred> math:zero?
            //     [ f ] [ <body> t ] kernel:if
            //   ] kernel:loop
            //
            // Trace per iteration:
            //   pred       leaves flag on top of ..a
            //   zero?      converts ANS-flag to Factor t/f (t == was 0)
            //   if         t → return f to loop  (exit)
            //              f → run body, return t (continue)
            //   loop       pops the returned flag
            out.push_str("[ ");
            emit_exprs(pred, r, out);
            out.push_str(" math:zero? [ f ] [ ");
            emit_exprs(body, r, out);
            out.push_str(" t ] kernel:if ] kernel:loop");
        }
        Expr::BeginAgain { body, .. } => {
            // Genuinely infinite — `loop` continues while body returns t,
            // so push t after the body.  LEAVE/EXIT will be added in
            // a later milestone via continuation throws.
            out.push_str("[ ");
            emit_exprs(body, r, out);
            out.push_str(" t ] kernel:loop");
        }

        // ── DO/LOOP, ?DO/LOOP, DO/+LOOP, ?DO/+LOOP ─────────────────
        //
        // The runtime entry points (forth.runtime:do-loop /
        // ?do-loop) take ( limit start quot -- ).  The body
        // quotation MUST leave a step amount on the stack as its
        // last action — bump-loop consumes it.
        //
        //   LOOP   →  body emits ` 1` at end (compiler-injected)
        //   +LOOP  →  body's final user expression already leaves
        //             the step on top; we emit nothing extra
        //
        // limit and start are already on the data stack from
        // expressions preceding the DO marker — they aren't part of
        // the DoLoop AST node.
        Expr::DoLoop { is_qdo, body, loop_kind, .. } => {
            out.push_str("[ ");
            emit_exprs(body, r, out);
            if matches!(loop_kind, LoopKind::Plus1) {
                // Step +1 for plain LOOP.
                out.push_str(" 1");
            }
            // PlusN: body's tail already produces the step.
            out.push_str(" ] ");
            if *is_qdo {
                out.push_str("forth.runtime:?do-loop");
            } else {
                out.push_str("forth.runtime:do-loop");
            }
        }

        // ── CASE/OF/ENDOF/ENDCASE ──────────────────────────────────
        //
        // Emit as a nested IF chain, recursing through arms.  The
        // dispatch value sits on the data stack at CASE entry; each
        // arm dups it for comparison and drops both copies on match.
        // ENDCASE's drop fires in the innermost else, where the
        // dispatch value has survived all OF tests.
        //
        // Shape:
        //   dup MATCH0 = [ drop BODY0 ] [
        //     dup MATCH1 = [ drop BODY1 ] [
        //       ...
        //         DEFAULT? drop
        //     ] kernel:if
        //   ] kernel:if
        //
        // No arms + no default → just `drop` (an ANS-vacuous CASE).
        Expr::Case { arms, default, .. } => {
            emit_case_chain(arms, default.as_deref(), r, out);
        }
    }
}

/// Recursive helper for `Expr::Case`.  Splits the arms list head/tail,
/// emits a `dup MATCH = [ drop BODY ] [ <rest> ] if` per arm, with
/// the base case being the default branch (if any) followed by the
/// single ENDCASE-drop.
fn emit_case_chain(
    arms: &[CaseArm],
    default: Option<&[Expr]>,
    r: &Resolved,
    out: &mut String,
) {
    if let Some((head, tail)) = arms.split_first() {
        out.push_str("dup ");
        emit_exprs(&head.match_expr, r, out);
        out.push_str(" = [ drop ");
        emit_exprs(&head.body, r, out);
        out.push_str(" ] [ ");
        emit_case_chain(tail, default, r, out);
        out.push_str(" ] kernel:if");
    } else {
        // Base case: no more arms.  Run default if any, then drop
        // the dispatch value (ENDCASE's job).
        if let Some(d) = default {
            emit_exprs(d, r, out);
            // Convention: default leaves the dispatch value on top
            // for ENDCASE to drop.  If it didn't, the trailing drop
            // will underflow at runtime — same as ANS would.
            out.push(' ');
        }
        out.push_str("kernel:drop");
    }
}

/// Escape a Forth string body for safe inclusion in a Factor `"..."`.
/// Factor's string syntax recognises `\` escapes; we double-escape
/// backslashes and escape the closing quote.  Forth strings are raw
/// in the source, so backslash carries no special meaning there.
fn factor_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"'  => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{lex, parse, resolve::resolve};

    fn compile_str(src: &str) -> String {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        let r = resolve(prog).unwrap();
        emit(&r, &EmitOpts::default())
    }

    #[test]
    fn empty_program_emits_only_using() {
        let out = compile_str("");
        assert!(out.starts_with("USING:"));
        assert!(out.contains("flush"));
    }

    #[test]
    fn integer_literal_passes_through() {
        let out = compile_str("42 .");
        assert!(out.contains("42"));
        assert!(out.contains("forth.runtime:."));
    }

    #[test]
    fn simple_definition_emits_colon() {
        let out = compile_str(": square ( n -- n^2 ) dup * ;");
        assert!(out.contains(": square ( n -- n^2 ) dup * ;"),
                "expected canonical colon def in {out:?}");
    }

    #[test]
    fn user_word_call_after_def() {
        let out = compile_str(": square ( n -- n^2 ) dup * ; 5 square .");
        // square def + top-level "5 square forth.runtime:."
        assert!(out.contains(": square"));
        assert!(out.contains("5 square forth.runtime:."));
    }

    #[test]
    fn ans_division_maps_to_integer_divide() {
        let out = compile_str("10 3 /");
        // ANS `/` is integer divide; we picked Factor `/i`.
        assert!(out.contains("/i"), "expected /i, got {out}");
    }

    #[test]
    fn float_keeps_decimal_point() {
        let out = compile_str("3.0");
        assert!(out.contains("3.0"), "got {out}");
    }

    #[test]
    fn dot_quote_emits_type() {
        let out = compile_str(": greet .\" hi\" ;");
        assert!(out.contains("\"hi\" forth.runtime:type"), "got {out}");
    }
}
