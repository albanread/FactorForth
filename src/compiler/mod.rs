//! Compiler — the ANS Forth front end.
//!
//! Stages, in order:
//!
//!   1. **Lex** (`lex.rs`) — source text → token stream.
//!   2. **Parse** (`parse.rs`, Phase 2.2) — token stream → AST.
//!   3. **Resolve** (`resolve.rs`, Phase 2.3) — AST + dictionary →
//!      annotated AST with Factor target names.
//!   4. **Effect** (`effect.rs`, Phase 2.7) — stack-effect inference
//!      and checking.
//!   5. **Emit** (`emit.rs`, Phase 2.3+) — annotated AST → canonical
//!      Factor source (IR).
//!
//! The IR is fed to the embedded Factor VM via `session::eval` (Phase 3)
//! which calls into the patched factor.dll's `nf_eval_string`.
//!
//! Errors flow through `error.rs` with source positions; users see ANS-
//! style messages, never Factor frames.

pub mod ast;
pub mod dump;
pub mod effect;
pub mod emit;
pub mod error;
pub mod let_lang;
pub mod lex;
pub mod lower_classes;
pub mod lower_exit;
pub mod lower_qdup;
pub mod lower_recurse;
pub mod parse;
pub mod resolve;
pub mod sema;

pub use ast::{
    CaseArm, CollectionDef, CollectionKind, ConstFlavour, ConstValue,
    ConstantDef, CreateDef, Definition, Expr, Item, Literal, LoopKind,
    Program, StackEffect, TemplateDef, TemplateInstanceDef, VariableDef,
};
pub use effect::{infer, Effect, EffectError, Inferred};
pub use emit::{emit, EmitOpts};
pub use error::{CompileError, Pos, Span};
pub use lex::{lex, NumBase, StringKind, Tok, Token};
pub use parse::{parse, ParseError};
pub use resolve::{resolve, ResolveError, Resolved, Target};
pub use sema::{
    build as build_sema,
    build_with_prior as build_sema_with_prior,
    build_with_prior_and_templates as build_sema_with_prior_and_templates,
    EscapeReason, EscapeState, Sema, UserWord,
};

/// Persistent state that carries across compiles in one interactive
/// session.  When the user types `: square ... ;` in one eval and
/// then `square` in the next, the second compile needs to know
/// `square` is a user word.  Hold one of these per Session in
/// host code and use [`compile_in_context`] for each eval.
///
/// Each successful compile merges the names it defined into
/// `user_words`.  On session drop / reboot the host discards
/// the context (Factor's dictionary resets too, so the maps
/// must stay in lockstep).
#[derive(Clone, Debug, Default)]
pub struct CompileContext {
    /// Lowercase ANS name → span of the first definition.  Span
    /// is the first-definition site for diagnostics; the latest
    /// definition is what's live in Factor's dictionary (Factor
    /// allows redefinition and the newest wins).
    pub user_words: std::collections::HashMap<String, error::Span>,
    /// Lowercase ANS name → inferred stack effect.  Lets eval N+1's
    /// body synth see eval N's words as concretely-effected rather
    /// than Unknown — without this, the synth path falls back to
    /// row-vars for any body that references a prior-eval word,
    /// and Factor's strict inference then refuses to compile a
    /// concrete body under a row-var annotation.
    pub user_effects: std::collections::HashMap<String, Effect>,
    /// Lowercase template name → the TemplateDef.  Templates defined
    /// in one eval need to be visible to subsequent evals' triple
    /// patterns (`<n> tmplname <newname>`) so they expand to
    /// `Item::TemplateInstance` rather than a stray WordRef.
    pub templates: std::collections::BTreeMap<String, ast::TemplateDef>,
    /// Lowercase VALUE name → first-definition span.  Used by
    /// resolve to tell whether a `TO name` target is actually a
    /// VALUE (vs. a regular word or a VARIABLE).  Carries only
    /// the span, not the full ValueDef, because the runtime
    /// storage symbol lives in Factor's image and follows from
    /// the public name (`nf-value-<name>`) — no need to re-emit
    /// the def on subsequent compiles.
    pub values: std::collections::HashMap<String, error::Span>,

    /// Lowercase class name → flat slot list (parent slots first,
    /// then own).  Used cross-eval so a CLASS defined in eval N
    /// is visible in eval N+1 — constructor stack effect sizing
    /// and accessor lookup both depend on knowing the full slot
    /// list.  Same persistence story as `templates` and `values`:
    /// Factor's tuple-class is in the image, we just need to
    /// remember enough metadata on our side to compile against
    /// it from later evals.
    pub classes: std::collections::HashMap<String, Vec<String>>,

