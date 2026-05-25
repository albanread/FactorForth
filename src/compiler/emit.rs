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
    Program, TemplateInstanceDef, VariableDef,
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
    // math.functions / math.order / math.constants — needed for LET
    // codegen (sqrt sin cos pi e min max etc.).  Even when no LET
    // form is present, `math.order:<` and friends appear via the
    // boolean-convention wrappers.  All loaded by basis bootstrap.
    set.insert("math.functions");
    set.insert("math.order");
    set.insert("math.constants");
    for t in s.word_targets.values() {
        if let Some(v) = t.vocab() { set.insert(v); }
    }
    // `continuations` lands in `set` automatically when EXIT is
    // used — its target's vocab IS "continuations" — so the
    // `with-return` wrap emit.rs adds for the same case resolves.
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
                emit_constant(c, r, &mut out); out.push('\n');
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
            Item::TemplateInstance(ti) => {
                emit_template_instance(ti, &mut out); out.push('\n');
                wrote_anything = true;
            }
            // Item::Template itself emits NOTHING — it's a parse-
            // time/sema-time construct.  Instances carry the body.
            Item::Template(_) => {}
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

/// Emit a CREATE/DOES> template instance (M2.9b).  Same overall
/// shape as a Collection — SYMBOL holding a backing buffer, plus
/// an accessor — but the accessor's body is the captured does_body
/// from the source template, with two minimal translations:
///
///   - `+` becomes `forth.runtime:nf-addr+` (so user code like
///     `does> swap cells +` indexes correctly into our opaque
///     nf-addr instead of failing on `+`).
///   - Everything else passes through verbatim via `emit_expr`,
///     so cells/chars/@/!/etc. work normally.
fn emit_template_instance(ti: &TemplateInstanceDef, out: &mut String) {
    let n = &ti.name;
    let bytes = ti.allocated_bytes.max(1);
    write!(out,
        "SYMBOL: nf-tmpl-{n}\n{bytes} <buffer> nf-tmpl-{n} set-global\n: {n} ( idx -- addr ) nf-tmpl-{n} get-global ",
        n = n, bytes = bytes,
    ).unwrap();
    emit_does_body(&ti.does_body, out);
    write!(out, " ; inline").unwrap();
}

/// Walk a captured does_body and emit each expression.  WordRefs
/// to `+` get translated to `nf-addr+` (since after the SYMBOL
/// push the data-stack top is an nf-addr, not a number).  We
/// don't yet do this for `-` because address subtraction is rare
/// in ANS code; will add when the first test forces it.
fn emit_does_body(exprs: &[Expr], out: &mut String) {
    let mut first = true;
    for e in exprs {
        if !first { out.push(' '); }
        first = false;
        match e {
            Expr::WordRef { name, .. } if name == "+" => {
                out.push_str("forth.runtime:nf-addr+");
            }
            Expr::Lit(Literal::Int { value, .. }) => {
                write!(out, "{value}").unwrap();
            }
            Expr::Lit(Literal::Float { value, .. }) => {
                if value.fract() == 0.0 && value.is_finite() {
                    write!(out, "{value:.1}").unwrap();
                } else {
                    write!(out, "{value}").unwrap();
                }
            }
            Expr::WordRef { name, .. } => {
                // Other WordRefs: emit as-is.  Resolution against
                // the builtin/user dictionary happened earlier;
                // this is purely textual at this point.  For names
                // that need vocab prefixing (`cells`, `@`, etc.)
                // the user's source already had them resolved by
                // the time we got here.  We emit the same `forth.
                // runtime:` prefix that the rest of the emitter uses
                // for known forth.runtime words.
                emit_does_word(name, out);
            }
            // Literal strings and nested control flow inside a
            // does_body are deferred — neither is needed for the
            // common cell-array pattern.
            _ => {
                out.push_str("/* deferred-does-expr */");
            }
        }
    }
}

