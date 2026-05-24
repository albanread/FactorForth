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

/// Top-level item — either a `:` definition, a run of top-level
/// (interpreter-state) code, or a defining-word form (VARIABLE /
/// CONSTANT / FCONSTANT).
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A `: name ( ... ) body ;` definition.
    Definition(Definition),
    /// One or more expressions outside any `:` definition.  They
    /// execute in order at load time.  Adjacent top-level
    /// expressions are folded into a single `TopLevel` for
    /// efficiency.
    TopLevel { exprs: Vec<Expr>, span: Span },
    /// `VARIABLE name` — declares a one-cell allocation accessible
    /// via `name` (pushes address) and `@`/`!`/`+!` (memory ops).
    /// Whether it gets the narrow (Factor SYMBOL) or wide (nf-addr
    /// tuple) emission depends on escape analysis (sema).
    Variable(VariableDef),
    /// `<literal> CONSTANT name` — defines `name` to push a value
    /// known at compile time.  Folded into a Factor `CONSTANT:`
    /// so callers see a literal at use sites.
    Constant(ConstantDef),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableDef {
    pub name: String,
    pub name_span: Span,
    /// Span from `VARIABLE` keyword through the name token.
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstantDef {
    pub name: String,
    pub name_span: Span,
    pub value: ConstValue,
    /// Span from the value-expression through the name token.
    pub span: Span,
    /// Whether the source used `CONSTANT` (integer-shaped) or
    /// `FCONSTANT` (float-shaped).  Both flow through the same
    /// AST node; the discriminator matters for emit.
    pub flavour: ConstFlavour,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstFlavour {
    /// ANS `CONSTANT` — integer cell.
    Cell,
    /// ANS `FCONSTANT` — IEEE-754 double.
    Float,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
}

impl ConstValue {
    pub fn as_int(self) -> Option<i64> { match self { ConstValue::Int(v) => Some(v), _ => None } }
    pub fn as_float(self) -> Option<f64> { match self { ConstValue::Float(v) => Some(v), _ => None } }
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

    /// `LIMIT START DO body LOOP`  (or `?DO`, or `+LOOP`).
    ///
    /// At runtime, the two operands beneath the DO marker are taken
    /// as `( limit start -- )`.  The body runs with the loop index
    /// accessible as `I` (innermost) or `J` (next-outer).
    ///
    /// `is_qdo` distinguishes `?DO` (skip if limit == start) from
    /// `DO` (always run at least once).
    ///
    /// `loop_kind`:
    ///   - `Plus1`: terminator is `LOOP`; step is +1 each iteration,
    ///     injected at emit time.
    ///   - `PlusN`: terminator is `+LOOP`; the body itself produces
    ///     the step on top of the stack at end-of-body.
    DoLoop {
        is_qdo: bool,
        body: Vec<Expr>,
        loop_kind: LoopKind,
        span: Span,
    },

    /// `n CASE  v1 OF body1 ENDOF  v2 OF body2 ENDOF  [default]  ENDCASE`.
    ///
    /// Each `CaseArm` carries the expressions that produce the match
    /// value (before `OF`) and the body (between `OF` and `ENDOF`).
    /// `default` is the optional run-of-expressions between the last
    /// `ENDOF` and `ENDCASE`.
    ///
    /// ANS semantics:
    ///   - At runtime the dispatch value is on the stack at CASE entry.
    ///   - Each OF dups the dispatch, compares, and on match drops both
    ///     copies and runs the body, then jumps past ENDCASE.
    ///   - On mismatch, the dup is consumed by `=`, the dispatch
    ///     remains; control falls through to the next OF.
    ///   - If no OF matches, the default (if any) runs with the
    ///     dispatch value still on the stack.
    ///   - ENDCASE drops the dispatch value.  Convention: default
    ///     leaves it on the stack for ENDCASE to drop.
    Case {
        arms: Vec<CaseArm>,
        default: Option<Vec<Expr>>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaseArm {
    /// Expressions before `OF` that produce the match value on the
    /// stack at OF time.  Often a single literal, but ANS allows any
    /// expression here.
    pub match_expr: Vec<Expr>,
    /// Expressions between `OF` and `ENDOF`.
    pub body: Vec<Expr>,
    /// Span from this arm's first match-expr token through `ENDOF`.
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopKind {
    /// Terminator `LOOP`: step is implicitly +1.
    Plus1,
    /// Terminator `+LOOP`: body leaves step on the stack.
    PlusN,
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
            Expr::DoLoop           { span, .. } => *span,
            Expr::Case             { span, .. } => *span,
        }
    }
}