    /// Lowercase name → its `SEE` introspection record.  Persisted
    /// across evals so `SEE foo` works on a `foo` defined in an
    /// earlier REPL line.  Built from each compile's AST + source by
    /// [`build_docs`]; newest definition wins (matching Factor's
    /// redefinition semantics).  A few KB of strings per session —
    /// the cost of retaining source is negligible.
    pub docs: std::collections::HashMap<String, ast::WordDoc>,

    /// Canonical paths of files already pulled in by `NEEDS`, the
    /// include-once set.  Persisted across evals so a second `NEEDS`
    /// of the same file — in this or a later eval — expands to nothing
    /// (its words are already in `user_words` / the Factor image).
    /// Keyed by `fs::canonicalize` result, falling back to the raw
    /// path string when the file can't be canonicalized.
    pub included: std::collections::HashSet<String>,
}

impl CompileContext {
    pub fn new() -> Self { Self::default() }
}

/// Top-level convenience: ANS Forth source string → Factor IR string.
///
/// Pipeline: lex → parse → sema (resolve + effect + escape) → emit.
/// Errors stringify via each stage's `Display`.  Returns the IR
/// ready to feed to `nf_eval_string`.
///
/// **Effect diagnostics are warnings, not errors.**  A declared
/// `( a -- b )` annotation that doesn't match the body's inferred
/// behaviour produces a `Mismatch` in `sema.effect_errors` but the
/// compile still emits valid IR — with the synthesised effect,
/// which is correct by construction.  Callers (the CLI, the
/// future IDE) decide whether to surface, ignore, or escalate.
/// This matches Forth's permissive culture: the user can write
/// programs whose effects are ambiguous on purpose; we tell them
/// what we see and let them proceed.
///
/// Use [`compile_with_diagnostics`] when you want to see the
/// warnings programmatically.  This function discards them.
///
/// This is the simplest possible driver — Phase 3 will wrap it in
/// a `Session` that owns the embedded VM and supports incremental
/// `compile_and_eval` calls with carry-over state.
pub fn compile(source: &str) -> Result<String, String> {
    compile_with_diagnostics(source).map(|(ir, _warnings)| ir)
}

/// Compile and return the IR plus any effect-diagnostic warnings.
/// Stage errors (lex, parse, resolve) are still hard failures —
/// they prevent us from producing any IR at all.  Effect issues
/// are not: the synth produces correct IR regardless.
pub fn compile_with_diagnostics(
    source: &str,
) -> Result<(String, Vec<EffectError>), String> {
    let mut ctx = CompileContext::new();
    compile_in_context_with_diagnostics(source, &mut ctx)
}

/// Like [`compile`] but threaded through a [`CompileContext`] that
/// remembers names defined by prior compiles in this session.
///
/// Use this in interactive contexts (the REPL, the IDE worker) so
/// that `: square dup * ;` in one eval and `square` in the next
/// resolve correctly.  Tests and the offline `compile` CLI continue
/// to use [`compile`] which creates a fresh context each call.
///
/// On success, names defined this compile are merged into
/// `ctx.user_words` so subsequent calls see them.
pub fn compile_in_context(
    source: &str,
    ctx: &mut CompileContext,
) -> Result<String, String> {
    compile_in_context_with_diagnostics(source, ctx).map(|(ir, _)| ir)
}

