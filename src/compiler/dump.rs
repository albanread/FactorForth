//! Phase-by-phase dumps.  Pretty-printers for every compiler stage,
//! producing human- *and* AI-readable text.  These exist so a human
//! debugging the compiler — or Claude reading state in a future
//! session without re-deriving everything from source — can ask
//! "what does the compiler see at this point?" and get a clear
//! answer.
//!
//! Each dump function writes to a `String` rather than to a `Write`
//! trait object — we accept the buffering cost for ergonomics, and
//! these dumps are bounded to a handful of pages per program.
//!
//! ## Formats
//!
//! The dumps follow a consistent shape:
//!
//! ```text
//! HEADER  (one-line summary)
//! ─────────────────────────────────────────
//!   content, indented as needed
//! ```
//!
//! Spans are rendered as `L:C-L:C` (1-based line:col half-open).
//! Lowercased names are noted as such when relevant.  Indented
//! blocks use two spaces per level; nested AST nodes show the same.
//!
//! ## Stages
//!
//! - `dump_tokens(&[Token])` — lex output
//! - `dump_ast(&Program)` — parse output
//! - `dump_sema(&Sema)` — semantic database
//! - `dump_effects(&Sema)` — focused on user-word effects
//! - `dump_ir(&str)` — emit output (raw Factor source)
//! - `dump_all(...)` — concatenate all stages with separators

use std::fmt::Write;

use super::ast::{CollectionKind, ConstValue, Expr, Item, Literal, Program};
use super::effect::Effect;
use super::error::Span;
use super::lex::{StringKind, Tok, Token};
use super::sema::{EscapeReason, EscapeState, Sema};

// ─── Tokens ────────────────────────────────────────────────────────────────

pub fn dump_tokens(tokens: &[Token]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "TOKENS  ({} token{})",
                     tokens.len(),
                     if tokens.len() == 1 { "" } else { "s" });
    let _ = writeln!(out, "{}", separator());
    if tokens.is_empty() {
        let _ = writeln!(out, "  (empty input)");
        return out;
    }
    for t in tokens {
        let pos = format!("{}:{}-{}:{}",
                          t.span.start.line, t.span.start.col,
                          t.span.end.line,   t.span.end.col);
        let (kind, value) = describe_tok(&t.kind);
        let _ = writeln!(out, "  {:<13} {:<11} {}", pos, kind, value);
    }
    out
}

fn describe_tok(t: &Tok) -> (&'static str, String) {
    match t {
        Tok::Word(s)            => ("Word",   format!("`{s}`")),
        Tok::Int { value, base, raw } =>
            ("Int",    format!("{value}  ({:?}, raw=`{raw}`)", base)),
        Tok::Float { value, raw } =>
            ("Float",  format!("{value}  (raw=`{raw}`)")),
        Tok::Str { value, kind } =>
            ("Str",    format!("{:?}  (kind={:?})", value, kind)),
        Tok::LineComment(s)     => ("Line\\",   format!("\"{s}\"")),
        Tok::BlockComment(s)    => ("Block(",   format!("\"{s}\"")),
        Tok::LetBlock(s)        =>
            ("LET",     format!("{} chars", s.len())),
    }
}

// ─── AST ───────────────────────────────────────────────────────────────────

pub fn dump_ast(prog: &Program) -> String {
    let mut out = String::new();
    let n_def = prog.items.iter().filter(|i| matches!(i, Item::Definition(_))).count();
    let n_top = prog.items.iter().filter(|i| matches!(i, Item::TopLevel { .. })).count();
    let _ = writeln!(out, "AST  ({} definition{}, {} top-level block{})",
                     n_def, if n_def == 1 { "" } else { "s" },
                     n_top, if n_top == 1 { "" } else { "s" });
    let _ = writeln!(out, "{}", separator());
    if prog.items.is_empty() {
        let _ = writeln!(out, "  (empty program)");
        return out;
    }
    for item in &prog.items {
        write_item(&mut out, item, 1);
        let _ = writeln!(out);
    }
    out
}

