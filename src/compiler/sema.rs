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
    TemplateDef, TemplateInstanceDef, ValueDef, VariableDef,
};
use super::error::Pos;
use super::effect::{infer_with_prior, Effect, EffectError};
use super::error::Span;
use super::resolve::{resolve_with_prior, ResolveError, Target};

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
    /// Forced wide by the compile driver because the variable
    /// is in a REPL context — subsequent evals can reference it
    /// and we can't see those uses now.  See #52.
    InteractiveSession,
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
    /// Lowercase VALUE name → full ValueDef.  Mirrors `variables` /
    /// `constants` / `templates`.  Used by emit (to seed the storage
    /// symbol and emit the reader word) and exposed to callers so
    /// they can merge new VALUE names into a persistent
    /// `CompileContext.values` for cross-eval TO resolution.
    pub values: BTreeMap<String, ValueDef>,

    /// Lowercase class name → flattened slot list (parent slots
    /// first, then own slots).  Used by emit to size constructors
    /// correctly and to emit accessors for every slot a class
    /// instance has, not just the ones declared on this class.
    ///
    /// Populated by `lower_classes::compute_class_slots` at sema
    /// build time.  Cross-eval class persistence (sprint 2) will
    /// extend this with prior_classes from the CompileContext.
    pub class_slots: HashMap<String, Vec<String>>,

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

    // ── For SEE introspection ────────────────────────────────────
    /// Lowercase name → its introspection record (kind, effect,
    /// source, detail).  Populated by the driver in `mod.rs` after
    /// the build (it needs the original source text, which the
    /// sema build doesn't carry), seeded with prior-eval docs from
    /// `CompileContext.docs` so `SEE` works cross-eval.  Read by
    /// emit's `Expr::See` arm.
    pub docs: HashMap<String, super::ast::WordDoc>,
}

// ─── Builder ───────────────────────────────────────────────────────────────

/// Build the full semantic model from a parsed program.  Runs each
/// sub-pass in order.  Resolve errors are hard fails (we can't
/// proceed without word mapping); effect errors are collected but
/// don't stop the build (callers may want to dump partial results).
pub fn build(program: Program) -> Result<Sema, ResolveError> {
    let empty_names = HashMap::new();
    let empty_effects = HashMap::new();
    build_with_prior(program, &empty_names, &empty_effects)
}

/// Same as [`build`] but seeded with names AND effect info for
/// words defined in prior compiles within this interactive
/// session.  See [`super::resolve::resolve_with_prior`] and
/// [`super::effect::infer_with_prior`].
pub fn build_with_prior(
    program: Program,
    prior_user_words: &HashMap<String, Span>,
    prior_effects: &HashMap<String, Effect>,
) -> Result<Sema, ResolveError> {
    let empty = BTreeMap::new();
    build_with_prior_and_templates(program, prior_user_words, prior_effects, &empty)
}

/// As above, plus templates from prior compiles.  A `template`
/// defined in eval 1 needs to be visible to eval 2's
/// `<n> templatename <newname>` triples so they expand to
/// `Item::TemplateInstance` correctly.
pub fn build_with_prior_and_templates(
    program: Program,
    prior_user_words: &HashMap<String, Span>,
    prior_effects: &HashMap<String, Effect>,
    prior_templates: &BTreeMap<String, TemplateDef>,
) -> Result<Sema, ResolveError> {
    let empty_values = HashMap::new();
    let empty_classes = HashMap::new();
    build_with_prior_state(
        program, prior_user_words, prior_effects, prior_templates,
        &empty_values, &empty_classes,
    )
}

