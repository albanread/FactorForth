//! Stack-effect inference.  Phase 2.7.
//!
//! Two jobs:
//!
//!   1. Catch programs where a declared `( ins -- outs )` annotation
//!      doesn't match the body's actual behaviour.  Example:
//!
//!          : bad ( -- ) 1 2 ;
//!          ↓
//!          declared 0 outputs but body produces 2
//!
//!   2. Provide the inferred effect of every user-defined word so
//!      callers don't have to re-derive it.  Lets `: caller foo bar
//!      ;` type-check against its callees.
//!
//! ## First-cut scope
//!
//! Handles straight-line bodies with full rigour, plus first-cut
//! formulas for the structured control forms we lower directly
//! (`IF`, `BEGIN`, `DO`, `CASE`).  When a structure's branches do
//! not unify to one concrete shape, inference returns `Unknown` and
//! the body-vs-declared check is skipped — the declared effect is
//! trusted for caller typing.
//!
//! ## What an effect is
//!
//! `Effect::Known { inputs, outputs }` means "consumes `inputs` items
//! from the data stack and leaves `outputs` items in their place."
//! ANS lets effect-comment items have type-or-purpose names; we
//! ignore those for inference (just count).  Floating-point stack
//! and return stack are NOT modelled in this first cut.

use std::collections::HashMap;

use super::ast::{Definition, Expr, Item, Literal};
use super::error::Span;
use super::resolve::{Resolved, Target};

// ─── Types ─────────────────────────────────────────────────────────────────

/// Net stack effect: consume `inputs`, leave `outputs`.
///
/// `Unknown` means the analyser couldn't (or chose not to) derive
/// a number — caller code must trust the declared effect, or treat
/// this word as opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Known { inputs: u32, outputs: u32 },
    Unknown,
}

impl Effect {
    pub const fn known(inputs: u32, outputs: u32) -> Self {
        Effect::Known { inputs, outputs }
    }

    /// Compose `self` followed by `next`.  Returns Unknown if either
    /// side is.
    pub fn then(self, next: Effect) -> Effect {
        match (self, next) {
            (Effect::Unknown, _) | (_, Effect::Unknown) => Effect::Unknown,
            (Effect::Known { inputs: ai, outputs: ao },
             Effect::Known { inputs: bi, outputs: bo }) => {
                if ao >= bi {
                    // `next` consumes from `self`'s output.
                    Effect::Known {
                        inputs:  ai,
                        outputs: ao - bi + bo,
                    }
                } else {
                    // `next` needs more than `self` left on top —
                    // it dips into `self`'s pre-state by (bi - ao).
                    Effect::Known {
                        inputs:  ai + (bi - ao),
                        outputs: bo,
                    }
                }
            }
        }
    }
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Effect::Known { inputs, outputs } =>
                write!(f, "( {inputs} -- {outputs} )"),
            Effect::Unknown =>
                write!(f, "( ? )"),
        }
    }
}

/// An effect-related diagnostic.  These are *warnings*, not errors:
/// the compile still produces IR (with the synthesised effect, which
/// is correct by construction).  Forth's culture is permissive about
/// ambiguous shapes; we surface the issue in the IDE / CLI without
/// blocking the build.
///
/// Three reasons we keep the historical `EffectError` name even
/// though they're warnings:
///   1. The data shape (a mismatch / an ambiguity) is unchanged.
///   2. Code that holds these in a `Vec` already exists.
///   3. The CLI's stderr renderer can apply severity per-variant
///      without us reshuffling the type tree.
///
/// If we ever introduce *fatal* effect issues (e.g., a stack-effect
/// declaration that the user explicitly asks to be enforced),
/// extend with a severity field.  For now everything here is a
/// warning.
#[derive(Clone, Debug, PartialEq)]
pub enum EffectError {
    /// Declared `( -- )`-style annotation doesn't match the body's
    /// inferred behaviour.  The synth is authoritative; the
    /// declaration is treated as documentation that's drifted from
    /// the code.
    Mismatch {
        name: String,
        at: Span,
        declared: (u32, u32),
        inferred: (u32, u32),
    },
    CaseNeedsDefault {
        name: String,
        at: Span,
    },
}