fn write_item(out: &mut String, item: &Item, depth: usize) {
    let indent = "  ".repeat(depth);
    match item {
        Item::Definition(d) => {
            let _ = writeln!(out, "{indent}Definition `{}` @ {}",
                             d.name, span_str(&d.span));
            if let Some(e) = &d.effect {
                let _ = writeln!(out, "{indent}  effect: ( {} -- {} )",
                                 e.inputs.join(" "),
                                 e.outputs.join(" "));
            }
            for be in &d.body {
                write_expr(out, be, depth + 1);
            }
        }
        Item::TopLevel { exprs, span } => {
            let _ = writeln!(out, "{indent}TopLevel @ {}", span_str(span));
            for e in exprs { write_expr(out, e, depth + 1); }
        }
        Item::Variable(v) => {
            let _ = writeln!(out, "{indent}Variable `{}` @ {}",
                             v.name, span_str(&v.span));
        }
        Item::Constant(c) => {
            let val = match &c.value {
                ConstValue::Int(i)   => format!("Int {i}"),
                ConstValue::Float(f) => format!("Float {f}"),
                ConstValue::Computed(exprs) =>
                    format!("Computed ({} exprs)", exprs.len()),
            };
            let _ = writeln!(out, "{indent}Constant `{}` = {val} ({:?}) @ {}",
                             c.name, c.flavour, span_str(&c.span));
        }
        Item::Create(cd) => {
            let _ = writeln!(out, "{indent}Create `{}` allotted {} byte{} @ {}",
                             cd.name, cd.allotted_bytes,
                             if cd.allotted_bytes == 1 { "" } else { "s" },
                             span_str(&cd.span));
        }
        Item::Collection(cl) => {
            let _ = writeln!(out, "{indent}{} `{}` ({} element{}) @ {}",
                             match cl.kind {
                                 CollectionKind::Array   => "Array",
                                 CollectionKind::FArray  => "FArray",
                                 CollectionKind::CBuffer => "CBuffer",
                             },
                             cl.name, cl.count,
                             if cl.count == 1 { "" } else { "s" },
                             span_str(&cl.span));
        }
        Item::Template(t) => {
            let _ = writeln!(out, "{indent}Template `{}` ({} ctor, {} does) @ {}",
                             t.name, t.constructor.len(), t.does_body.len(),
                             span_str(&t.span));
        }
        Item::TemplateInstance(ti) => {
            let _ = writeln!(out, "{indent}TemplateInstance `{}` from `{}` ({} bytes) @ {}",
                             ti.name, ti.template_name, ti.allocated_bytes,
                             span_str(&ti.span));
        }
        Item::Value(v) => {
            let _ = writeln!(out, "{indent}Value `{}` ({} init expr{}) @ {}",
                             v.name, v.initial.len(),
                             if v.initial.len() == 1 { "" } else { "s" },
                             span_str(&v.span));
            for e in &v.initial { write_expr(out, e, depth + 1); }
        }
        Item::Class(c) => {
            let _ = writeln!(out, "{indent}Class `{}`{}{} ({} slot{}) @ {}",
                             c.name,
                             if c.extends.is_some() { " EXTENDS " } else { "" },
                             c.extends.as_deref().unwrap_or(""),
                             c.slots.len(),
                             if c.slots.len() == 1 { "" } else { "s" },
                             span_str(&c.span));
        }
        Item::Generic(g) => {
            let _ = writeln!(out, "{indent}Generic `{}` ( {} -- {} ) @ {}",
                             g.name,
                             g.effect.inputs.join(" "),
                             g.effect.outputs.join(" "),
                             span_str(&g.span));
        }
        Item::Method(m) => {
            let specs: Vec<String> = m.specializers.iter()
                .map(|s| format!("{}:{}", s.param_name, s.class_name))
                .collect();
            let _ = writeln!(out, "{indent}Method `{}` [{}] ({} body expr{}) @ {}",
                             m.generic_name, specs.join(", "),
                             m.body.len(),
                             if m.body.len() == 1 { "" } else { "s" },
                             span_str(&m.span));
            for e in &m.body { write_expr(out, e, depth + 1); }
        }
        Item::RawFactor(r) => {
            let preview: String = r.source.chars().take(40).collect();
            let _ = writeln!(out, "{indent}RawFactor `{}{}` @ {}",
                             preview,
                             if r.source.len() > 40 { "…" } else { "" },
                             span_str(&r.span));
        }
    }
}

