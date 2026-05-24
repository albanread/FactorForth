//! Semantic model of the whole program.
//!
//! The compiler's earlier passes (lex, parse) produce data; `sema`
//! produces *knowledge*.  Every later pass — effect-check, emit,
//! variable narrowing (M2.8), CREATE-array narrowing (M2.9), inlining
//! decisions, dead-code elimination, IDE queries — consults this
//! struct rather than walking the AST again.
//!
//! ## Design choices
//!
//! - **Mutable across sub-passes.**  Each sub-pass owns one slot of
//!   the struct and writes into it.  Simpler dataflow than fully
//!   immutable; no explicit ordering machinery needed.  Trade-off:
//!   you can't trust a slot before its sub-pass has run.  Mitigation:
//!   keep the build pipeline (`build`) explicit so the order is
//!   visible in one place.
//!
//! - **Conservative on uncertain cases.**  Escape analysis: any
//!   unrecognised use of a variable's address marks it "wide" (slow
//!   nf-addr path).  Effect inference: any control flow node yields
//!   `Effect::Unknown` and the declared annotation is trusted.  False
//!   negatives are correct, just slower; false positives would be
//!   silently miscompiled and that's far worse.
//!
//! - **Queryable.**  The struct exposes its tables directly.  No
//!   hidden state, no setter machinery.  Downstream code (emit, the
//!   CLI's dump command, future IDE tooling) reads the fields.
//!
//! ## What lives where
//!
//! ```text
//! program            — the AST as parsed
//! word_targets       — per-WordRef span → emit target  (from resolve)
//! user_words         — name → def info                 (from resolve)
//! user_effects       — name → inferred stack effect    (from effect)
//! effect_errors      — declared-vs-inferred mismatches (from effect)
//! call_graph         — caller name → callee names      (from sema::analyse)
//! use_sites          — referenced name → ref spans     (from sema::analyse)
//! escape             — variable name → escape state    (from sema::escape, M2.8+)
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::ast::{
    CollectionDef, ConstantDef, CreateDef, Expr, Item, Literal, Program,
    TemplateDef, TemplateInstanceDef, VariableDef,
};
use super::error::Pos;
use super::effect::{infer, Effect, EffectError};
use super::error::Span;
use super::resolve::{resolve, ResolveError, Target};

// ─── Types ─────────────────────────────────────────────────────────────────

/// Conservative escape state for a variable's address.  Filled by
/// `sema::escape::analyse` (M2.8).  Defaulted to `Unknown` until that
/// pass runs.
#[derive(Clone, Debug, PartialEq)]
pub enum EscapeState {
    /// Every use is `@`, `!`, `+!`, `c@`, or `c!` — the address never
    /// flows anywhere else.  Emit can use the fast SYMBOL path.
    Narrow,
    /// At least one use escapes: dup'd, stored, address-arithmetic'd,
    /// passed to a user word, or otherwise observable as a value.
    /// Emit must use the nf-addr tuple path.
    Wide { reason: EscapeReason, at: Span },
    /// Not yet analysed.  Treat as Wide if you have to make a
    /// decision before the pass has run.
    Unknown,
}

/// Why a variable was marked Wide.  Drives the diagnostic the
/// `--dump=sema` output shows for any wide variable, so the user
/// can pinpoint which line of source forced the slow path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscapeReason {
    /// Address was DUP'd or OVER'd — a copy survives past the next sink.
    Duplicated,
    /// Address was passed to a user word whose behaviour is opaque
    /// to escape analysis (could store it, return it, etc.).
    PassedToUnknownWord,
    /// `cell+`, `char+`, `+` after the address — pointer arithmetic.
    AddressArithmetic,
    /// `,` (comma) or any non-`@`/`!` sink we don't recognise.
    UnknownSink,
    /// `.` or another I/O word saw the raw address.
    PrintedAsValue,
}

/// Info we keep about each user-defined word.
#[derive(Clone, Debug)]
pub struct UserWord {
    pub name: String,
    /// Span of the name token in the `:` line.
    pub def_span: Span,
    /// Declared `( a -- b )` annotation if present.
    pub declared_inputs:  Option<u32>,
    pub declared_outputs: Option<u32>,
}

/// The semantic database.
pub struct Sema {
    pub program: Program,

    // ── From resolve ─────────────────────────────────────────────
    pub word_targets: HashMap<Span, Target>,
    pub user_words:   HashMap<String, UserWord>,