impl std::fmt::Display for EffectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EffectError::Mismatch { name, at, declared, inferred } => {
                write!(
                    f,
                    "warning: `{name}` at {at}: declared ( -- {} item{}) but body produces ( -- {} item{})",
                    declared.1, if declared.1 == 1 { "" } else { "s " },
                    inferred.1, if inferred.1 == 1 { "" } else { "s " },
                )?;
                if declared.0 != inferred.0 {
                    write!(
                        f,
                        "; also consumes {} (declared {})",
                        inferred.0, declared.0,
                    )?;
                }
                Ok(())
            }
            EffectError::CaseNeedsDefault { name, at } => {
                write!(
                    f,
                    "warning: `{name}` at {at}: CASE has no explicit DEFAULT/OTHER branch, and the no-match path leaves a different stack shape than its OF arms"
                )
            }
        }
    }
}

impl std::error::Error for EffectError {}

// ─── Effect table for ANS builtins ─────────────────────────────────────────

/// The effect of every ANS word resolve knows about.  Keys are
/// ANS-lowercased names (matching `resolve::builtin_table`'s keys).
/// Values are net data-stack effects.
fn builtin_effects() -> HashMap<&'static str, Effect> {
    use Effect::Known as K;
    let e = |i, o| K { inputs: i, outputs: o };
    let mut m: HashMap<&'static str, Effect> = HashMap::new();

    // Stack shuffles ─ kernel
    m.insert("dup",  e(1, 2));
    m.insert("drop", e(1, 0));
    m.insert("swap", e(2, 2));
    m.insert("over", e(2, 3));
    m.insert("rot",  e(3, 3));
    m.insert("nip",  e(2, 1));
    m.insert("tuck", e(2, 3));

    // Arithmetic
    m.insert("+",      e(2, 1));
    m.insert("-",      e(2, 1));
    m.insert("*",      e(2, 1));
    m.insert("/",      e(2, 1));
    m.insert("mod",    e(2, 1));
    m.insert("negate", e(1, 1));
    m.insert("abs",  e(1, 1));
    m.insert("min",  e(2, 1));
    m.insert("max",  e(2, 1));

    // Comparisons / bitwise
    m.insert("=",   e(2, 1));
    m.insert("<>",  e(2, 1));
    m.insert("<",   e(2, 1));
    m.insert(">",   e(2, 1));
    m.insert("<=",  e(2, 1));
    m.insert(">=",  e(2, 1));
    m.insert("0=",  e(1, 1));
    m.insert("0<",  e(1, 1));
    m.insert("0>",  e(1, 1));
    m.insert("and", e(2, 1));
    m.insert("or",  e(2, 1));
    m.insert("xor", e(2, 1));
    m.insert("invert", e(1, 1));

    // DO/LOOP support words (NOT the DO/LOOP structure itself — that's
    // an AST node, not a WordRef).  These are stand-alone words.
    m.insert("i",      e(0, 1));
    m.insert("j",      e(0, 1));
    m.insert("leave",  e(0, 0));
    m.insert("unloop", e(0, 0));

    // I/O
    m.insert(".",     e(1, 0));
    m.insert("cr",    e(0, 0));
    m.insert("emit",  e(1, 0));
    m.insert("space", e(0, 0));
    m.insert("spaces", e(1, 0));

    // String vocabulary (M2.10)
    m.insert("type",  e(2, 0));   // c-addr u --
    m.insert("cmove", e(3, 0));   // src dst u --
    m.insert("fill",  e(3, 0));   // c-addr u char --
    m.insert("bl",    e(0, 1));   // -- 32
    m.insert("key",   e(0, 1));   // -- ch
    m.insert("accept", e(2, 1));  // c-addr u -- u'

    // Pictured numeric output (M2.10b)
    m.insert("<#",    e(0, 0));   // --
    m.insert("#",     e(1, 1));   // n -- n/base
    m.insert("#s",    e(1, 1));   // n -- 0
    m.insert("sign",  e(1, 0));   // n --
    m.insert("hold",  e(1, 0));   // ch --
    m.insert("#>",    e(1, 2));   // n -- c-addr u
    m.insert("n>$",   e(1, 2));   // n -- c-addr u
    m.insert("hex",     e(0, 0));
    m.insert("decimal", e(0, 0));
    m.insert("binary",  e(0, 0));
    m.insert("octal",   e(0, 0));

    // Memory model
    m.insert("@",  e(1, 1));
    m.insert("!",  e(2, 0));
    m.insert("c@", e(1, 1));
    m.insert("c!", e(2, 0));
    m.insert("+!", e(2, 0));

    // Cell/char arithmetic
    m.insert("cell+", e(1, 1));
    m.insert("char+", e(1, 1));
    m.insert("cells", e(1, 1));
    m.insert("chars", e(1, 1));
    m.insert("floats", e(1, 1));
    m.insert("allot", e(1, 0));
    m.insert("here",  e(0, 1));

    // Float memory ops
    m.insert("f@", e(1, 1));
    m.insert("f!", e(2, 0));

    // ── M2.x quick-wins: latent runtime words surfaced ─────────────
    // Without effects here, our colon-def emit falls back to row-vars
    // (..a -- ..b) which Factor refuses to inline-compile against
    // fixed-effect callees like our forth.runtime:>r.  Declared
    // effects let the synth path produce concrete (N -- M) so Factor
    // accepts the def.
    // (?DUP intentionally not surfaced — see resolve.rs comment.)
    m.insert("depth", e(0, 1));

    // Return-stack: data-stack effects only (the r-stack is invisible
    // here, by design).
    m.insert(">r",    e(1, 0));
    m.insert("r>",    e(0, 1));
    m.insert("r@",    e(0, 1));
    m.insert("rdrop", e(0, 0));
    m.insert("2>r",   e(2, 0));
    m.insert("2r>",   e(0, 2));

    m.insert("u.",    e(1, 0));

    m.insert("s>d",   e(1, 1));   // identity on 64-bit cells
    m.insert("d>s",   e(1, 1));
    m.insert("d>f",   e(1, 1));
    m.insert("f>d",   e(1, 1));

    m.insert("f+", e(2, 1));
    m.insert("f-", e(2, 1));
    m.insert("f*", e(2, 1));
    m.insert("f/", e(2, 1));
    m.insert("f<", e(2, 1));
    m.insert("f>", e(2, 1));
    m.insert("f=", e(2, 1));

    m.insert("execute", e(1, 0));   // xt --

    // ── M2.x #39 ANS Core completeness ────────────────────────────
    m.insert("1+",     e(1, 1));
    m.insert("1-",     e(1, 1));
    m.insert("2*",     e(1, 1));
    m.insert("2/",     e(1, 1));
    m.insert("/mod",   e(2, 2));   // a b -- r q
    m.insert("*/",     e(3, 1));   // a b c -- d
    m.insert("*/mod",  e(3, 2));   // a b c -- r q
    m.insert("lshift", e(2, 1));   // n u -- n'
    m.insert("rshift", e(2, 1));
    m.insert("2dup",   e(2, 4));
    m.insert("2drop",  e(2, 0));
    m.insert("2swap",  e(4, 4));
    m.insert("2over",  e(4, 6));
    m.insert("-rot",   e(3, 3));
    m.insert("2@",     e(1, 2));   // addr -- x1 x2
    m.insert("2!",     e(3, 0));   // x1 x2 addr --
    m.insert("erase",  e(2, 0));   // c-addr u --
    m.insert("0<>",    e(1, 1));   // n -- flag

    // ── M2.x #43 managed strings ($-vocab) ────────────────────────
    m.insert("$len",       e(1, 1));   // s -- n
    m.insert("$clen",      e(1, 1));
    m.insert("$+",         e(2, 1));   // a b -- c
    m.insert("$upper",     e(1, 1));
    m.insert("$lower",     e(1, 1));
    m.insert("$find",      e(2, 1));   // haystack needle -- index
    m.insert("$contains?", e(2, 1));
    m.insert("$starts?",   e(2, 1));
    m.insert("$ends?",     e(2, 1));
    m.insert("$slice",     e(3, 1));   // s from len -- s'
    m.insert("$cmp",       e(2, 1));
    m.insert("$hash",      e(1, 1));
    m.insert("$.",         e(1, 0));
    m.insert("$.cr",       e(1, 0));
    m.insert("int>$",      e(1, 1));
    m.insert("$>int",      e(1, 1));
    m.insert(">$",         e(2, 1));   // c-addr u -- s
    m.insert("$>addr",     e(1, 2));   // s -- c-addr u

    // ── M2.x #32 ANS File Access (minimal) ────────────────────────
    m.insert("included",   e(2, 0));   // c-addr u --

    m
}