fn write_expr(out: &mut String, e: &Expr, depth: usize) {
    let indent = "  ".repeat(depth);
    match e {
        Expr::Lit(Literal::Int   { value, span }) =>
            { let _ = writeln!(out, "{indent}Int {value} @ {}", span_str(span)); }
        Expr::Lit(Literal::Float { value, span }) =>
            { let _ = writeln!(out, "{indent}Float {value} @ {}", span_str(span)); }
        Expr::Lit(Literal::Str   { value, kind, span }) =>
            { let _ = writeln!(out, "{indent}Str ({}) {:?} @ {}",
                               str_kind_short(kind), value, span_str(span)); }
        Expr::WordRef { name, span } =>
            { let _ = writeln!(out, "{indent}WordRef `{name}` @ {}", span_str(span)); }
        Expr::If { then_body, else_body, span } => {
            let _ = writeln!(out, "{indent}If @ {}", span_str(span));
            let _ = writeln!(out, "{indent}  then:");
            for be in then_body { write_expr(out, be, depth + 2); }
            if let Some(eb) = else_body {
                let _ = writeln!(out, "{indent}  else:");
                for be in eb { write_expr(out, be, depth + 2); }
            }
        }
        Expr::BeginUntil { body, span } => {
            let _ = writeln!(out, "{indent}BeginUntil @ {}", span_str(span));
            for be in body { write_expr(out, be, depth + 1); }
        }
        Expr::BeginWhileRepeat { pred, body, span } => {
            let _ = writeln!(out, "{indent}BeginWhileRepeat @ {}", span_str(span));
            let _ = writeln!(out, "{indent}  pred:");
            for be in pred { write_expr(out, be, depth + 2); }
            let _ = writeln!(out, "{indent}  body:");
            for be in body { write_expr(out, be, depth + 2); }
        }
        Expr::BeginAgain { body, span } => {
            let _ = writeln!(out, "{indent}BeginAgain @ {}", span_str(span));
            for be in body { write_expr(out, be, depth + 1); }
        }
        Expr::DoLoop { is_qdo, body, loop_kind, span } => {
            let _ = writeln!(out, "{indent}DoLoop{} ({:?}) @ {}",
                             if *is_qdo { " (?do)" } else { "" },
                             loop_kind, span_str(span));
            for be in body { write_expr(out, be, depth + 1); }
        }
        Expr::Case { arms, default, span } => {
            let _ = writeln!(out, "{indent}Case @ {}", span_str(span));
            for (i, arm) in arms.iter().enumerate() {
                let _ = writeln!(out, "{indent}  arm[{i}] @ {}", span_str(&arm.span));
                let _ = writeln!(out, "{indent}    match:");
                for be in &arm.match_expr { write_expr(out, be, depth + 3); }
                let _ = writeln!(out, "{indent}    body:");
                for be in &arm.body { write_expr(out, be, depth + 3); }
            }
            if let Some(d) = default {
                let _ = writeln!(out, "{indent}  default:");
                for be in d { write_expr(out, be, depth + 2); }
            }
        }
        Expr::Tick { name, span } => {
            let _ = writeln!(out, "{indent}Tick `{name}` @ {}", span_str(span));
        }
        Expr::To { name, span } => {
            let _ = writeln!(out, "{indent}To `{name}` @ {}", span_str(span));
        }
        Expr::LetForm { form, span } => {
            let _ = writeln!(out, "{indent}LetForm @ {}", span_str(span));
            let _ = writeln!(out, "{indent}  inputs:  {:?}", form.inputs);
            let _ = writeln!(out, "{indent}  outputs: {:?}", form.outputs);
            let _ = writeln!(out, "{indent}  {} results, {} where-bindings",
                             form.results.len(), form.wheres.len());
        }
    }
}

fn str_kind_short(k: &StringKind) -> &'static str {
    match k {
        StringKind::DotQuote     => ".\"",
        StringKind::SQuote       => "S\"",
        StringKind::CQuote       => "C\"",
        StringKind::SDollarQuote => "S$\"",
    }
}

// ─── Sema ──────────────────────────────────────────────────────────────────

