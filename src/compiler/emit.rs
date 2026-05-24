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

use super::ast::{
    CaseArm, CollectionDef, CollectionKind, ConstFlavour, ConstValue,
    ConstantDef, CreateDef, Definition, Expr, Item, Literal, LoopKind,
    Program, VariableDef,
};
use super::lex::StringKind;
use super::resolve::Target;
use super::sema::{EscapeState, Sema};

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

/// Which Factor vocabs the emitted IR needs in its `USING:` clause?
///
/// Baseline (always emitted): `kernel`, `math`, `io`, `forth.runtime`.
/// These cover the emit-time fixed strings the compiler produces
/// regardless of user code (`kernel:if`, `math:zero?`, `io:flush`,
/// `forth.runtime:type` from `."` strings).
///
/// On top of that, each resolved word reference contributes its
/// target's vocab.  Returns a sorted, deduplicated list.
pub fn vocabs_needed(s: &Sema) -> Vec<&'static str> {
    let mut set: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    set.insert("kernel");
    set.insert("math");
    set.insert("io");
    set.insert("forth.runtime");
    // `namespaces` for SYMBOL: get-global / set-global / change-global —
    // used by the narrow-variable path AND by the wide path's
    // hidden-symbol backing.  Cheap to add even when no variables
    // are present.
    set.insert("namespaces");
    for t in s.word_targets.values() {
        if let Some(v) = t.vocab() { set.insert(v); }
    }
    set.into_iter().collect()
}

/// Top-level emit entry point.
pub fn emit(r: &Sema, opts: &EmitOpts) -> String {
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
    // Reorder for Factor's parse-time word lookup:
    //   1. Variables and constants (dictionary entries every : def
    //      might forward-reference).  ANS allows forward references;
    //      Factor's strict parser resolves words at parse time, so
    //      a `:` body that mentions `n` requires `SYMBOL: n` to
    //      have been seen already.  Keep source order within each
    //      group.
    //   2. `:` definitions.  Same forward-reference logic — they
    //      can call each other in any source order so long as all
    //      are visible by parse time.  We emit DEFER: declarations
    //      first to handle mutual recursion (one user word calls
    //      another defined later in source).
    //   3. TopLevel runs in source order.  These execute the
    //      compiled code; reordering them would scramble side
    //      effects.
    //
    // Within categories 1 and 2, source order is preserved.  This
    // means programs that depend on source order between
    // categories (e.g. `100 x !` before `variable x` is declared)
    // are caught by resolve, not silently miscompiled.
    let _ = wrote_def;
    let _ = wrote_top;
    let mut wrote_anything = false;

    // Pass A: vars + consts + CREATE buffers in source order.
    for item in &r.program.items {
        match item {
            Item::Variable(v) => {
                emit_variable(v, r, &mut out); out.push('\n');
                wrote_anything = true;
            }
            Item::Constant(c) => {
                emit_constant(c, &mut out); out.push('\n');
                wrote_anything = true;
            }
            Item::Create(cd) => {
                emit_create(cd, &mut out); out.push('\n');
                wrote_anything = true;
            }
            Item::Collection(cl) => {
                emit_collection(cl, &mut out); out.push('\n');
                wrote_anything = true;
            }
            _ => {}
        }
    }

    // Pass B: `:` definitions in source order.  After variables
    // and constants, so SYMBOL: and CONSTANT: references inside a
    // body resolve at parse time.
    //
    // Forward references BETWEEN user words (mutual recursion, or
    // a word that calls a not-yet-defined later word) currently
    // fail at Factor's parser stage.  Earlier draft used `DEFER:`
    // to forward-declare every user word, but DEFER: changes
    // Factor's strictness around `:` and broke no-annotation
    // definitions.  Mutual recursion is rare in real ANS code;
    // when we hit a test case that needs it, we'll add `DEFER:`
    // selectively per-pair.
    for item in &r.program.items {
        if let Item::Definition(d) = item {
            emit_definition(d, r, &mut out); out.push('\n');
            wrote_anything = true;
        }
    }

    // Pass D: TopLevel runs in source order.
    for item in &r.program.items {
        if let Item::TopLevel { exprs, .. } = item {
            emit_exprs(exprs, r, &mut out);
            out.push('\n');
            wrote_anything = true;
        }
    }
    let _ = wrote_anything;
    if opts.flush_at_end {
        if !out.ends_with(' ') && !out.ends_with('\n') { out.push(' '); }
        out.push_str("flush");
    }
    out
}