// ─── Inference driver ──────────────────────────────────────────────────────

/// Result of running effect inference over a whole program.
#[derive(Clone, Debug)]
pub struct Inferred {
    /// Per-name effect, keyed by lowercase ANS name.  This is the
    /// effect callers should TYPE THEMSELVES AGAINST: declared when
    /// present (chicken-and-egg for mutual recursion), inferred
    /// when no annotation, Unknown when neither says anything.
    pub user_effects: HashMap<String, Effect>,
    /// Per-name body-derived effect.  This is the GROUND TRUTH from
    /// walking the body — independent of the declared annotation.
    /// Use this when deciding which annotation to emit (synth wins
    /// over a wrong user declaration).  Missing if the body was
    /// never inferred (e.g., for variables/constants).
    pub body_effects: HashMap<String, Effect>,
}

/// Run inference + checking over a resolved program.  Returns the
/// inferred effects of user words, and a list of declared-vs-inferred
/// mismatches.  No mismatch is a hard error here — the driver
/// (`compile`) decides whether to fail the compile or warn.
pub fn infer(r: &Resolved) -> (Inferred, Vec<EffectError>) {
    let empty = HashMap::new();
    infer_with_prior(r, &empty)
}

/// Like [`infer`] but seeded with effect info for names defined in
/// prior compiles in this interactive session.  Lets eval 2's body
/// synth account for words eval 1 defined.  Without this, a word
/// defined in one eval looks Unknown to subsequent compiles, and
/// the synth falls back to row-vars — which then doesn't match
/// the body's concrete effect, and Factor's strict inference
/// rejects the compile.
pub fn infer_with_prior(
    r: &Resolved,
    prior_effects: &HashMap<String, Effect>,
) -> (Inferred, Vec<EffectError>) {
    let builtins = builtin_effects();
    // Start with prior_effects (so refs to previously-defined
    // words resolve), then layer this compile's declarations on
    // top (so re-defs supersede).
    let mut user_effects: HashMap<String, Effect> = prior_effects.clone();

    // Seed user_effects with declared effects so straight-line
    // callers can type-check against them, even before bodies are
    // analysed.  Mutual recursion: the declared effect is taken as
    // truth here.  Undeclared definitions get Unknown until we walk
    // their body.
    for item in &r.program.items {
        match item {
            Item::Definition(d) => {
                let lc = d.name.to_ascii_lowercase();
                let eff = match &d.effect {
                    Some(se) => Effect::known(
                        se.inputs.len() as u32,
                        se.outputs.len() as u32,
                    ),
                    None => Effect::Unknown,
                };
                user_effects.insert(lc, eff);
            }
            // Variables and constants push exactly one item per
            // reference (the address / the value).  This lets
            // callers like `: foo  x @  ;` infer their body as
            // ( -- v ) instead of falling back to Unknown.
            Item::Variable(v) => {
                user_effects.insert(v.name.to_ascii_lowercase(), Effect::known(0, 1));
            }
            Item::Constant(c) => {
                user_effects.insert(c.name.to_ascii_lowercase(), Effect::known(0, 1));
            }
            Item::Create(cd) => {
                // CREATE'd words push their data-space base address.
                user_effects.insert(cd.name.to_ascii_lowercase(), Effect::known(0, 1));
            }
            Item::Collection(cl) => {
                // Collection instances are `( idx -- addr )` — they
                // take an index and return the address of that
                // element.  Net effect (1 -- 1).
                user_effects.insert(cl.name.to_ascii_lowercase(), Effect::known(1, 1));
            }
            Item::Template(t) => {
                // A template name on its own does nothing meaningful
                // when called (it expects to be in instantiation
                // position).  Effectively a parser-level marker
                // here.  Register as (0 -- 0) so any stray reference
                // doesn't poison its caller's inference.
                user_effects.insert(t.name.to_ascii_lowercase(), Effect::known(0, 0));
            }
            Item::TemplateInstance(ti) => {
                // Same as Collection: ( idx -- addr ).  (For
                // templates that aren't indexed accessors, the
                // does_body might have a different effect; we
                // approximate as (1 -- 1) for now since the common
                // case is array-like.)
                user_effects.insert(ti.name.to_ascii_lowercase(), Effect::known(1, 1));
            }
            Item::TopLevel { .. } => {}
        }
    }

    let mut errors: Vec<EffectError> = Vec::new();
    let mut body_effects: HashMap<String, Effect> = HashMap::new();

    // Pass 2: infer each definition's body and compare against
    // declared (if any).  Build an Env locally per iteration so the
    // borrow of `user_effects` ends before we update it.  The user-
    // word effects available to a callee are those seen at the
    // *current* point in the source — forward references to
    // later-defined words use the declared effect we seeded in
    // pass 1, not a not-yet-inferred body effect.
    for idx in 0..r.program.items.len() {
        let Item::Definition(d) = &r.program.items[idx] else { continue };
        let body_eff = {
            let env = Env { builtins: &builtins,
                            user_effects: &user_effects,
                            resolved: r };
            infer_block(&d.body, &env)
        };
        let lc = d.name.to_ascii_lowercase();
        body_effects.insert(lc.clone(), body_eff);
        check_definition(d, body_eff, &mut errors);
        {
            let env = Env { builtins: &builtins,
                            user_effects: &user_effects,
                            resolved: r };
            collect_case_warnings(&d.body, &env, &d.name, &mut errors);
        }
        if d.effect.is_none() {
            user_effects.insert(lc, body_eff);
        }
    }

    (Inferred { user_effects, body_effects }, errors)
}