    // ── M2.8 / M2.9 dictionary entries ───────────────────────────
    /// Variable name (lowercased) → its declaration.  Kept separate
    /// from `user_words` so escape analysis and emit can iterate
    /// over them without filtering.
    pub variables: BTreeMap<String, VariableDef>,
    /// Constant name (lowercased) → declaration (carries value).
    pub constants: BTreeMap<String, ConstantDef>,
    /// CREATE'd data buffer name (lowercased) → declaration.  Each
    /// CREATE allocates a byte-array backing store sized by ALLOT.
    pub creates: BTreeMap<String, CreateDef>,
    /// Standard-collection-defining-word instances (array, farray,
    /// cbuffer) keyed by lowercase name.  Sema's primary view of
    /// what user-named collections exist.
    pub collections: BTreeMap<String, CollectionDef>,
    /// User-defined CREATE/DOES> templates keyed by lowercase name.
    /// Each template can be instantiated at TopLevel by writing
    /// `<args> templatename <newname>`; expand_templates walks
    /// TopLevels and replaces those triples with TemplateInstance
    /// items.
    pub templates: BTreeMap<String, TemplateDef>,

    // ── From effect ──────────────────────────────────────────────
    /// Callers' view of a word's effect: declared if present, else
    /// inferred, else Unknown.  Use for typing other words.
    pub user_effects:  HashMap<String, Effect>,
    /// Body-derived effect — the ground truth from walking the
    /// definition's body.  Use to decide which annotation to emit
    /// when the user's declaration is potentially wrong.
    pub body_effects:  HashMap<String, Effect>,
    pub effect_errors: Vec<EffectError>,

    // ── From sema::analyse ───────────────────────────────────────
    /// caller name → set of callee names (deduplicated).
    /// Keys are user words; callee names are whatever ANS name the
    /// caller wrote (lowercased), which may be a builtin or user word.
    pub call_graph: BTreeMap<String, BTreeSet<String>>,
    /// referenced name → spans of every reference in the program.
    pub use_sites: BTreeMap<String, Vec<Span>>,

    // ── From sema::escape (M2.8) ─────────────────────────────────
    pub escape: HashMap<String, EscapeState>,
}

// ─── Builder ───────────────────────────────────────────────────────────────

/// Build the full semantic model from a parsed program.  Runs each
/// sub-pass in order.  Resolve errors are hard fails (we can't
/// proceed without word mapping); effect errors are collected but
/// don't stop the build (callers may want to dump partial results).
pub fn build(program: Program) -> Result<Sema, ResolveError> {
    // Template expansion runs BEFORE resolve.  Each `<n>
    // templatename <newname>` triple in TopLevel becomes an
    // Item::TemplateInstance.  resolve then sees a normal
    // program where `<newname>` is a registered user word.
    let program = expand_templates_pre_resolve(program);
    let resolved = resolve(program)?;
    let (inferred, effect_errors) = infer(&resolved);

    // Lift the resolve UserWord-equivalent (just name + span) into our
    // richer UserWord with declared effect counts.
    let mut user_words: HashMap<String, UserWord> = HashMap::new();
    for item in &resolved.program.items {
        if let Item::Definition(d) = item {
            let (di, do_) = match &d.effect {
                Some(se) => (Some(se.inputs.len() as u32),
                             Some(se.outputs.len() as u32)),
                None => (None, None),
            };
            user_words.insert(
                d.name.to_ascii_lowercase(),
                UserWord {
                    name: d.name.clone(),
                    def_span: d.name_span,
                    declared_inputs:  di,
                    declared_outputs: do_,
                },
            );
        }
    }

    // Collect variables, constants, and CREATE'd buffers into
    // their own tables.
    let mut variables: BTreeMap<String, VariableDef> = BTreeMap::new();
    let mut constants: BTreeMap<String, ConstantDef> = BTreeMap::new();
    let mut creates:   BTreeMap<String, CreateDef>   = BTreeMap::new();
    let mut collections: BTreeMap<String, CollectionDef> = BTreeMap::new();
    let mut templates: BTreeMap<String, TemplateDef> = BTreeMap::new();
    for item in &resolved.program.items {
        match item {
            Item::Variable(v) => {
                variables.insert(v.name.to_ascii_lowercase(), v.clone());
            }
            Item::Constant(c) => {
                constants.insert(c.name.to_ascii_lowercase(), c.clone());
            }
            Item::Create(cd) => {
                creates.insert(cd.name.to_ascii_lowercase(), cd.clone());
            }
            Item::Collection(cl) => {
                collections.insert(cl.name.to_ascii_lowercase(), cl.clone());
            }
            Item::Template(t) => {
                templates.insert(t.name.to_ascii_lowercase(), t.clone());
            }
            _ => {}
        }
    }

    let mut sema = Sema {
        program: resolved.program,
        word_targets: resolved.word_targets,
        user_words,
        variables,
        constants,
        creates,
        collections,
        templates,
        user_effects: inferred.user_effects,
        body_effects: inferred.body_effects,
        effect_errors,
        call_graph: BTreeMap::new(),
        use_sites: BTreeMap::new(),
        escape: HashMap::new(),
    };

    analyse_call_graph(&mut sema);
    analyse_escape(&mut sema);

    Ok(sema)
}