fn emit_using_line(r: &Sema, out: &mut String) {
    let vocabs = vocabs_needed(r);
    out.push_str("USING:");
    for v in vocabs { out.push(' '); out.push_str(v); }
    out.push_str(" ;\n");
}

/// Emit a VARIABLE.  Two paths, decided by sema's escape analysis:
///
/// **Narrow** (every use is `@`/`!`/`+!`/`c@`/`c!`): the user-visible
/// "address" is really just an ANS naming convention; we emit a
/// Factor `SYMBOL:` and translate the @/!/+! sinks to
/// `get-global`/`set-global`/`change-global` via the peep-emit in
/// `emit_exprs`.  Factor's optimiser can see across these and
/// constant-fold or hoist load/store across loops.
///
/// **Wide** (address escapes): a backing nf-addr byte-array bound
/// to a hidden SYMBOL at definition time, plus a wrapping word
/// that returns the same address on every call (matching ANS).
/// The address then flows through `forth.runtime:@`/`!`/etc.
///
/// Both paths initialise to 0 so `x @` before any `x !` returns 0,
/// matching ANS (variables read 0 before first store).
fn emit_variable(v: &VariableDef, r: &Sema, out: &mut String) {
    let lc = v.name.to_ascii_lowercase();
    let is_narrow = matches!(r.escape.get(&lc), Some(EscapeState::Narrow));
    if is_narrow {
        // Narrow: `SYMBOL: x` defines `x` as a parser-level word
        // that pushes the symbol itself when executed.  The peep in
        // emit_exprs translates `x @` to `x get-global` etc.
        write!(out,
            "SYMBOL: {n}\n0 {n} set-global",
            n = v.name).unwrap();
    } else {
        // Wide: hidden SYMBOL holds the one nf-addr; user-visible
        // word returns it.
        write!(out,
            "SYMBOL: nf-var-{n}\n<variable> nf-var-{n} set-global\n: {n} ( -- addr ) nf-var-{n} get-global ; inline",
            n = v.name).unwrap();
    }
}

/// Emit a standard collection (array / farray / cbuffer).  Three
/// lines per instance:
///
/// 1. `SYMBOL: nf-coll-<name>`               (the storage handle)
/// 2. `<bytes> <buffer> nf-coll-<name> set-global`  (allocate
///    `count * elt_size` bytes, store as the handle's value)
/// 3. `: <name> ( idx -- addr )                     (the accessor)
///        nf-coll-<name> get-global swap <elt_size> * nf-addr+
///    ; inline`
///
/// The accessor uses `nf-addr+` rather than `+` because + on an
/// nf-addr fails (our address model is opaque).  Future
/// optimisation: emit Factor `specialized-array`s when the
/// elements are known-typed and access patterns are recognisable
/// — that's a M2.9b+ task.
fn emit_collection(cl: &CollectionDef, out: &mut String) {
    let n = &cl.name;
    let elt_size = cl.kind.elt_size();
    let total_bytes = cl.count.saturating_mul(elt_size).max(1);
    let multiplier = elt_size;  // accessor multiplies idx by this
    write!(out,
        "SYMBOL: nf-coll-{n}\n{total_bytes} <buffer> nf-coll-{n} set-global\n: {n} ( idx -- addr ) nf-coll-{n} get-global swap {multiplier} * forth.runtime:nf-addr+ ; inline",
        n = n, total_bytes = total_bytes, multiplier = multiplier,
    ).unwrap();
}