struct Env<'a> {
    builtins:     &'a HashMap<&'static str, Effect>,
    user_effects: &'a HashMap<String, Effect>,
    resolved:     &'a Resolved,
}

/// Walk a body and return its net effect.  Straight-line bodies
/// give an exact `Effect::Known`; bodies containing any control-
/// flow node yield `Effect::Unknown` for this first cut.
fn infer_block(exprs: &[Expr], env: &Env) -> Effect {
    let mut acc = Effect::known(0, 0);
    for e in exprs {
        let one = effect_of_expr(e, env);
        acc = acc.then(one);
        if matches!(acc, Effect::Unknown) { return Effect::Unknown; }
    }
    acc
}

fn join_branch_effects(then_eff: Effect, else_eff: Effect) -> Option<(u32, u32)> {
    match (then_eff, else_eff) {
        (Effect::Known { inputs: ti, outputs: to },
         Effect::Known { inputs: ei, outputs: eo })
            if ti == ei && to == eo =>
        {
            Some((ti, to))
        }
        _ => None,
    }
}

fn if_else_effect(then_eff: Effect, else_eff: Effect) -> Effect {
    match join_branch_effects(then_eff, else_eff) {
        Some((inputs, outputs)) => Effect::known(1, 0).then(Effect::known(inputs, outputs)),
        None => Effect::Unknown,
    }
}