/// Emit a word name inside a does_body.  For names that live in
/// forth.runtime, prepend the vocab prefix so Factor's parser
/// resolves cleanly even though we're inside an emit-time-
/// constructed `:` body.
fn emit_does_word(name: &str, out: &mut String) {
    let lc = name.to_ascii_lowercase();
    let needs_prefix = matches!(lc.as_str(),
        "cells" | "chars" | "floats" | "cell+" | "char+"
        | "@" | "!" | "c@" | "c!" | "+!" | "f@" | "f!"
        | "nf-!" | "nf-c!" | "nf-+!" | "nf-f!"
        | "type" | "cmove" | "fill" | "bl"
    );
    if needs_prefix {
        write!(out, "forth.runtime:{lc}").unwrap();
    } else {
        out.push_str(name);
    }
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
///
/// Computed values (multi-token expressions like `3.5e 240e f/
/// FCONSTANT mb-dx`) can't go through CONSTANT: — that parser
/// only takes one token.  Emit them as `: name ( -- v ) body ;
/// inline` instead.  Factor's compiler folds pure-arithmetic
/// inline bodies to the same machine code as the literal form,
/// so there's no runtime cost.
fn emit_constant(c: &ConstantDef, r: &Sema, out: &mut String) {
    match &c.value {
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
        ConstValue::Computed(exprs) => {
            // Effect annotation: FCONSTANT produces a float, CONSTANT
            // produces an int.  Factor's strict effect checker doesn't
            // care about the type (both are one cell), but the named
            // slot makes the IR readable.
            let out_name = match c.flavour {
                ConstFlavour::Float => "f",
                ConstFlavour::Cell  => "n",
            };
            write!(out, ": {} ( -- {} ) ", c.name, out_name).unwrap();
            emit_exprs(exprs, r, out);
            write!(out, " ; inline").unwrap();
        }
    }
    let _ = c.flavour;  // Cell vs Float discriminator already reflected in value
}

fn emit_definition(d: &Definition, r: &Sema, out: &mut String) {
    // Emit the Factor-side mangled name (e.g. `->` → `nf-arrow`)
    // so collisions with Factor's parser tokens don't break
    // compilation.  Caller-side references go through the same
    // mangling in resolve.rs::factor_user_name.
    let factor_name = super::resolve::factor_user_name(
        &d.name.to_ascii_lowercase(),
    );
    write!(out, ": {} ", factor_name).unwrap();
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
    // Sanitize a stack-effect name for Factor's parser.  ANS lets
    // programmers write `...` to mean "any number of items"; Factor
    // doesn't accept `...` as a literal name token.  Replace with
    // `dots` (or any plausible identifier) so the IR parses.
    fn sanitize(name: &str) -> String {
        if name == "..." || name.contains('.') {
            // Replace dots with underscores; bare ... becomes "dots".
            if name == "..." { return "_dots_".to_string(); }
            return name.replace('.', "_");
        }
        name.to_string()
    }
    let emit_declared = |out: &mut String| {
        let eff = d.effect.as_ref().unwrap();
        out.push('(');
        for s in &eff.inputs {
            out.push(' ');
            out.push_str(&sanitize(s));
        }
        if eff.inputs.is_empty() { out.push(' '); }
        out.push_str(" --");
        for s in &eff.outputs {
            out.push(' ');
            out.push_str(&sanitize(s));
        }
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
    // ANS EXIT support.  Two-stage strategy:
    //
    //   1. Run `lower_exit::lower_body` to rewrite EXIT into
    //      structured tail-inlining at the AST level.  For the
    //      common case — EXIT at the top of a def body, or inside
    //      an IF / CASE arm whose enclosing scope eventually reaches
    //      the def body — this produces a pure structured-control-flow
    //      AST with NO references to `continuations:return`, so the
    //      emitted Factor IR is free of callcc0 and gets the full
    //      `compiler.tree` SSA / float-unboxing / inline-cache JIT.
    //
    //   2. Any EXIT that survives the transform must be inside a
    //      loop body (we leave loops opaque — see `lower_exit` for
    //      why).  For those, we still wrap the whole def in
    //      `[ ... ] continuations:with-return` as a correctness
    //      fallback.  The slow path is paid only per-def-that-needs-it,
    //      and only until the Rec 2 recursive-loop lowering lands.
    // Sema has already run lower_exit on every `:` body when this
    // Sema was built, so `d.body` is the lowered form.  Re-running
    // would be a no-op.  We just check whether EXIT survives (only
    // possible when it lives inside a loop body, which lower_exit
    // leaves opaque) — those still need the with-return wrap as
    // a correctness fallback.
    if super::lower_exit::body_uses_exit(&d.body, &r.word_targets) {
        out.push_str("[ ");
        emit_exprs(&d.body, r, out);
        out.push_str(" ] continuations:with-return");
    } else {
        emit_exprs(&d.body, r, out);
    }
    out.push_str(" ;");
}

// NB: EXIT detection used to live here as `body_uses_exit` /
// `expr_uses_exit`.  Both moved to `compiler::lower_exit` when the
// tail-inlining transform took over — emit.rs now calls
// `lower_exit::body_uses_exit` on the *lowered* body to decide
// whether the with-return fallback wrap is still required.

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
            // Emitting `{var_name} get-global + {var_name} set-global` avoids 
            // `change-global`'s strict nominal stack effect `( variable quot -- )`
            // which confuses Factor's inference in IF branches when the quot consumes 
            // a value from under the scope.
            write!(out, "{var_name} get-global + {var_name} set-global").unwrap();
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
                    // ANS `." text"` emits the literal at runtime.
                    // We use the dedicated `print-string` helper —
                    // a thin Factor `write` wrapper — rather than
                    // round-tripping the text through nf-addr just
                    // to feed `type`, since the user can't observe
                    // the address either way.
                    out.push('"');
                    out.push_str(&factor_escape(value));
                    out.push_str("\" forth.runtime:print-string");
                }
                StringKind::SQuote => {
                    // ANS `S" text"` produces (c-addr, u) on the
                    // data stack — TWO items.  `s-quote-runtime`
                    // materialises the Factor literal into a fresh
                    // nf-addr backed by a UTF-8 byte-array, plus
                    // the byte length.  GC'd; no PAD, no clobbering.
                    out.push('"');
                    out.push_str(&factor_escape(value));
                    out.push_str("\" forth.runtime:s-quote-runtime");
                }
                StringKind::SDollarQuote => {
                    // NewFactor managed-string literal (M2.x #43).
                    // Pushes a single Factor `string` handle — the
                    // GC-tracked, Unicode-aware, immutable string
                    // type.  All `$` vocab operates on these.
                    out.push('"');
                    out.push_str(&factor_escape(value));
                    out.push('"');
                }
                StringKind::CQuote => {
                    // ANS C" pushes a counted-string c-addr (length
                    // byte at addr, chars from addr+1).  Less
                    // commonly used than S"; deferred.  Falls
                    // through to bare-string for now.
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
            // ANS booleans are -1 (true) / 0 (false), but Factor's
            // `kernel:if` treats `0` as truthy (only `f` is false-y).
            // Bridge: prepend `math:zero?` to convert ANS flag to
            // Factor's t/f, then SWAP the branches so a `t` from
            // `zero?` (input was 0 = false) runs the else branch.
            //
            //   flag IF then ELSE else THEN
            //     →  flag math:zero? [ else ] [ then ] kernel:if
            //
            //   flag IF then THEN  (no else)
            //     →  flag math:zero? [ then ] kernel:unless
            //
            // `unless` runs the quotation when the input is FALSE-y,
            // which after `zero?` means "the flag was non-zero" =
            // ANS true.  Same semantics as IF with empty else.
            out.push_str("math:zero? ");
            if let Some(eb) = else_body {
                out.push_str("[ ");
                emit_exprs(eb, r, out);
                out.push_str(" ] [ ");
                emit_exprs(then_body, r, out);
                out.push_str(" ] kernel:if");
            } else {
                out.push_str("[ ");
                emit_exprs(then_body, r, out);
                out.push_str(" ] kernel:unless");
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

        // ' name pushes the XT as a one-element quotation
        // `[ name ]` — that's the form `call( -- )` (which our
        // ans-execute uses) reliably dispatches on.  Factor's
        // raw word object doesn't go through `call`'s polymorphic
        // path the way a quotation does.
        Expr::Tick { span, name } => {
            let target = r.word_targets.get(span)
                .map(|t| t.to_factor_token())
                .unwrap_or_else(|| name.clone());
            out.push_str("[ ");
            out.push_str(&target);
            out.push_str(" ]");
        }

        // LET form: lower to Factor `[| ... | ... ] call( ... )`
        // via the let_lang::codegen module.
        Expr::LetForm { form, .. } => {
            match super::let_lang::lower_to_factor(form) {
                Ok(ir) => out.push_str(&ir),
                Err(e) => {
                    // Codegen rejected the form post-parse.  Emit a
                    // visible diagnostic at runtime rather than
                    // silently producing wrong IR.
                    let escaped = e.replace('\\', "\\\\").replace('"', "\\\"");
                    let _ = write!(out,
                        "\"LET codegen error: {escaped}\" forth.runtime:print-string");
                }
            }
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
    fn dot_quote_emits_print_string() {
        // `." ..."` now emits via the dedicated print-string
        // helper (M2.10) — TYPE was reclaimed for ANS-correct
        // (c-addr u) semantics.
        let out = compile_str(": greet .\" hi\" ;");
        assert!(out.contains("\"hi\" forth.runtime:print-string"), "got {out}");
    }

    #[test]
    fn s_quote_emits_s_quote_runtime() {
        // S" now produces (nf-addr u) via s-quote-runtime, per ANS.
        let out = compile_str("s\" hi\" type");
        assert!(out.contains("\"hi\" forth.runtime:s-quote-runtime"), "got {out}");
        assert!(out.contains("forth.runtime:type"), "got {out}");
    }
}