/// Emit a CREATE'd data buffer.  Same wide-path pattern as a
/// variable but the backing byte-array is sized by ALLOT (rather
/// than a single cell).  CREATE is always emitted wide because
/// callers do address arithmetic on the result (`name N cells + @`)
/// which our current escape analyser flags as escape.
///
/// DOES> is M2.9b — when it lands, the wrapping word's body picks
/// up a runtime-action quotation.  For now CREATE without DOES>
/// just exposes the address.
fn emit_create(cd: &CreateDef, out: &mut String) {
    let n = &cd.name;
    let bytes = cd.allotted_bytes.max(1); // 0-byte buffers aren't useful
    write!(out,
        "SYMBOL: nf-create-{n}\n{bytes} <buffer> nf-create-{n} set-global\n: {n} ( -- addr ) nf-create-{n} get-global ; inline",
        n = n, bytes = bytes,
    ).unwrap();
}

/// Emit a CONSTANT / FCONSTANT.  Factor's `CONSTANT:` is a parsing
/// word that captures a single literal token at parse time and
/// creates a constant word.  Identical semantics to ANS.
fn emit_constant(c: &ConstantDef, out: &mut String) {
    match c.value {
        ConstValue::Int(v) => {
            write!(out, "CONSTANT: {} {}", c.name, v).unwrap();
        }
        ConstValue::Float(v) => {
            // Force decimal point so Factor parses as float, not int.
            if v.fract() == 0.0 && v.is_finite() {
                write!(out, "CONSTANT: {} {:.1}", c.name, v).unwrap();
            } else {
                write!(out, "CONSTANT: {} {}", c.name, v).unwrap();
            }
        }
    }
    let _ = c.flavour;  // Cell vs Float discriminator already reflected in value
}

fn emit_definition(d: &Definition, r: &Sema, out: &mut String) {
    write!(out, ": {} ", d.name).unwrap();
    // Factor's `:` REQUIRES a stack-effect annotation.  Picking
    // which one to emit follows the principle that synth is
    // authoritative (it's derived from the body, so it's correct
    // by construction) while the user's declaration carries
    // documentation value (names like `n^2`, `c-addr`) that the
    // synth can't recover.
    //
    // Decision table:
    //
    //   declared    synth         emit
    //   ─────────   ─────────     ────────────────────────────────
    //   present     Known, match  declared  (synth confirms; keep names)
    //   present     Known, ≠      synth     (synth wins; counts correct)
    //   present     Unknown       declared  (synth can't speak; trust user)
    //   absent      Known         synth     (synthesise from body)
    //   absent      Unknown       row-vars  (give up; accept any)
    let lc = d.name.to_ascii_lowercase();
    // `body_effects` is the body-walk truth, separate from
    // `user_effects` which is the caller's view (declared if
    // present).  We want truth here, not the user's possibly-stale
    // claim.
    let synth = r.body_effects.get(&lc).copied();
    let declared_counts = d.effect.as_ref()
        .map(|e| (e.inputs.len() as u32, e.outputs.len() as u32));

    let emit_synth = |out: &mut String, inputs: u32, outputs: u32| {
        synth_effect_annotation(inputs, outputs, out);
    };
    let emit_declared = |out: &mut String| {
        let eff = d.effect.as_ref().unwrap();
        out.push('(');
        for s in &eff.inputs { out.push(' '); out.push_str(s); }
        if eff.inputs.is_empty() { out.push(' '); }
        out.push_str(" --");
        for s in &eff.outputs { out.push(' '); out.push_str(s); }
        out.push_str(" ) ");
    };

    match (declared_counts, synth) {
        (Some((di, do_)), Some(super::effect::Effect::Known { inputs, outputs }))
            if di == inputs && do_ == outputs =>
        {
            // Counts match — emit user's annotation with names.
            emit_declared(out);
        }
        (Some(_), Some(super::effect::Effect::Known { inputs, outputs })) => {
            // Declared but synth says different.  Synth wins.
            // The diagnostic for the mismatch is already in sema.
            emit_synth(out, inputs, outputs);
        }
        (Some(_), _) => {
            // Synth is Unknown; declared is best we have.
            emit_declared(out);
        }
        (None, Some(super::effect::Effect::Known { inputs, outputs })) => {
            emit_synth(out, inputs, outputs);
        }
        (None, _) => {
            out.push_str("( ..a -- ..b ) ");
        }
    }
    emit_exprs(&d.body, r, out);
    out.push_str(" ;");
}