pub fn compile_in_context_with_diagnostics(
    source: &str,
    ctx: &mut CompileContext,
) -> Result<(String, Vec<EffectError>), String> {
    let toks = lex(source).map_err(|e| e.to_string())?;
    let prog = parse(&toks).map_err(|e| e.to_string())?;

    // NEEDS expansion (Rust-driven, compile-time include-once).  Build
    // this file's own SEE docs first — its spans index `source` — then
    // splice in any `NEEDS`'d files, once each, folding each included
    // file's docs against ITS OWN source so SEE shows the right text.
    // After this, `prog` is one merged module and no `Item::Needs`
    // remains for the later passes to see.
    let mut compile_docs = build_docs(&prog.items, source);
    let prog = expand_needs(prog, ctx, None, &mut compile_docs)?;

    let mut sema = sema::build_with_prior_state(
        prog,
        &ctx.user_words,
        &ctx.user_effects,
        &ctx.templates,
        &ctx.values,
        &ctx.classes,
    ).map_err(|e| e.to_string())?;

    // Force every variable in this compile to Wide.  The Narrow
    // optimization (Factor SYMBOL: + get-global/set-global peep)
    // requires whole-program visibility — only safe when ALL uses
    // of a variable are in the same compile.  In a REPL session,
    // eval N+1 references variables defined in eval N, and the
    // peep has no way to translate cross-compile.  Wide form
    // emits a `:` def that returns the storage's nf-addr; eval
    // N+1's reference to the variable name just calls that def
    // and gets the address — cross-eval safe.  Filed as #52.
    //
    // The batch `compile` driver doesn't do this — narrow
    // optimization still applies for one-shot compiles like the
    // conformance test runner, where whole-program visibility
    // holds.
    let dummy_span = error::Span {
        start: error::Pos { line: 0, col: 0, byte_offset: 0 },
        end:   error::Pos { line: 0, col: 0, byte_offset: 0 },
    };
    for state in sema.escape.values_mut() {
        *state = sema::EscapeState::Wide {
            reason: sema::EscapeReason::InteractiveSession,
            at: dummy_span,
        };
    }

    // Build SEE docs for definitions in THIS compile, then seed
    // sema.docs with prior-eval docs first so cross-eval `SEE` works
    // and this compile's (newer) definitions overwrite stale ones.
    sema.docs = ctx.docs.clone();
    for (name, doc) in compile_docs {
        sema.docs.insert(name, doc);
    }

    let warnings = sema.effect_errors.clone();
    let ir = emit(&sema, &EmitOpts::default());
    // Merge new defs into the persistent context for subsequent
    // compiles in this session.  Names from sema.user_words
    // (covers Definition / Variable / Constant / Create /
    // Collection / Template / TemplateInstance), effects from
    // sema.user_effects (whatever inference figured out).
    for (name, uw) in &sema.user_words {
        ctx.user_words.insert(name.clone(), uw.def_span);
    }
    for (name, eff) in &sema.user_effects {
        ctx.user_effects.insert(name.clone(), *eff);
    }
    for (name, tmpl) in &sema.templates {
        ctx.templates.insert(name.clone(), tmpl.clone());
    }
    for (name, vdef) in &sema.values {
        ctx.values.insert(name.clone(), vdef.name_span);
    }
    // Merge class metadata so later evals can use the constructor
    // and accessors.  `sema.class_slots` is keyed by lowercased
    // class name and holds the FLATTENED slot list — exactly what
    // build_with_prior_state's `prior_classes` parameter wants.
    for (name, slots) in &sema.class_slots {
        ctx.classes.insert(name.clone(), slots.clone());
    }
    // Persist this compile's SEE docs (sema.docs already holds prior
    // docs + this compile's, with this compile's winning).
    ctx.docs = sema.docs.clone();
    Ok((ir, warnings))
}

/// Render a parsed `StackEffect` back to canonical `( a b -- c )`
/// text for display in `SEE`.  Returns the empty string when there
/// is no effect to show.
fn render_effect(e: &ast::StackEffect) -> String {
    let mut s = String::from("( ");
    for i in &e.inputs { s.push_str(i); s.push(' '); }
    s.push_str("-- ");
    for o in &e.outputs { s.push_str(o); s.push(' '); }
    s.push(')');
    s
}

