//! Word resolution.  ANS Forth word name → Factor target binding.
//!
//! The dictionary is the **single source of truth** for which ANS
//! words NewFactor supports.  Resolve looks up each `WordRef` in
//! this dictionary and annotates it with its emit target:
//!
//!   - `Target::Builtin { factor_name }` — emit the name verbatim.
//!     The Factor IR's `USING:` line imports `vocab` so it's in scope.
//!   - `Target::QualifiedBuiltin { vocab, factor_name }` — emit as
//!     `vocab:factor_name` to dodge ambiguity (e.g. `.` collides
//!     between `prettyprint` and `forth.runtime`).
//!   - `Target::UserDefined { factor_name }` — emit verbatim; the
//!     name is a Forth-side `:` definition we've already compiled.
//!
//! Unknown words produce `ResolveError::UnknownWord` with the ANS
//! token's span — never any Factor frame.
//!
//! The dictionary in this file is the Phase 2.3 minimum: the words
//! needed to run `: square ( n -- n^2 ) dup * ; 5 square .`.
//! Later milestones grow it; the goal is for the table to stay
//! data-driven so adding ANS words doesn't touch this code.

use std::collections::HashMap;

use super::ast::{Definition, Expr, Item, Literal, Program};
use super::error::Span;

/// Which Factor name to emit for a given ANS word.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Emit `factor_name` bare; vocab must be in `USING:`.
    Builtin { vocab: &'static str, factor_name: &'static str },
    /// Emit `vocab:factor_name` — always disambiguates.
    QualifiedBuiltin { vocab: &'static str, factor_name: &'static str },
    /// User-defined word from this compilation unit.
    UserDefined { factor_name: String },
}

impl Target {
    /// What goes into the IR for this word reference.
    pub fn to_factor_token(&self) -> String {
        match self {
            Target::Builtin { factor_name, .. } => (*factor_name).to_string(),
            Target::QualifiedBuiltin { vocab, factor_name } =>
                format!("{vocab}:{factor_name}"),
            Target::UserDefined { factor_name } => factor_name.clone(),
        }
    }

    /// Which Factor vocab the IR needs to import (`USING:` clause).
    /// `None` for user-defined (they're emitted into the current
    /// vocab, no import needed).
    pub fn vocab(&self) -> Option<&'static str> {
        match self {
            Target::Builtin { vocab, .. } => Some(*vocab),
            Target::QualifiedBuiltin { vocab, .. } => Some(*vocab),
            Target::UserDefined { .. } => None,
        }
    }
}

/// Resolution errors.  Carries the offending span; the Display impl
/// produces ANS-style messages with line/column.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    UnknownWord { name: String, at: Span },
    RedefinedWord { name: String, at: Span, prev: Span },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownWord { name, at } =>
                write!(f, "unknown word `{name}` at {at}"),
            ResolveError::RedefinedWord { name, at, prev } =>
                write!(f, "redefinition of `{name}` at {at} (previously defined at {prev})"),
        }
    }
}

impl std::error::Error for ResolveError {}

// ─── Built-in dictionary ────────────────────────────────────────────────────

/// The static dictionary of ANS → Factor mappings.  Grows as
/// milestones land.  Lookup is case-insensitive on the ANS side.
///
/// Vocabs used (must match what `emit::vocabs_needed` knows about):
///
///   - `kernel`           dup, drop, swap, over, rot
///   - `math`             +, -, *, /, mod
///   - `math.order`       <, >, <=, >=
///   - `forth.runtime`    ANS-specific I/O, memory, booleans
///   - `io`               flush
///
/// Words listed as `QualifiedBuiltin` are ambiguous in Factor's
/// default search path (e.g. `.` exists in `prettyprint` and in
/// `forth.runtime`); emit fully qualified to avoid the parser's
/// "resolves to more than one word" error.
fn builtin_table() -> HashMap<&'static str, Target> {
    use Target::*;
    let entries: &[(&str, Target)] = &[
        // Stack words ─ kernel
        ("dup",  Builtin { vocab: "kernel", factor_name: "dup"  }),
        ("drop", Builtin { vocab: "kernel", factor_name: "drop" }),
        ("swap", Builtin { vocab: "kernel", factor_name: "swap" }),
        ("over", Builtin { vocab: "kernel", factor_name: "over" }),
        ("rot",  Builtin { vocab: "kernel", factor_name: "rot"  }),
        ("nip",  Builtin { vocab: "kernel", factor_name: "nip"  }),
        ("tuck", Builtin { vocab: "kernel", factor_name: "tuck" }),

        // Arithmetic ─ math (Factor's `+ - * /` are not in `kernel`)
        ("+",    Builtin { vocab: "math", factor_name: "+"   }),
        ("-",    Builtin { vocab: "math", factor_name: "-"   }),
        ("*",    Builtin { vocab: "math", factor_name: "*"   }),
        ("/",    Builtin { vocab: "math", factor_name: "/i"  }),  // ANS / is integer-divide
        ("mod",  Builtin { vocab: "math", factor_name: "mod" }),
        ("negate", Builtin { vocab: "math", factor_name: "neg" }),

        // I/O — `.` collides with prettyprint, so always FQ
        (".",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "." }),
        ("cr", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "cr" }),
        ("emit", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "emit" }),
        ("space", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "space" }),

        // Memory model — `@` collides with math.ratios, so always FQ
        ("@",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "@" }),
        ("!",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-!" }),
        ("c@",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "c@" }),
        ("c!",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-c!" }),
        ("+!",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-+!" }),
    ];
    entries.iter().map(|(k, v)| (*k, v.clone())).collect()
}

// ─── Resolution driver ──────────────────────────────────────────────────────