/// As above, plus VALUE names and class slot-lists carried over
/// from prior compiles.  `TO` targets that aren't in either this
/// compile's VALUE items or `prior_values` are rejected with
/// `ToNotValue`.  Class constructors and accessors referenced from
/// this compile can resolve against names declared in
/// `prior_classes` even when the `CLASS:` itself isn't redeclared.
pub fn build_with_prior_state(
    program: Program,
    prior_user_words: &HashMap<String, Span>,
    prior_effects: &HashMap<String, Effect>,
    prior_templates: &BTreeMap<String, TemplateDef>,
    prior_values: &HashMap<String, Span>,
    prior_classes: &HashMap<String, Vec<String>>,
) -> Result<Sema, ResolveError> {
    // Template expansion runs BEFORE resolve.  Each `<n>
    // templatename <newname>` triple in TopLevel becomes an
    // Item::TemplateInstance.  resolve then sees a normal
    // program where `<newname>` is a registered user word.
    let program = expand_templates_pre_resolve_with_prior(program, prior_templates);

    // ── lower_qdup ───────────────────────────────────────────────
    // Rewrite `?DUP IF ... THEN` into `DUP IF ... ELSE DROP THEN`
    // before resolve sees the AST.  `?dup` has no Factor-compilable
    // body (polymorphic effect), and Factor's strict effect checker
    // would reject any runtime word with that shape.  The peephole
    // rewrite produces balanced branches that compile to zero-cost
    // machine code on the JIT fast path.  See compiler::lower_qdup.
    let program = super::lower_qdup::lower_program(program);

    // ── lower_recurse ────────────────────────────────────────────
    // Bind every `RECURSE` reference inside a `:` body to the
    // enclosing definition's own name.  Resolve then handles it as
    // an ordinary self-call (pass-1 has already registered the
    // def's name in user_words).  Definitions that use RECURSE
    // without a stack-effect annotation get flagged here and
    // escalated to a sema-level error below.
    let (program, missing_recurse_effects) = super::lower_recurse::lower_program(program);
    if let Some(first) = missing_recurse_effects.first() {
        return Err(ResolveError::RecurseNeedsEffect {
            word: first.word_name.clone(),
            at: first.at,
        });
    }

    // Compute the flattened slot list per class BEFORE resolve so
    // resolve can register the right number of accessor names.
    // Prior-compile classes are folded in so cross-eval inheritance
    // and accessor reuse Just Work — a class defined in eval N
    // contributes its slot list, and eval N+1's `<oldname>` /
    // `oldname>slot` / `slot>>oldname` resolve correctly.
    let class_slots = super::lower_classes::compute_class_slots(
        &program,
        prior_classes,
    );

    let mut resolved = super::resolve::resolve_with_prior_and_values_and_classes(
        program, prior_user_words, prior_values, &class_slots,
    )?;

    // ── lower_exit ───────────────────────────────────────────────
    // Rewrite ANS EXIT into structured tail-inlining before any
    // downstream pass sees the bodies.  Two payoffs:
    //
    //   * Effect inference (next pass) sees concrete bodies without
    //     the `( -- * )` shape that EXIT-as-continuations:return
    //     would otherwise paint over the synth.  Definitions whose
    //     only "control flow" was an EXIT-shaped early bail-out now
    //     get a clean `Known` inference and emit a proper
    //     `( a -- b )` annotation instead of `( ..a -- ..b )`.
    //
    //   * Emit can skip the `[ ... ] continuations:with-return`
    //     wrap entirely (no callcc0 → full `compiler.tree` JIT).
    //
    // EXIT inside a loop body is left alone — the with-return
    // fallback in emit catches it.  See compiler::lower_exit for
    // the algorithm and its scope of correctness.
    for item in resolved.program.items.iter_mut() {
        if let Item::Definition(d) = item {
            d.body = super::lower_exit::lower_body(&d.body, &resolved.word_targets);
        }
    }

    let (mut inferred, effect_errors) = infer_with_prior(&resolved, prior_effects);

    // Seed user_effects with the per-class synthesised words.
    // These have concrete known effects derivable from slot count,
    // but effect.rs doesn't have access to the slot map — sema does.
    // Without this seeding, eval N+1's `5 6 <oldpoint>` call would
    // see the constructor as Effect::Unknown and Factor would refuse
    // the IR.
    for item in &resolved.program.items {
        if let Item::Class(c) = item {
            let class_lc = c.name.to_ascii_lowercase();
            let empty: Vec<String> = Vec::new();
            let all_slots = class_slots.get(&class_lc).unwrap_or(&empty);
            let n_slots = all_slots.len() as u32;
            inferred.user_effects.insert(
                format!("<{class_lc}>"),
                Effect::known(n_slots, 1),
            );
            for s in all_slots {
                inferred.user_effects.insert(
                    format!("{class_lc}>{s}"),
                    Effect::known(1, 1),
                );
                inferred.user_effects.insert(
                    format!("{s}>>{class_lc}"),
                    Effect::known(2, 1),
                );
                inferred.user_effects.insert(
                    format!("{class_lc}.{s}!"),
                    Effect::known(2, 0),
                );
            }
        }
    }

    // Lift the resolve UserWord-equivalent (just name + span) into our
    // richer UserWord with declared effect counts.
    let mut user_words: HashMap<String, UserWord> = HashMap::new();
    for item in &resolved.program.items {
        let (name, name_span, di, do_) = match item {
            Item::Definition(d) => {
                let (di, do_) = match &d.effect {
                    Some(se) => (Some(se.inputs.len() as u32),
                                 Some(se.outputs.len() as u32)),
                    None => (None, None),
                };
                (d.name.clone(), d.name_span, di, do_)
            }
            // All other kinds of "name introduction" register the
            // name so cross-compile lookup sees them, but carry no
            // declared stack-effect info (variables push an addr,
            // constants push their value, collections take an idx —
            // each has a known intrinsic effect that doesn't need
            // a user-declared `( -- )` annotation).
            Item::Variable(v)          => (v.name.clone(), v.name_span, None, None),
            Item::Constant(c)          => (c.name.clone(), c.name_span, None, None),
            Item::Create(cd)           => (cd.name.clone(), cd.name_span, None, None),
            Item::Collection(cl)       => (cl.name.clone(), cl.name_span, None, None),
            Item::Template(t)          => (t.name.clone(), t.name_span, None, None),
            Item::TemplateInstance(ti) => (ti.name.clone(), ti.name_span, None, None),
            // VALUE emits as `: name ( -- v ) ... ;` so callers see
            // it like a `:` def with effect (0, 1).  Record it
            // with that declared shape so cross-compile inference
            // doesn't fall back to Unknown.
            Item::Value(v)             => (v.name.clone(), v.name_span, Some(0), Some(1)),
            // Class registers its own name as (0, 0) — bare class-name
            // references don't produce stack effects.  Constructor and
            // accessor synth names are registered separately below.
            Item::Class(c)             => (c.name.clone(), c.name_span, Some(0), Some(0)),
            // Generic name carries its declared effect.
            Item::Generic(g) => (
                g.name.clone(), g.name_span,
                Some(g.effect.inputs.len() as u32),
                Some(g.effect.outputs.len() as u32),
            ),
            // Method extends an existing generic; no separate name.
            Item::Method(_)            => continue,
            // Raw Factor injection: no Forth-visible name.
            Item::RawFactor(_)         => continue,
            Item::TopLevel { .. }      => continue,
        };
        user_words.insert(
            name.to_ascii_lowercase(),
            UserWord {
                name,
                def_span: name_span,
                declared_inputs:  di,
                declared_outputs: do_,
            },
        );
    }

    // For every class declared in this compile, ALSO register the
    // synthesised constructor and accessor names in user_words
    // (and seed user_effects below).  Without this, the names
    // resolve fine THIS compile (resolve pass 1 inserts them into
    // its own user_words map) but they don't propagate to
    // CompileContext.user_words for the next eval — which means
    // `<point>` defined in eval N is invisible to eval N+1.
    //
    // For each class, all synth names are derived from the FLAT
    // slot list (parent + own).  Effects:
    //   <classname>           : (n_slots, 1)
    //   classname>slot        : (1, 1)
    //   slot>>classname       : (2, 1)
    //   classname.slot!       : (2, 0)
    for item in &resolved.program.items {
        if let Item::Class(c) = item {
            let class_lc = c.name.to_ascii_lowercase();
            let empty: Vec<String> = Vec::new();
            let all_slots = class_slots.get(&class_lc).unwrap_or(&empty);
            let n_slots = all_slots.len() as u32;
            // Constructor: <classname>
            user_words.entry(format!("<{class_lc}>")).or_insert(UserWord {
                name: format!("<{}>", c.name),
                def_span: c.name_span,
                declared_inputs:  Some(n_slots),
                declared_outputs: Some(1),
            });
            // Per-slot accessors.
            for s in all_slots {
                user_words.entry(format!("{class_lc}>{s}")).or_insert(UserWord {
                    name: format!("{}>{s}", c.name),
                    def_span: c.name_span,
                    declared_inputs:  Some(1),
                    declared_outputs: Some(1),
                });
                user_words.entry(format!("{s}>>{class_lc}")).or_insert(UserWord {
                    name: format!("{s}>>{}", c.name),
                    def_span: c.name_span,
                    declared_inputs:  Some(2),
                    declared_outputs: Some(1),
                });
                user_words.entry(format!("{class_lc}.{s}!")).or_insert(UserWord {
                    name: format!("{}.{s}!", c.name),
                    def_span: c.name_span,
                    declared_inputs:  Some(2),
                    declared_outputs: Some(0),
                });
            }
        }
    }

    // Collect variables, constants, and CREATE'd buffers into
    // their own tables.
    let mut variables: BTreeMap<String, VariableDef> = BTreeMap::new();
    let mut constants: BTreeMap<String, ConstantDef> = BTreeMap::new();
    let mut creates:   BTreeMap<String, CreateDef>   = BTreeMap::new();
    let mut collections: BTreeMap<String, CollectionDef> = BTreeMap::new();
    let mut templates: BTreeMap<String, TemplateDef> = BTreeMap::new();
    let mut values:    BTreeMap<String, ValueDef>    = BTreeMap::new();
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
            Item::Value(v) => {
                values.insert(v.name.to_ascii_lowercase(), v.clone());
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
        values,
        class_slots,
        user_effects: inferred.user_effects,
        body_effects: inferred.body_effects,
        effect_errors,
        call_graph: BTreeMap::new(),
        use_sites: BTreeMap::new(),
        escape: HashMap::new(),
        docs: HashMap::new(),
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
    let empty = BTreeMap::new();
    expand_templates_pre_resolve_with_prior(program, &empty)
}

fn expand_templates_pre_resolve_with_prior(
    program: Program,
    prior_templates: &BTreeMap<String, TemplateDef>,
) -> Program {
    // Collect templates: prior (from earlier evals in this session)
    // PLUS templates defined in this compile.  This-compile takes
    // precedence for same-name keys (Factor allows redefinition).
    let mut templates: BTreeMap<String, TemplateDef> = prior_templates.clone();
    for i in &program.items {
        if let Item::Template(t) = i {
            templates.insert(t.name.to_ascii_lowercase(), t.clone());
        }
    }
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
            // Computed CONSTANT/FCONSTANT bodies need walking so
            // sema picks up word references inside the value
            // expression (e.g. `3.5e 240e f/ FCONSTANT mb-dx`).
            Item::Constant(c) => {
                if let crate::compiler::ast::ConstValue::Computed(exprs) = &c.value {
                    walk_body_for_refs(exprs, None, sema);
                }
            }
            // Variable, Create, Collection carry no expressions
            // to walk.
            Item::Variable(_) | Item::Create(_) | Item::Collection(_) => {}
            // VALUE initial-body may reference user words.
            Item::Value(v) => {
                walk_body_for_refs(&v.initial, None, sema);
            }
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
            // Method body — caller is the generic name.
            Item::Method(m) => {
                let lc = m.generic_name.to_ascii_lowercase();
                walk_body_for_refs(&m.body, Some(&lc), sema);
            }
            // Class / Generic / RawFactor have no Forth-side body.
            Item::Class(_) | Item::Generic(_) | Item::RawFactor(_) => {}
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
            Item::Constant(c) => {
                if let crate::compiler::ast::ConstValue::Computed(exprs) = &c.value {
                    walk_block_for_escape(exprs, &var_names, sema);
                }
            }
            Item::Variable(_) | Item::Create(_) | Item::Collection(_) => {}
            Item::Value(v) => {
                walk_block_for_escape(&v.initial, &var_names, sema);
            }
            Item::Template(t) => {
                walk_block_for_escape(&t.constructor, &var_names, sema);
                walk_block_for_escape(&t.does_body,   &var_names, sema);
            }
            Item::TemplateInstance(ti) => {
                walk_block_for_escape(&ti.does_body, &var_names, sema);
            }
            Item::Method(m) => {
                walk_block_for_escape(&m.body, &var_names, sema);
            }
            Item::Class(_) | Item::Generic(_) | Item::RawFactor(_) => {}
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
            Expr::Tick { name, span } => {
                // ' name pushes the XT — same as referencing the
                // name's xt, so record it as a use site (it ties
                // the target word into the call graph).
                let lc = name.to_ascii_lowercase();
                sema.use_sites.entry(lc.clone()).or_default().push(*span);
                if let Some(c) = caller {
                    sema.call_graph
                        .entry(c.to_string())
                        .or_default()
                        .insert(lc);
                }
            }
            Expr::To { name, span } => {
                // `TO name` is a write to the VALUE — count it as a
                // use site so dead-code analysis sees the dependency,
                // and link it to the caller's call graph.
                let lc = name.to_ascii_lowercase();
                sema.use_sites.entry(lc.clone()).or_default().push(*span);
                if let Some(c) = caller {
                    sema.call_graph
                        .entry(c.to_string())
                        .or_default()
                        .insert(lc);
                }
            }
            Expr::LetForm { .. } => {
                // LET forms are self-contained — no external word
                // refs, no use-sites for the sema graph to record.
            }
            Expr::See { name, span } => {
                // SEE records a use-site for its target so a word that
                // is *only* ever SEEn still counts as referenced (and
                // doesn't get flagged dead).  No call-graph edge — SEE
                // doesn't call the word, it describes it.
                let lc = name.to_ascii_lowercase();
                sema.use_sites.entry(lc).or_default().push(*span);
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
