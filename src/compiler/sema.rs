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

use super::ast::{Definition, Expr, Item, Program};
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

    // ── From effect ──────────────────────────────────────────────
    pub user_effects:  HashMap<String, Effect>,
    pub effect_errors: Vec<EffectError>,

    // ── From sema::analyse ───────────────────────────────────────
    /// caller name → set of callee names (deduplicated).
    /// Keys are user words; callee names are whatever ANS name the
    /// caller wrote (lowercased), which may be a builtin or user word.
    pub call_graph: BTreeMap<String, BTreeSet<String>>,
    /// referenced name → spans of every reference in the program.
    pub use_sites: BTreeMap<String, Vec<Span>>,

    // ── From sema::escape (M2.8+) ────────────────────────────────
    pub escape: HashMap<String, EscapeState>,
}

// ─── Builder ───────────────────────────────────────────────────────────────

/// Build the full semantic model from a parsed program.  Runs each
/// sub-pass in order.  Resolve errors are hard fails (we can't
/// proceed without word mapping); effect errors are collected but
/// don't stop the build (callers may want to dump partial results).
pub fn build(program: Program) -> Result<Sema, ResolveError> {
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

    let mut sema = Sema {
        program: resolved.program,
        word_targets: resolved.word_targets,
        user_words,
        user_effects: inferred.user_effects,
        effect_errors,
        call_graph: BTreeMap::new(),
        use_sites: BTreeMap::new(),
        escape: HashMap::new(),
    };

    analyse_call_graph(&mut sema);
    // M2.8 will call analyse_escape(&mut sema) here.

    Ok(sema)
}

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