/// Standalone version of template expansion that runs BEFORE
/// resolve, so resolve sees the program with TemplateInstance
/// items already in place (giving `<newname>` a registered
/// dictionary entry).
fn expand_templates_pre_resolve(program: Program) -> Program {
    // Collect templates from the program first.
    let templates: BTreeMap<String, TemplateDef> = program.items.iter()
        .filter_map(|i| match i {
            Item::Template(t) => Some((t.name.to_ascii_lowercase(), t.clone())),
            _ => None,
        })
        .collect();
    if templates.is_empty() { return program; }

    let mut new_items: Vec<Item> = Vec::with_capacity(program.items.len());
    for item in program.items {
        match item {
            Item::TopLevel { exprs, span } => {
                expand_toplevel(exprs, span, &templates, &mut new_items);
            }
            other => new_items.push(other),
        }
    }
    Program { items: new_items }
}

fn expand_toplevel(
    exprs: Vec<Expr>,
    span: super::error::Span,
    templates: &BTreeMap<String, TemplateDef>,
    out: &mut Vec<Item>,
) {
    let mut current: Vec<Expr> = Vec::new();
    let mut i = 0;
    while i < exprs.len() {
        // Recognise template at position i?
        let tmpl = match &exprs[i] {
            Expr::WordRef { name, .. } => {
                templates.get(&name.to_ascii_lowercase())
            }
            _ => None,
        };
        if let Some(tmpl) = tmpl {
            // Need a literal int before and a WordRef after.
            let count_lit = current.last().and_then(|e| match e {
                Expr::Lit(Literal::Int { value, .. }) if *value >= 0 => Some(*value as u32),
                _ => None,
            });
            let name_after = exprs.get(i + 1).and_then(|e| match e {
                Expr::WordRef { name, span } => Some((name.clone(), *span)),
                _ => None,
            });
            if let (Some(count), Some((newname, newname_span))) = (count_lit, name_after) {
                // Consume the count from `current`.
                let count_expr = current.pop().unwrap();
                // Flush any leftover `current` as a TopLevel.
                flush_current(&mut current, out);
                // Compute allocated bytes from the template constructor.
                let bytes = compute_alloc_bytes(tmpl, count);
                out.push(Item::TemplateInstance(TemplateInstanceDef {
                    name: newname,
                    name_span: newname_span,
                    template_name: tmpl.name.clone(),
                    allocated_bytes: bytes,
                    does_body: tmpl.does_body.clone(),
                    span: super::error::Span {
                        start: count_expr.span().start,
                        end: newname_span.end,
                    },
                }));
                i += 2;  // skip template-name + new-name
                continue;
            }
            // Template name without the expected adjacents — treat
            // as a regular word reference (will likely error at
            // emit/runtime, but pass through here).
        }
        current.push(exprs[i].clone());
        i += 1;
    }
    flush_current(&mut current, out);
    let _ = span;  // not needed once we've split the TopLevel
}

fn flush_current(current: &mut Vec<Expr>, out: &mut Vec<Item>) {
    if current.is_empty() { return; }
    let first_span = current.first().unwrap().span();
    let last_span  = current.last().unwrap().span();
    out.push(Item::TopLevel {
        exprs: std::mem::take(current),
        span: super::error::Span {
            start: first_span.start,
            end: last_span.end,
        },
    });
}

/// Inspect a template's constructor to determine how many bytes
/// per "unit" the user wants.  First-cut grammar:
///   contains `cells` or `floats` → 8 bytes per unit
///   contains `chars`             → 1 byte  per unit
///   otherwise (raw `allot`)      → 1 byte per unit
fn compute_alloc_bytes(tmpl: &TemplateDef, count: u32) -> u32 {
    let mut unit: u32 = 1;
    for e in &tmpl.constructor {
        if let Expr::WordRef { name, .. } = e {
            let lc = name.to_ascii_lowercase();
            match lc.as_str() {
                "cells" | "floats" => unit = 8,
                "chars"            => unit = 1,
                _ => {}
            }
        }
    }
    count.saturating_mul(unit).max(1)
}

