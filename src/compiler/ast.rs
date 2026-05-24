//! Abstract syntax for ANS Forth.  Phase 2.2 — the non-control-flow
//! subset: definitions, literals, word references, and top-level
//! code.  Control flow (IF/THEN, DO/LOOP, BEGIN/UNTIL, CASE/OF, …)
//! lands in milestones 2.4–2.6 as additional `Expr` variants.
//!
//! Design notes:
//!
//! * `Span` everywhere — every node carries its source position so
//!   error messages and (later) the IR emitter can echo line/column.
//! * `Definition` carries a *parsed* `StackEffect` even though the
//!   effect annotation in Forth is technically a comment.  ANS
//!   distinguishes the two by structure (`--` inside parens); we
//!   pluck the effect form at parse time so resolve / type-check
//!   doesn't have to re-derive it from a generic block comment.
//! * Top-level forms (outside `:`/`;`) are grouped into one
//!   `Item::TopLevel` per parse — they execute in order at image
//!   load time, just as ANS Forth would interpret them.

use super::error::Span;
use super::lex::StringKind;

/// A full parsed source file.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

/// Top-level item — either a `:` definition or a run of top-level
/// (interpreter-state) code.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A `: name ( ... ) body ;` definition.
    Definition(Definition),
    /// One or more expressions outside any `:` definition.  They
    /// execute in order at load time.  Adjacent top-level
    /// expressions are folded into a single `TopLevel` for
    /// efficiency.
    TopLevel { exprs: Vec<Expr>, span: Span },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    /// Source-case name.  Resolve lowercases for dictionary lookup.
    pub name: String,
    /// Span of the name token itself (for "redefinition of X" errors).
    pub name_span: Span,
    /// Optional stack-effect annotation.  Forth allows `:` definitions
    /// without one (just a regular block comment), but we strongly
    /// prefer them and the formatter/linter will warn when absent.
    pub effect: Option<StackEffect>,
    /// Body expressions, in source order.
    pub body: Vec<Expr>,
    /// Span from `:` through the `;`.
    pub span: Span,
}

/// One ANS stack-effect annotation: `( a b -- c )`.  May contain
/// dashes-only on either side (empty input or output).  Names are
/// kept as strings — we don't need to interpret them at parse time,
/// only carry them through for diagnostics and (eventually) effect
/// checking against inferred shapes.
#[derive(Clone, Debug, PartialEq)]
pub struct StackEffect {
    /// Items before the `--`.
    pub inputs:  Vec<String>,
    /// Items after the `--`.
    pub outputs: Vec<String>,
    /// Span of the surrounding parens.
    pub span:    Span,
}

/// An expression — anything that goes inside a definition body, or
/// at top level.
///
/// Phase 2.2 introduced `Lit` and `WordRef`.
/// Phase 2.4 adds the structured control-flow forms.  The keywords
/// themselves (`if`, `else`, `then`, `begin`, `until`, `while`,
/// `repeat`, `again`) are NOT word references — the parser recognises
/// them as syntax and folds them into these nodes.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Lit(Literal),
    WordRef { name: String, span: Span },

    /// `flag IF then-body [ELSE else-body] THEN`.
    /// Consumes the flag at runtime; non-zero → then-body, zero →
    /// else-body (or nothing if absent).  Maps to Factor's
    /// `[ then ] [ else ] if` combinator (`when` when else is empty).
    If {
        then_body: Vec<Expr>,
        /// `None` when source has no ELSE clause.
        else_body: Option<Vec<Expr>>,
        span: Span,
    },

    /// `BEGIN body flag UNTIL`.
    /// Loops: execute body, pop flag, if flag = 0 loop back to BEGIN.
    /// Equivalently: continue while flag is false.
    BeginUntil { body: Vec<Expr>, span: Span },

    /// `BEGIN pred WHILE body REPEAT`.
    /// Loops: execute pred (must leave a flag), pop flag, if flag is
    /// true execute body and loop, else exit.  Maps directly to
    /// Factor's `[ pred ] [ body ] while`.
    BeginWhileRepeat {
        pred: Vec<Expr>,
        body: Vec<Expr>,
        span: Span,
    },

    /// `BEGIN body AGAIN`.
    /// Infinite loop.  Exits only via LEAVE or EXIT (handled in
    /// later milestones — for now this emits as Factor's
    /// `[ body t ] loop` which is genuinely infinite).
    BeginAgain { body: Vec<Expr>, span: Span },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Int   { value: i64, span: Span },
    Float { value: f64, span: Span },
    Str   { value: String, kind: StringKind, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Lit(Literal::Int   { span, .. }) => *span,
            Expr::Lit(Literal::Float { span, .. }) => *span,
            Expr::Lit(Literal::Str   { span, .. }) => *span,
            Expr::WordRef { span, .. } => *span,
            Expr::If               { span, .. } => *span,
            Expr::BeginUntil       { span, .. } => *span,
            Expr::BeginWhileRepeat { span, .. } => *span,
            Expr::BeginAgain       { span, .. } => *span,
        }
    }
}