pub fn dump_sema(s: &Sema) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "SEMA");
    let _ = writeln!(out, "{}", separator_thick());

    // User words
    let _ = writeln!(out, "User words ({}):", s.user_words.len());
    if s.user_words.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        let mut names: Vec<&String> = s.user_words.keys().collect();
        names.sort();
        for name in names {
            let u = &s.user_words[name];
            let decl = match (u.declared_inputs, u.declared_outputs) {
                (Some(i), Some(o)) => format!("declared ( {i} → {o} )"),
                _ => "(no declared effect)".into(),
            };
            let infer = match s.user_effects.get(name).copied() {
                Some(Effect::Known { inputs, outputs }) =>
                    format!("inferred ( {inputs} → {outputs} )"),
                Some(Effect::Unknown) => "inferred ( ? — control flow )".into(),
                None                  => "inferred (—)".into(),
            };
            let _ = writeln!(out, "  {:<16} {:<28} {}", u.name, decl, infer);
            let _ = writeln!(out, "                     def @ {}", span_str(&u.def_span));
        }
    }
    let _ = writeln!(out);

    // Effect errors
    if !s.effect_errors.is_empty() {
        let _ = writeln!(out, "Effect errors ({}):", s.effect_errors.len());
        for e in &s.effect_errors {
            let _ = writeln!(out, "  {e}");
        }
        let _ = writeln!(out);
    }

    // Variables
    let _ = writeln!(out, "Variables ({}):", s.variables.len());
    if s.variables.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (name, v) in &s.variables {
            let escape = match s.escape.get(name) {
                Some(EscapeState::Narrow) => " [narrow]",
                Some(EscapeState::Wide { reason, .. }) => match reason {
                    EscapeReason::Duplicated          => " [wide: duplicated]",
                    EscapeReason::PassedToUnknownWord => " [wide: passed to unknown word]",
                    EscapeReason::AddressArithmetic   => " [wide: address arithmetic]",
                    EscapeReason::UnknownSink         => " [wide: unknown sink]",
                    EscapeReason::PrintedAsValue      => " [wide: printed]",
                    EscapeReason::InteractiveSession  => " [wide: REPL / compile_in_context]",
                },
                Some(EscapeState::Unknown) | None => " [unknown]",
            };
            let _ = writeln!(out, "  {:<16} def @ {}{escape}",
                             v.name, span_str(&v.span));
        }
    }
    let _ = writeln!(out);

    // Constants
    let _ = writeln!(out, "Constants ({}):", s.constants.len());
    if s.constants.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (_, c) in &s.constants {
            let val = match &c.value {
                ConstValue::Int(i)   => format!("{i}"),
                ConstValue::Float(f) => format!("{f}"),
                ConstValue::Computed(exprs) =>
                    format!("<computed: {} expr(s)>", exprs.len()),
            };
            let _ = writeln!(out, "  {:<16} = {val:<10} ({:?}) @ {}",
                             c.name, c.flavour, span_str(&c.span));
        }
    }
    let _ = writeln!(out);

    // CREATE'd data buffers
    let _ = writeln!(out, "CREATEs ({}):", s.creates.len());
    if s.creates.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (_, cd) in &s.creates {
            let _ = writeln!(out, "  {:<16} {} byte{} @ {}",
                             cd.name, cd.allotted_bytes,
                             if cd.allotted_bytes == 1 { "" } else { "s" },
                             span_str(&cd.span));
        }
    }
    let _ = writeln!(out);

    // Standard collections (array / farray / cbuffer)
    let _ = writeln!(out, "Collections ({}):", s.collections.len());
    if s.collections.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (_, cl) in &s.collections {
            let _ = writeln!(out, "  {:<16} {:<8} ×{:<6} ({} bytes) @ {}",
                             cl.name,
                             cl.kind.keyword(),
                             cl.count,
                             cl.count.saturating_mul(cl.kind.elt_size()),
                             span_str(&cl.span));
        }
    }
    let _ = writeln!(out);

    // Call graph
    let _ = writeln!(out, "Call graph:");
    if s.call_graph.is_empty() {
        let _ = writeln!(out, "  (no user words call anything)");
    } else {
        for (caller, callees) in &s.call_graph {
            if callees.is_empty() {
                let _ = writeln!(out, "  {caller}  →  (no calls)");
            } else {
                let list: Vec<String> = callees.iter().cloned().collect();
                let _ = writeln!(out, "  {caller}  →  {}", list.join(", "));
            }
        }
    }
    let _ = writeln!(out);

    // Use sites
    let _ = writeln!(out, "Use sites:");
    if s.use_sites.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (name, sites) in &s.use_sites {
            let spans: Vec<String> = sites.iter().map(span_str).collect();
            let _ = writeln!(out, "  {:<16} @ {}", name, spans.join(", "));
        }
    }
    let _ = writeln!(out);

    // Escape state (M2.8)
    let _ = writeln!(out, "Escape analysis:");
    if s.escape.is_empty() {
        let _ = writeln!(out, "  (M2.8 — not yet collected)");
    } else {
        for (name, state) in &s.escape {
            match state {
                EscapeState::Narrow =>
                    { let _ = writeln!(out, "  {name:<16} narrow"); }
                EscapeState::Wide { reason, at } =>
                    { let _ = writeln!(out, "  {name:<16} wide ({reason:?} @ {})", span_str(at)); }
                EscapeState::Unknown =>
                    { let _ = writeln!(out, "  {name:<16} unknown"); }
            }
        }
    }

    out
}

