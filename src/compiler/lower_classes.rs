//! lower_classes — pre-resolve helper that flattens class slot
//! lists across single-inheritance chains so resolve can register
//! the right constructor/accessor names and effect inference can
//! size them correctly.
//!
//! Doesn't mutate the AST.  Just computes a `class_slots` map:
//!
//!   class-name (lowercased) → full slot list, in declaration order,
//!                              parent slots first
//!
//! Used by:
//!   - resolve: to enumerate which `<name>` / `name>slot` / `slot>>name`
//!     identifiers to register in user_words
//!   - effect inference: to size constructor and accessor effects
//!   - emit: to render `TUPLE: name [< parent] { slot } ...` and the
//!     accompanying `:` defs

use std::collections::HashMap;

use super::ast::{ClassDef, Item, Program};
use super::error::Span;

/// Walk `prog.items` in source order, combining each class's own
/// slots with its inherited slots from `prior_classes` plus any
/// earlier-in-this-compile parents.  Returns a map keyed by
/// lowercased class name.
///
/// Missing-parent isn't an error here — resolve surfaces it as
/// `NotAClass`.  We just produce a best-effort slot list (just the
/// own slots) and let resolve report the user error.
pub fn compute_class_slots(
    prog: &Program,
    prior_classes: &HashMap<String, Vec<String>>,
) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = prior_classes.clone();
    for item in &prog.items {
        if let Item::Class(c) = item {
            let mut slots: Vec<String> = Vec::new();
            if let Some(parent) = &c.extends {
                let parent_lc = parent.to_ascii_lowercase();
                if let Some(parent_slots) = out.get(&parent_lc) {
                    slots.extend_from_slice(parent_slots);
                }
            }
            for s in &c.slots {
                slots.push(s.name.to_ascii_lowercase());
            }
            out.insert(c.name.to_ascii_lowercase(), slots);
        }
    }
    out
}

/// Names a class declaration contributes to the user dictionary.
/// Caller passes the fully-flattened slot list (from
/// `compute_class_slots`).  Returns `(name, span)` pairs that resolve
/// should register in pass 1.
///
///   - the class name itself (so `M: cls gen …` resolves)
///   - `<classname>` — constructor
///   - `classname>slotname`  — getter, `( p -- v )`
///   - `slotname>>classname` — chainable setter, `( p v -- p )`
///   - `classname.slotname!` — ANS-flavoured store, `( v p -- )`
///
/// The two setters look like they overlap but are *different
/// operations* in idiom: `>>` keeps the object on the stack for
/// fluent updates (`<point> 3 x>>point 4 y>>point`), while `.slot!`
/// drops it and matches ANS `42 var !` muscle memory for users who
/// just want to mutate one field and move on.
pub fn class_synthesised_names(c: &ClassDef, all_slots: &[String]) -> Vec<(String, Span)> {
    let class_lc = c.name.to_ascii_lowercase();
    let mut names: Vec<(String, Span)> = Vec::with_capacity(2 + all_slots.len() * 3);
    names.push((class_lc.clone(), c.name_span));
    names.push((format!("<{class_lc}>"), c.name_span));
    for s in all_slots {
        names.push((format!("{class_lc}>{s}"), c.name_span));
        names.push((format!("{s}>>{class_lc}"), c.name_span));
        names.push((format!("{class_lc}.{s}!"), c.name_span));
    }
    names
}