/// Render `(inputs -- outputs)` with synthetic item names (a, b, c…
/// for inputs; r0, r1, … for outputs).  The names don't carry
/// meaning in Factor; we just need *something* there.
fn synth_effect_annotation(inputs: u32, outputs: u32, out: &mut String) {
    out.push('(');
    for i in 0..inputs {
        out.push(' ');
        // a, b, c, …, z, aa, ab, … if we ever overrun 26.
        out.push(((b'a' + (i % 26) as u8)) as char);
    }
    if inputs == 0 { out.push(' '); }
    out.push_str(" --");
    for i in 0..outputs {
        write!(out, " r{i}").unwrap();
    }
    if outputs == 0 { out.push(' '); }
    out.push_str(" ) ");
}

fn emit_exprs(exprs: &[Expr], r: &Sema, out: &mut String) {
    let mut first = true;
    let mut i = 0;
    while i < exprs.len() {
        if !first { out.push(' '); }
        first = false;
        // Peep: a WordRef of a narrow variable, followed by a
        // recognised sink, becomes a Factor-global access in one
        // step.  Skips both source tokens.
        if let Some(consumed) = try_emit_narrow_sink(exprs, i, r, out) {
            i += consumed;
            continue;
        }
        emit_expr(&exprs[i], r, out);
        i += 1;
    }
}

/// If `exprs[i..]` starts with `<narrow-var> @|!|+!|c@|c!`, emit the
/// corresponding Factor global-access form and return how many
/// source tokens (always 2) were consumed.  Otherwise None.
fn try_emit_narrow_sink(
    exprs: &[Expr],
    i: usize,
    r: &Sema,
    out: &mut String,
) -> Option<usize> {
    let cur  = exprs.get(i)?;
    let next = exprs.get(i + 1)?;
    let (var_name, _var_span) = match cur {
        Expr::WordRef { name, span } => (name, span),
        _ => return None,
    };
    let next_name = match next {
        Expr::WordRef { name, .. } => name,
        _ => return None,
    };
    let var_lc = var_name.to_ascii_lowercase();
    if !matches!(r.escape.get(&var_lc), Some(EscapeState::Narrow)) {
        return None;
    }
    let next_lc = next_name.to_ascii_lowercase();
    match next_lc.as_str() {
        "@" | "c@" => {
            write!(out, "{var_name} get-global").unwrap();
            Some(2)
        }
        "!" | "c!" => {
            write!(out, "{var_name} set-global").unwrap();
            Some(2)
        }
        "+!" => {
            // ANS `value var +!` ⇒ var := var + value.
            // Factor `change-global` is ( variable quot -- ), so
            // we emit `var [ + ] change-global` — variable below,
            // quot on top.  At runtime change-global pops [+]
            // (quot), pops var (symbol), reads var, calls [+]
            // with (value, current), stores result.
            write!(out, "{var_name} [ + ] change-global").unwrap();
            Some(2)
        }
        _ => None,
    }
}

fn emit_expr(e: &Expr, r: &Sema, out: &mut String) {
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
    r: &Sema,
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
    use super::super::{lex, parse, sema::build as build_sema};

    fn compile_str(src: &str) -> String {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        let sema = build_sema(prog).unwrap();
        emit(&sema, &EmitOpts::default())
    }

    #[test]
    fn vocabs_needed_includes_runtime_for_dot() {
        let toks = lex("42 .").unwrap();
        let prog = parse(&toks).unwrap();
        let sema = build_sema(prog).unwrap();
        let v = vocabs_needed(&sema);
        assert!(v.contains(&"forth.runtime"));
        assert!(v.contains(&"kernel"));
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