/// A program after resolution: every word reference annotated with
/// its emit target, every definition's name reachable as a built-in.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub program: Program,
    /// Per-`Expr::WordRef` → resolved target.  Keyed by source span
    /// (which is unique-per-token) so we don't have to mutate the AST.
    pub word_targets: HashMap<Span, Target>,
    /// User-defined word names from this compilation unit, with the
    /// span of their `:` definition for redefinition diagnostics.
    pub user_words: HashMap<String, Span>,
}

pub fn resolve(prog: Program) -> Result<Resolved, ResolveError> {
    let builtins = builtin_table();
    let mut user_words: HashMap<String, Span> = HashMap::new();

    // Pass 1: collect user-defined word names so forward references
    // and recursion resolve correctly.  ANS Forth allows defining a
    // word that calls a later-defined word so long as parsing order
    // doesn't matter at runtime; we follow that.
    for item in &prog.items {
        if let Item::Definition(d) = item {
            let lc = d.name.to_ascii_lowercase();
            if let Some(prev) = user_words.get(&lc) {
                return Err(ResolveError::RedefinedWord {
                    name: d.name.clone(),
                    at: d.name_span,
                    prev: *prev,
                });
            }
            user_words.insert(lc, d.name_span);
        }
    }

    // Pass 2: resolve every WordRef in every body and at top level.
    let mut word_targets: HashMap<Span, Target> = HashMap::new();
    for item in &prog.items {
        match item {
            Item::Definition(d) => resolve_exprs(&d.body, &builtins, &user_words, &mut word_targets)?,
            Item::TopLevel { exprs, .. } => resolve_exprs(exprs, &builtins, &user_words, &mut word_targets)?,
        }
    }

    Ok(Resolved { program: prog, word_targets, user_words })
}

fn resolve_exprs(
    exprs: &[Expr],
    builtins: &HashMap<&'static str, Target>,
    user_words: &HashMap<String, Span>,
    out: &mut HashMap<Span, Target>,
) -> Result<(), ResolveError> {
    for e in exprs {
        match e {
            Expr::Lit(_) => {}
            Expr::WordRef { name, span } => {
                let lc = name.to_ascii_lowercase();
                // User-defined wins over builtins for the same name —
                // ANS Forth's "most recent definition wins" rule.
                if user_words.contains_key(&lc) {
                    out.insert(*span, Target::UserDefined {
                        factor_name: factor_user_name(&lc),
                    });
                } else if let Some(t) = builtins.get(lc.as_str()) {
                    out.insert(*span, t.clone());
                } else {
                    return Err(ResolveError::UnknownWord {
                        name: name.clone(), at: *span,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Mangle an ANS word name into a Factor-safe identifier.  Most ANS
/// names are already valid Factor identifiers (`square`, `mb-row`,
/// `dup`).  We forward-map the few that aren't and lowercase the
/// rest.  ANS case-insensitivity → lowercase canonical form.
///
/// Currently no mangling beyond lowercasing is needed for any name
/// the milestone uses.  When ANS-reserved tokens like `!` start
/// appearing as user-defined names, this is where the rename
/// happens.
pub(crate) fn factor_user_name(ans_lc: &str) -> String {
    // Trivial pass-through; extend as needed.
    ans_lc.to_string()
}

/// Helper for emit.rs: which vocabs does this resolved program need
/// to import in its `USING:` clause?  Returns a sorted, deduplicated
/// list.  Always includes `kernel` and `io` (for `flush` on the
/// driver path) so callers don't need to plumb those separately.
pub fn vocabs_needed(r: &Resolved) -> Vec<&'static str> {
    let mut set: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    set.insert("kernel");
    set.insert("io");
    for t in r.word_targets.values() {
        if let Some(v) = t.vocab() { set.insert(v); }
    }
    set.into_iter().collect()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{lex, parse};

    fn resolve_str(src: &str) -> Result<Resolved, ResolveError> {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        resolve(prog)
    }

    #[test]
    fn known_builtin_resolves() {
        let r = resolve_str("dup *").unwrap();
        assert_eq!(r.word_targets.len(), 2);
    }

    #[test]
    fn ans_dot_resolves_to_forth_runtime() {
        let r = resolve_str("42 .").unwrap();
        let target = r.word_targets.values().next().unwrap();
        assert!(matches!(target,
            Target::QualifiedBuiltin { vocab: "forth.runtime", factor_name: "." }));
    }

    #[test]
    fn user_defined_resolves() {
        let r = resolve_str(": square dup * ; 5 square").unwrap();
        // The `square` call at top level must resolve to user-defined.
        let found = r.word_targets.values()
            .any(|t| matches!(t, Target::UserDefined { factor_name } if factor_name == "square"));
        assert!(found, "expected square to resolve as UserDefined");
    }

    #[test]
    fn case_insensitive_lookup() {
        let r = resolve_str("DUP +").unwrap();
        assert_eq!(r.word_targets.len(), 2);
    }

    #[test]
    fn unknown_word_errors() {
        let err = resolve_str("blortz").unwrap_err();
        assert!(matches!(err, ResolveError::UnknownWord { ref name, .. } if name == "blortz"));
    }

    #[test]
    fn redefinition_errors() {
        let err = resolve_str(": foo 1 ; : foo 2 ;").unwrap_err();
        assert!(matches!(err, ResolveError::RedefinedWord { .. }));
    }

    #[test]
    fn vocabs_needed_includes_runtime_for_dot() {
        let r = resolve_str("42 .").unwrap();
        let v = vocabs_needed(&r);
        assert!(v.contains(&"forth.runtime"));
        assert!(v.contains(&"kernel"));
    }
}