fn infer_case_effect(arms: &[super::ast::CaseArm], default: Option<&[Expr]>, env: &Env) -> Effect {
    if let Some((head, tail)) = arms.split_first() {
        let cond_eff = Effect::known(1, 2)
            .then(infer_block(&head.match_expr, env))
            .then(Effect::known(2, 1));
        let then_eff = Effect::known(1, 0).then(infer_block(&head.body, env));
        let else_eff = infer_case_effect(tail, default, env);
        cond_eff.then(if_else_effect(then_eff, else_eff))
    } else {
        let base = match default {
            Some(d) => infer_block(d, env),
            None => Effect::known(0, 0),
        };
        base.then(Effect::known(1, 0))
    }
}

fn case_needs_default_warning(arms: &[super::ast::CaseArm], env: &Env) -> bool {
    let no_match_eff = Effect::known(1, 0);
    arms.iter().any(|arm| {
        let arm_eff = Effect::known(1, 0).then(infer_block(&arm.body, env));
        matches!(arm_eff, Effect::Known { .. }) && join_branch_effects(arm_eff, no_match_eff).is_none()
    })
}

fn collect_case_warnings(exprs: &[Expr], env: &Env, name: &str, errors: &mut Vec<EffectError>) {
    for e in exprs {
        match e {
            Expr::If { then_body, else_body, .. } => {
                collect_case_warnings(then_body, env, name, errors);
                if let Some(eb) = else_body {
                    collect_case_warnings(eb, env, name, errors);
                }
            }
            Expr::BeginUntil { body, .. }
            | Expr::BeginAgain { body, .. }
            | Expr::DoLoop { body, .. } => {
                collect_case_warnings(body, env, name, errors);
            }
            Expr::BeginWhileRepeat { pred, body, .. } => {
                collect_case_warnings(pred, env, name, errors);
                collect_case_warnings(body, env, name, errors);
            }
            Expr::Case { arms, default, span } => {
                if default.is_none() && case_needs_default_warning(arms, env) {
                    errors.push(EffectError::CaseNeedsDefault {
                        name: name.to_string(),
                        at: *span,
                    });
                }
                for arm in arms {
                    collect_case_warnings(&arm.match_expr, env, name, errors);
                    collect_case_warnings(&arm.body, env, name, errors);
                }
                if let Some(d) = default {
                    collect_case_warnings(d, env, name, errors);
                }
            }
            _ => {}
        }
    }
}