// Helper so file-level imports don't need extra modifications.
#[allow(dead_code)]
fn _unused_pos_import_anchor(_: Pos) {}

/// Whole-program walk: who calls whom, and where every word is referenced.
///
/// Conservative: any `Expr::WordRef` inside any definition body or
/// top-level is recorded.  We don't yet distinguish "called from
/// generic body" vs "called from a dead branch" — that requires
/// control-flow reachability analysis we don't do.
fn analyse_call_graph(sema: &mut Sema) {
    // Snapshot what we need so we can mutate `sema.call_graph` and
    // `sema.use_sites` while still walking `sema.program`.
    let items = sema.program.items.clone();
    for item in &items {
        match item {
            Item::Definition(d) => {
                let caller_lc = d.name.to_ascii_lowercase();
                walk_body_for_refs(&d.body, Some(&caller_lc), sema);
            }
            Item::TopLevel { exprs, .. } => {
                walk_body_for_refs(exprs, None, sema);
            }
            // Variable, Constant, Create, Collection carry no
            // expressions to walk.
            Item::Variable(_) | Item::Constant(_)
            | Item::Create(_) | Item::Collection(_) => {}
            // Templates carry constructor + does_body; instances
            // carry their captured does_body.
            Item::Template(t) => {
                let lc = t.name.to_ascii_lowercase();
                walk_body_for_refs(&t.constructor, Some(&lc), sema);
                walk_body_for_refs(&t.does_body,   Some(&lc), sema);
            }
            Item::TemplateInstance(ti) => {
                let lc = ti.name.to_ascii_lowercase();
                walk_body_for_refs(&ti.does_body, Some(&lc), sema);
            }
        }
    }
}

/// Variable escape analysis (M2.8).
///
/// For every `Item::Variable v`, walk every Expr list in the program
/// (definition bodies, top-level runs, inside any nested control
/// flow).  At each WordRef whose name matches `v.name`, look at the
/// *immediately following* expression in the same list:
///
///   - `@` / `c@`           narrow sink (fetch)
///   - `!` / `c!` / `+!`    narrow sink (store)
///   - anything else         escape — variable is wide
///
/// "Anything else" includes: end of list (last expression), the
/// next expression being a literal, a control-flow node, an
/// unknown word, or a user-defined word.  Conservative.  Better
/// to flag wide and miss an optimisation than narrow incorrectly
/// and miscompile.
///
/// First reference that escapes records the reason and span; we
/// don't enumerate every escaping use.
pub fn analyse_escape(sema: &mut Sema) {
    // Build the set of variable names we care about.
    let var_names: std::collections::BTreeSet<String> =
        sema.program.items.iter()
            .filter_map(|i| match i {
                Item::Variable(v) => Some(v.name.to_ascii_lowercase()),
                _ => None,
            })
            .collect();
    if var_names.is_empty() { return; }

    // Initialise everyone narrow; we'll demote to Wide on first
    // escape.  This way a variable referenced zero times is narrow
    // by default (vacuously true that every use is a narrow sink).
    for n in &var_names {
        sema.escape.insert(n.clone(), EscapeState::Narrow);
    }

    let items = sema.program.items.clone();
    for item in &items {
        match item {
            Item::Definition(d) => walk_block_for_escape(&d.body, &var_names, sema),
            Item::TopLevel { exprs, .. } => walk_block_for_escape(exprs, &var_names, sema),
            Item::Variable(_) | Item::Constant(_)
            | Item::Create(_) | Item::Collection(_) => {}
            Item::Template(t) => {
                walk_block_for_escape(&t.constructor, &var_names, sema);
                walk_block_for_escape(&t.does_body,   &var_names, sema);
            }
            Item::TemplateInstance(ti) => {
                walk_block_for_escape(&ti.does_body, &var_names, sema);
            }
        }
    }
}