// ─── Effects (focused subset of sema) ──────────────────────────────────────

pub fn dump_effects(s: &Sema) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "EFFECTS  ({} user word{})",
                     s.user_effects.len(),
                     if s.user_effects.len() == 1 { "" } else { "s" });
    let _ = writeln!(out, "{}", separator());
    if s.user_effects.is_empty() {
        let _ = writeln!(out, "  (no user words)");
        return out;
    }
    let mut names: Vec<&String> = s.user_effects.keys().collect();
    names.sort();
    for name in names {
        let eff = s.user_effects[name];
        let _ = writeln!(out, "  {name:<16} {eff}");
    }
    if !s.effect_errors.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Errors:");
        for e in &s.effect_errors {
            let _ = writeln!(out, "  {e}");
        }
    }
    out
}

// ─── IR ────────────────────────────────────────────────────────────────────

pub fn dump_ir(ir: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "FACTOR IR  ({} byte{})", ir.len(),
                     if ir.len() == 1 { "" } else { "s" });
    let _ = writeln!(out, "{}", separator());
    for line in ir.lines() {
        let _ = writeln!(out, "  {line}");
    }
    out
}

// ─── All ───────────────────────────────────────────────────────────────────

/// Concatenate every available dump with thick separators between
/// stages.  `ir` is optional — emit may not have run, or the caller
/// may want sema-only output.
pub fn dump_all(
    tokens: &[Token],
    prog: &Program,
    sema: &Sema,
    ir: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(&dump_tokens(tokens));
    out.push_str("\n\n");
    out.push_str(&dump_ast(prog));
    out.push_str("\n");
    out.push_str(&dump_sema(sema));
    out.push_str("\n");
    if let Some(ir) = ir {
        out.push_str(&dump_ir(ir));
    }
    out
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn span_str(s: &Span) -> String {
    format!("{}:{}-{}:{}",
            s.start.line, s.start.col,
            s.end.line,   s.end.col)
}

fn separator() -> &'static str {
    "─────────────────────────────────────────"
}

fn separator_thick() -> &'static str {
    "═════════════════════════════════════════"
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{lex, parse, sema::build};

    fn dumps(src: &str) -> (String, String, String, String) {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        let sema = build(prog.clone()).unwrap();
        (
            dump_tokens(&toks),
            dump_ast(&prog),
            dump_sema(&sema),
            dump_effects(&sema),
        )
    }

    #[test]
    fn empty_program_dumps_cleanly() {
        let (t, a, s, e) = dumps("");
        assert!(t.contains("0 tokens"));
        assert!(a.contains("empty program") || a.contains("0 definition"));
        assert!(s.contains("User words (0)"));
        assert!(e.contains("0 user words") || e.contains("no user words"));
    }

    #[test]
    fn small_program_dumps_have_useful_content() {
        let (t, a, s, e) = dumps(": square ( n -- n^2 ) dup * ; 5 square .");
        // Tokens
        assert!(t.contains("Word") && t.contains("square"));
        // AST: a Definition named square with a body
        assert!(a.contains("Definition `square`"));
        assert!(a.contains("WordRef `dup`"));
        assert!(a.contains("TopLevel"));
        // Sema: user word + effect + call graph + use sites
        assert!(s.contains("square"));
        assert!(s.contains("Call graph"));
        assert!(s.contains("Use sites"));
        // Effects: square's inferred effect
        assert!(e.contains("square"));
    }

    #[test]
    fn ast_dump_shows_control_flow_structure() {
        let (_, a, _, _) = dumps(": foo dup 0 < if negate then ;");
        assert!(a.contains("If"), "expected If node in:\n{a}");
        assert!(a.contains("then:"));
    }

    #[test]
    fn dump_all_concatenates() {
        let toks = lex("42 .").unwrap();
        let prog = parse(&toks).unwrap();
        let sema = build(prog.clone()).unwrap();
        let all = dump_all(&toks, &prog, &sema, Some(": dummy ir ;"));
        assert!(all.contains("TOKENS"));
        assert!(all.contains("AST"));
        assert!(all.contains("SEMA"));
        assert!(all.contains("FACTOR IR"));
    }
}