fn effect_of_expr(e: &Expr, env: &Env) -> Effect {
    match e {
        // Every literal pushes one item.  String literals via `."`
        // emit at runtime via `forth.runtime:type` and produce no
        // user-visible data-stack item; ANS S" and C" produce one
        // or two items depending on which.  For now treat strings
        // as producing one item — wrong for `."` but `."` is rare
        // in stack-effect-critical code.  Refine when it bites.
        Expr::Lit(Literal::Int   { .. })
        | Expr::Lit(Literal::Float { .. }) => Effect::known(0, 1),
        Expr::Lit(Literal::Str   { kind, .. }) => match kind {
            super::lex::StringKind::DotQuote     => Effect::known(0, 0),
            super::lex::StringKind::SQuote       => Effect::known(0, 2),
            super::lex::StringKind::CQuote       => Effect::known(0, 1),
            super::lex::StringKind::SDollarQuote => Effect::known(0, 1),
        },

        Expr::WordRef { name, span } => {
            let lc = name.to_ascii_lowercase();
            // User-defined takes precedence (resolve guaranteed
            // that's what was bound).
            if let Some(t) = env.resolved.word_targets.get(span) {
                if matches!(t, Target::UserDefined { .. }) {
                    return env.user_effects.get(&lc).copied().unwrap_or(Effect::Unknown);
                }
            }
            env.builtins.get(lc.as_str()).copied().unwrap_or(Effect::Unknown)
        }

        // Control flow.  Each structure has a known formula in
        // terms of its sub-body effects.  We compute body effects
        // first, then apply the formula.  Returns Unknown if any
        // sub-body is Unknown or if a branch shape constraint is
        // violated (e.g. IF/ELSE branches with different effects).
        Expr::If { then_body, else_body, .. } => {
            let then_eff = infer_block(then_body, env);
            // Consume the flag.
            let flag = Effect::known(1, 0);
            match else_body {
                None => {
                    // No-else IF: the THEN-body must be balanced
                    // (i -- i) — the join point requires both
                    // "body ran" and "body skipped" paths to leave
                    // the stack the same shape.  If body isn't
                    // balanced, the overall effect is undefined.
                    match then_eff {
                        Effect::Known { inputs, outputs } if inputs == outputs => {
                            // After flag consumption + body, stack
                            // depth changes by -1 (the flag) + 0
                            // (body balanced).  Inputs: 1 flag plus
                            // body's input requirements.
                            flag.then(Effect::known(inputs, outputs))
                        }
                        Effect::Known { .. } => Effect::Unknown,
                        Effect::Unknown => Effect::Unknown,
                    }
                }
                Some(eb) => {
                    // IF/ELSE: both branches must have matching
                    // effects.  After flag consumption, EITHER
                    // branch runs.  Total: ( 1 + i -- o ).
                    let else_eff = infer_block(eb, env);
                    flag.then(if_else_effect(then_eff, else_eff))
                }
            }
        }

        Expr::BeginUntil { body, .. } => {
            // Body should produce a flag each iteration (its outputs
            // exceed inputs by exactly 1).  After UNTIL consumes the
            // flag and the loop exits, net effect on the data stack
            // is zero per iteration.  Net effect of loop: body's
            // inputs and outputs minus the flag.
            match infer_block(body, env) {
                Effect::Known { inputs, outputs }
                    if outputs >= 1 && outputs == inputs + 1 =>
                {
                    Effect::known(inputs, inputs)  // flag consumed by UNTIL
                }
                Effect::Known { inputs, outputs } if outputs > inputs => {
                    // Body produces more than just a flag (e.g.
                    // accumulator + flag).  Final state has the
                    // accumulator without the flag.
                    Effect::known(inputs, outputs - 1)
                }
                _ => Effect::Unknown,
            }
        }

        Expr::BeginWhileRepeat { pred, body, .. } => {
            // pred: (i -- i') where i' == i + 1 (produces flag).
            // body: (i -- i) — preserves shape for next iteration.
            // After exit, flag is consumed.  Net: (i_pred_in -- i_pred_in).
            let pred_eff = infer_block(pred, env);
            let body_eff = infer_block(body, env);
            match (pred_eff, body_eff) {
                (Effect::Known { inputs: pi, outputs: po },
                 Effect::Known { inputs: bi, outputs: bo })
                    if po == pi + 1 && bi == bo =>
                {
                    Effect::known(pi.max(bi), pi.max(bi))
                }
                _ => Effect::Unknown,
            }
        }

        Expr::BeginAgain { body, .. } => {
            // Infinite loop — never exits normally.  Effect is
            // unrepresentable in (in -- out) form.  We approximate
            // as Unknown; downstream synthesis will emit the
            // row-variable fallback for any enclosing definition.
            let _ = infer_block(body, env);
            Effect::Unknown
        }

        Expr::DoLoop { body, .. } => {
            // DO/?DO consumes limit + start (2 inputs).  Body must
            // be balanced (i -- i) since each iteration restores
            // the shape.  Net: (2 -- 0).
            match infer_block(body, env) {
                Effect::Known { inputs, outputs } if inputs == outputs => {
                    // Body balanced — loop is (2 -- 0) regardless
                    // of body inputs (those are dipped beneath the
                    // loop's limit+start).
                    Effect::known(2 + inputs, inputs)
                }
                _ => Effect::Unknown,
            }
        }

        Expr::Case { arms, default, .. } => {
            infer_case_effect(arms, default.as_deref(), env)
        }

        Expr::Tick { .. } => {
            // ' name pushes the XT of `name` — one stack item out,
            // nothing consumed.  Same as a literal.
            Effect::known(0, 1)
        }

        Expr::LetForm { form, .. } => {
            // LET consumes inputs.len() cells and produces
            // outputs.len() cells.  Static and known.
            Effect::known(form.inputs.len() as u32,
                          form.outputs.len() as u32)
        }
    }
}