fn walk_block_for_escape(
    exprs: &[Expr],
    var_names: &std::collections::BTreeSet<String>,
    sema: &mut Sema,
) {
    for (i, e) in exprs.iter().enumerate() {
        // Recurse into nested blocks first.  A nested block's own
        // start/end is a boundary — if a variable ref is the LAST
        // expression of an inner block, it's not "followed" by an
        // outer-block sink, so we mark it as escaping.
        match e {
            Expr::If { then_body, else_body, .. } => {
                walk_block_for_escape(then_body, var_names, sema);
                if let Some(eb) = else_body {
                    walk_block_for_escape(eb, var_names, sema);
                }
            }
            Expr::BeginUntil { body, .. }
            | Expr::BeginAgain { body, .. }
            | Expr::DoLoop { body, .. } => {
                walk_block_for_escape(body, var_names, sema);
            }
            Expr::BeginWhileRepeat { pred, body, .. } => {
                walk_block_for_escape(pred, var_names, sema);
                walk_block_for_escape(body, var_names, sema);
            }
            Expr::Case { arms, default, .. } => {
                for arm in arms {
                    walk_block_for_escape(&arm.match_expr, var_names, sema);
                    walk_block_for_escape(&arm.body, var_names, sema);
                }
                if let Some(d) = default {
                    walk_block_for_escape(d, var_names, sema);
                }
            }
            _ => {}
        }
        // Check this expression for a variable reference.
        let Expr::WordRef { name, span } = e else { continue };
        let lc = name.to_ascii_lowercase();
        if !var_names.contains(&lc) { continue; }
        // Look at the NEXT expression in this block.
        let reason = match exprs.get(i + 1) {
            Some(Expr::WordRef { name: next, .. }) => {
                let nlc = next.to_ascii_lowercase();
                if matches!(nlc.as_str(), "@" | "!" | "+!" | "c@" | "c!") {
                    continue; // narrow sink, no escape
                }
                EscapeReason::UnknownSink
            }
            Some(_) => EscapeReason::UnknownSink,
            None    => EscapeReason::UnknownSink,
        };
        // Only record the FIRST escaping site per variable.
        match sema.escape.get(&lc) {
            Some(EscapeState::Wide { .. }) => {}
            _ => {
                sema.escape.insert(lc, EscapeState::Wide { reason, at: *span });
            }
        }
    }
}

fn walk_body_for_refs(exprs: &[Expr], caller: Option<&str>, sema: &mut Sema) {
    for e in exprs {
        match e {
            Expr::Lit(_) => {}
            Expr::WordRef { name, span } => {
                let lc = name.to_ascii_lowercase();
                sema.use_sites.entry(lc.clone()).or_default().push(*span);
                if let Some(c) = caller {
                    sema.call_graph
                        .entry(c.to_string())
                        .or_default()
                        .insert(lc);
                }
            }
            Expr::If { then_body, else_body, .. } => {
                walk_body_for_refs(then_body, caller, sema);
                if let Some(eb) = else_body {
                    walk_body_for_refs(eb, caller, sema);
                }
            }
            Expr::BeginUntil { body, .. }
            | Expr::BeginAgain { body, .. }
            | Expr::DoLoop { body, .. } => {
                walk_body_for_refs(body, caller, sema);
            }
            Expr::BeginWhileRepeat { pred, body, .. } => {
                walk_body_for_refs(pred, caller, sema);
                walk_body_for_refs(body, caller, sema);
            }
            Expr::Case { arms, default, .. } => {
                for arm in arms {
                    walk_body_for_refs(&arm.match_expr, caller, sema);
                    walk_body_for_refs(&arm.body, caller, sema);
                }
                if let Some(d) = default {
                    walk_body_for_refs(d, caller, sema);
                }
            }
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{lex, parse};

    fn build_str(src: &str) -> Sema {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        build(prog).unwrap()
    }

    #[test]
    fn empty_sema_has_no_user_words() {
        let s = build_str("");
        assert!(s.user_words.is_empty());
        assert!(s.call_graph.is_empty());
    }

    #[test]
    fn call_graph_captures_user_call() {
        let s = build_str(": foo 1 + ; : bar foo ;");
        let bar_calls = s.call_graph.get("bar").expect("bar in graph");
        assert!(bar_calls.contains("foo"));
    }

    #[test]
    fn use_sites_count_all_refs() {
        let s = build_str(": square ( n -- ) dup * ; 5 square square");
        // square is referenced twice from top-level.
        let sites = s.use_sites.get("square").expect("square referenced");
        assert_eq!(sites.len(), 2);
    }

    #[test]
    fn user_word_carries_declared_effect() {
        let s = build_str(": foo ( a b -- c ) + ;");
        let u = s.user_words.get("foo").expect("foo defined");
        assert_eq!(u.declared_inputs, Some(2));
        assert_eq!(u.declared_outputs, Some(1));
    }

    #[test]
    fn nested_control_flow_walks_recursively() {
        let s = build_str(": foo dup 0 < if negate then ;");
        // `negate` is inside an IF body but should still appear in
        // foo's callee set.
        assert!(s.call_graph.get("foo").unwrap().contains("negate"));
    }
}