/// Build per-name [`ast::WordDoc`] records for every definition in a
/// compile, slicing the original source text for the `source` field.
/// Keyed by lowercased name.  Used by [`compile_in_context_with_diagnostics`]
/// to feed `SEE`.
///
/// Methods are intentionally skipped: a `METHOD:` shares its
/// generic's name, so recording it would clobber the generic's doc.
/// `SEE genericname` shows the generic; per-method listing is a
/// follow-up that needs a generic→methods map.
pub fn build_docs(
    items: &[ast::Item],
    source: &str,
) -> std::collections::HashMap<String, ast::WordDoc> {
    use ast::Item;
    let mut out = std::collections::HashMap::new();
    // Slice source[span] safely on byte boundaries.
    let slice = |span: &error::Span| -> String {
        let start = span.start.byte_offset as usize;
        let end = span.end.byte_offset as usize;
        source.get(start..end)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    for item in items {
        let (name, kind, effect, src, detail) = match item {
            Item::Definition(d) => (
                d.name.clone(), "colon definition",
                d.effect.as_ref().map(render_effect).unwrap_or_default(),
                slice(&d.span), String::new(),
            ),
            Item::Generic(g) => (
                g.name.clone(), "generic",
                render_effect(&g.effect), slice(&g.span), String::new(),
            ),
            Item::Constant(c) => {
                let detail = match &c.value {
                    ast::ConstValue::Int(v)   => format!("value: {v}"),
                    ast::ConstValue::Float(v) => format!("value: {v}"),
                    ast::ConstValue::Computed(_) => "value: (computed)".to_string(),
                };
                (c.name.clone(), "constant", String::new(), slice(&c.span), detail)
            }
            Item::Variable(v) => (
                v.name.clone(), "variable", String::new(), slice(&v.span), String::new(),
            ),
            Item::Value(v) => (
                v.name.clone(), "value", String::new(), slice(&v.span), String::new(),
            ),
            Item::Class(c) => {
                let slots = c.slots.iter()
                    .map(|s| s.name.clone()).collect::<Vec<_>>().join(" ");
                let mut detail = if slots.is_empty() {
                    "slots: (none)".to_string()
                } else {
                    format!("slots: {slots}")
                };
                if let Some(parent) = &c.extends {
                    detail.push_str(&format!("   extends: {parent}"));
                }
                (c.name.clone(), "class", String::new(), slice(&c.span), detail)
            }
            Item::Collection(cl) => (
                cl.name.clone(), "collection", String::new(), slice(&cl.span), String::new(),
            ),
            Item::Create(cd) => (
                cd.name.clone(), "data buffer (CREATE)", String::new(),
                slice(&cd.span), String::new(),
            ),
            Item::Template(t) => (
                t.name.clone(), "defining word (CREATE/DOES>)", String::new(),
                slice(&t.span), String::new(),
            ),
            // Methods share the generic's name (skip — see doc above);
            // TemplateInstance / RawFactor / TopLevel aren't named
            // definitions a user would `SEE`.
            _ => continue,
        };
        out.insert(name.to_ascii_lowercase(), ast::WordDoc {
            kind: kind.to_string(),
            effect,
            source: src,
            detail,
        });
    }
    out
}

/// Resolve `NEEDS` directives by splicing the referenced files' parsed
/// items into `prog`, in place, **once per file** for the session.
///
/// This is the Rust-driven include-once: a parsing word the front end
/// expands at compile time rather than a runtime call.  Composing at
/// the AST (parse each file, concatenate into one module, lower once)
/// keeps a single compilation unit — so an included file's definitions
/// are visible to code that follows the `NEEDS`, which a runtime
/// `INCLUDED` can't offer.
///
/// * `base_dir` — directory the directive's path is relative to.
///   `None` at the top level (paths resolve against the process CWD,
///   like `INCLUDED`); for a file pulled in here, its own directory,
///   so a library's `NEEDS sibling.f` finds the sibling.
/// * Dedup key is the canonical path (falling back to the joined path
///   string).  The key is inserted **before** recursing, so an import
///   cycle terminates instead of looping.
/// * `docs` accumulates each included file's SEE records, built against
///   that file's own source; `or_insert` means an outer file's doc for
///   a name wins over an inner one (newest-defined-wins is applied by
///   the caller layering this over `ctx.docs`).
fn expand_needs(
    prog: ast::Program,
    ctx: &mut CompileContext,
    base_dir: Option<&std::path::Path>,
    docs: &mut std::collections::HashMap<String, ast::WordDoc>,
) -> Result<ast::Program, String> {
    let mut out: Vec<ast::Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items {
        match item {
            ast::Item::Needs { path, .. } => {
                // Resolve the path relative to the including file.
                let full = match base_dir {
                    Some(dir) => dir.join(&path),
                    None => std::path::PathBuf::from(&path),
                };
                // Dedup key: canonical path if possible, else the
                // joined string (so a not-yet-readable path still
                // dedups consistently and the read error surfaces).
                let key = std::fs::canonicalize(&full)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| full.to_string_lossy().into_owned());
                if !ctx.included.insert(key) {
                    // Already pulled in this session — expand to nothing.
                    continue;
                }
                let contents = std::fs::read_to_string(&full)
                    .map_err(|e| format!(
                        "NEEDS: cannot read {}: {e}", full.display()))?;
                let toks = lex(&contents).map_err(|e| e.to_string())?;
                let sub = parse(&toks).map_err(|e| e.to_string())?;
                // SEE docs for the included file, against its own text.
                for (name, doc) in build_docs(&sub.items, &contents) {
                    docs.entry(name).or_insert(doc);
                }
                // Recurse with this file's directory as the new base so
                // its own NEEDS resolve relative to it.
                let child_base = full.parent().map(|p| p.to_path_buf());
                let sub = expand_needs(sub, ctx, child_base.as_deref(), docs)?;
                out.extend(sub.items);
            }
            other => out.push(other),
        }
    }
    Ok(ast::Program { items: out })
}