/// Compare a definition's declared and inferred effects.  Push a
/// `Mismatch` error to `errors` if they disagree (and both are known).
fn check_definition(d: &Definition, body_eff: Effect, errors: &mut Vec<EffectError>) {
    let Some(se) = &d.effect else { return; };
    let declared = (se.inputs.len() as u32, se.outputs.len() as u32);
    let Effect::Known { inputs: bi, outputs: bo } = body_eff else { return; };
    let inferred = (bi, bo);
    // ANS-strict comparison: net change must match.  We compare
    // both inputs and outputs separately, since an inputs mismatch
    // is its own ANS-meaningful problem.
    if declared != inferred {
        errors.push(EffectError::Mismatch {
            name: d.name.clone(),
            at: d.name_span,
            declared,
            inferred,
        });
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{lex, parse, resolve::resolve};

    fn infer_str(src: &str) -> (Inferred, Vec<EffectError>) {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        let r = resolve(prog).unwrap();
        infer(&r)
    }

    #[test]
    fn effect_compose_simple() {
        let a = Effect::known(0, 1);
        let b = Effect::known(1, 0);
        assert_eq!(a.then(b), Effect::known(0, 0));
    }

    #[test]
    fn effect_compose_needs_more() {
        // (0 -- 1) then (2 -- 1): second consumes 2 but only 1
        // available, so total inputs must be 1 (one from below).
        let a = Effect::known(0, 1);
        let b = Effect::known(2, 1);
        assert_eq!(a.then(b), Effect::known(1, 1));
    }

    #[test]
    fn correct_square_passes() {
        let (_, errs) = infer_str(": square ( n -- n^2 ) dup * ;");
        assert!(errs.is_empty(), "got: {errs:?}");
    }

    #[test]
    fn correct_add_passes() {
        let (_, errs) = infer_str(": add2 ( a b -- a+b ) + ;");
        assert!(errs.is_empty(), "got: {errs:?}");
    }

    #[test]
    fn the_plan_canonical_failure_reports() {
        // M2.7 success criterion verbatim.
        let (_, errs) = infer_str(": bad ( -- ) 1 2 ;");
        assert_eq!(errs.len(), 1);
        let msg = errs[0].to_string();
        assert!(msg.contains("declared") && msg.contains("2"),
                "expected mismatch mentioning 2, got: {msg}");
    }

    #[test]
    fn declared_inputs_mismatch_reports() {
        // Declared takes 1 input but body needs 2.
        let (_, errs) = infer_str(": bad ( a -- b ) + ;");
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn underflow_within_body_counts_as_inputs() {
        // `+ +` needs 4 inputs to balance (1, 1, then needs 2 more).
        // Wait, + is (2 -- 1).  Two +'s = first (2 -- 1) then second
        // (2 -- 1).  Composing: first leaves 1 output; second
        // consumes 2 but only 1 available → total inputs 1+1=2 more.
        // Net: (3 -- 1).  Verify.
        let (inferred, errs) = infer_str(": chain ( -- ) + + ;");
        // The declared (--) means 0 inputs 0 outputs.  We inferred
        // (3 -- 1).  Should error.
        assert!(!errs.is_empty(), "expected mismatch error");
        // Confirm the inferred shape.
        let e = inferred.user_effects.get("chain").copied().unwrap();
        // Since declared is present, user_effects retains declared.
        // We need to call infer_block directly to see the body's
        // effect.  Quick check via the error contents instead:
        let _ = e;
    }

    #[test]
    fn control_flow_in_body_skips_check() {
        // Declared (a -- |a|) but body has IF.  Inference yields
        // Unknown for the body, so no mismatch is reported even
        // though the declared shape might or might not be right.
        let (_, errs) = infer_str(": myabs ( n -- |n| ) dup 0 < if negate then ;");
        // body has IF → Unknown → check skipped.  No effect error.
        let effect_errs: Vec<_> = errs.iter().collect();
        assert!(effect_errs.is_empty(), "got: {effect_errs:?}");
    }

    #[test]
    fn missing_annotation_no_error() {
        // No declared effect → nothing to mismatch against.
        let (_, errs) = infer_str(": who-knows 1 2 3 ;");
        assert!(errs.is_empty());
    }

    #[test]
    fn user_word_effects_propagate() {
        // `inc` declared (n -- n+1).  Caller `twice` should use that.
        let (inf, errs) = infer_str(
            ": inc ( n -- n+1 ) 1 + ; \
             : twice ( n -- n+2 ) inc inc ;"
        );
        assert!(errs.is_empty(), "got: {errs:?}");
        assert_eq!(inf.user_effects.get("inc"),
                   Some(&Effect::known(1, 1)));
        assert_eq!(inf.user_effects.get("twice"),
                   Some(&Effect::known(1, 1)));
    }

        #[test]
        fn case_effect_statement_with_default_is_known() {
                let (inf, errs) = infer_str(
                        ": classify \
                             case \
                                 1 of .\" one\" endof \
                                 2 of .\" two\" endof \
                                 .\" unknown\" \
                             endcase ;"
                );
                assert!(errs.is_empty(), "got: {errs:?}");
                assert_eq!(inf.body_effects.get("classify"),
                                     Some(&Effect::known(1, 0)));
        }

        #[test]
        fn case_effect_statement_without_default_is_known() {
                let (inf, errs) = infer_str(
                        ": maybe \
                             case \
                                 1 of .\" one\" endof \
                             endcase ;"
                );
                assert!(errs.is_empty(), "got: {errs:?}");
                assert_eq!(inf.body_effects.get("maybe"),
                                     Some(&Effect::known(1, 0)));
        }

        #[test]
        fn case_effect_value_with_dispatch_preserving_default_is_known() {
                let (inf, errs) = infer_str(
                        ": code \
                             case \
                                 1 of 100 endof \
                                 2 of 200 endof \
                                 0 swap \
                             endcase ;"
                );
                assert!(errs.is_empty(), "got: {errs:?}");
                assert_eq!(inf.body_effects.get("code"),
                                     Some(&Effect::known(1, 1)));
        }

        #[test]
        fn case_effect_value_without_total_fallback_is_unknown() {
                let (inf, errs) = infer_str(
                        ": code \
                             case \
                                 1 of 100 endof \
                             endcase ;"
                );
            assert!(errs.iter().any(|e| matches!(e, EffectError::CaseNeedsDefault { .. })),
                "expected missing-default warning, got: {errs:?}");
                assert_eq!(inf.body_effects.get("code"),
                                     Some(&Effect::Unknown));
        }

        #[test]
        fn explicit_default_suppresses_case_warning() {
            let (_, errs) = infer_str(
                ": code \
                     case \
                     1 of 100 endof \
                     default 0 swap \
                     endcase ;"
            );
            assert!(!errs.iter().any(|e| matches!(e, EffectError::CaseNeedsDefault { .. })),
                "unexpected missing-default warning: {errs:?}");
        }
}
