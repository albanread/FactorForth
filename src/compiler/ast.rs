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

    /// `CREATE name [N ALLOT | N CELLS ALLOT]` — defines `name`
    /// to push a base address for a named data buffer.  Size is
    /// captured at parse time from any immediately-following
    /// ALLOT pattern.  DOES> is a separate milestone; CREATE
    /// without DOES> just exposes the address (same as VARIABLE
    /// but variable-sized).
    ///
    /// CREATE remains as the escape hatch for novel data shapes
    /// — the standard library's `array` / `farray` / `cbuffer`
    /// cover the common cases without exposing pointer arithmetic.
    Create(CreateDef),

    /// Standard application-oriented collection defining-words.
    /// `4 array primes`, `240 farray xs`, `80 cbuffer line`.
    /// Each instance is callable as `( idx -- addr )`; user
    /// then uses `@`/`!` (array), `f@`/`f!` (farray), or `c@`/`c!`
    /// (cbuffer) to access the element.
    Collection(CollectionDef),

    /// A `:` definition containing both CREATE and DOES> — a
    /// defining-word template.  Captured at parse time; expanded
    /// at sema time when user code writes `<args> name <newname>`.
    ///
    /// Conceptually this is a closure factory:
    ///   - `constructor` runs at instantiation time, consuming
    ///     args and recording how much state to allocate.
    ///   - `does_body` becomes the runtime body of the created
    ///     word, run with the state's address on the stack.
    Template(TemplateDef),

    /// Result of expanding a template invocation.  Emitted with
    /// the same shape as Collection but the accessor body is the
    /// captured does_body of the source template.
    TemplateInstance(TemplateInstanceDef),

    /// `<expr> VALUE name` — see [`ValueDef`].  Defines a polymorphic
    /// settable named cell; the slot accepts any Factor value type.
    Value(ValueDef),

    /// `CLASS: name [EXTENDS parent] SLOT: x SLOT: y ... ;` — defines a
    /// record class with named slots.  Lowers to a Factor `TUPLE:`
    /// declaration plus auto-generated constructor (`<name>`) and
    /// accessor (`name>slot`, `slot>>name`) `:` definitions.
    Class(ClassDef),

    /// `GENERIC: name ( a -- d )` — declares a generic function with a
    /// fixed stack-effect signature.  Methods get attached via
    /// `Item::Method`.  Effect annotation is required (drives dispatch
    /// arity and stack-effect inference for callers).
    Generic(GenericDef),

    /// `METHOD: gname ( a:cls -- d ) body ;` — defines a method that
    /// specialises an existing generic function on the named class.
    /// Sprint 1 only supports single-dispatch on the first input.
    Method(MethodDef),

    /// Escape hatch for emitting raw Factor source.  Used by
    /// `lower_classes` to inject `TUPLE:` / `GENERIC:` / `M:`
    /// declarations that have no Forth-side AST equivalent.  Emit
    /// passes through verbatim.
    RawFactor(RawFactorItem),

    /// `NEEDS path` — include-once directive.  Resolved entirely in the
    /// Rust front end *before* any later pass runs: the expansion pass
    /// (`compiler::expand_needs`) reads the file, parses it, and splices
    /// its items into this program at the directive's position — but
    /// only the first time a given file is seen in the session (dedup
    /// keyed on the canonical path, held in `CompileContext`).  A
    /// repeat `NEEDS` of an already-loaded file expands to nothing.
    ///
    /// Because expansion happens up front, no downstream pass (resolve,
    /// effect, emit, …) ever encounters a live `Needs` — their match
    /// arms exist only for exhaustiveness.
    Needs { path: String, span: Span },
}

/// Introspection record for one defined name, consumed by `SEE`.
///
/// Built at compile time from the AST + the original source text,
/// and persisted across evals in `CompileContext.docs` so `SEE`
/// works on words defined in an earlier REPL line.  Everything here
/// is a pre-rendered string so emit can splice it straight into a
/// literal print without re-deriving anything.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WordDoc {
    /// Human-readable kind: "colon definition", "generic", "method",
    /// "constant", "variable", "value", "class", "collection",
    /// "template".
    pub kind: String,
    /// Rendered stack effect, e.g. "( n -- n2 )".  Empty when the
    /// definition carries no effect annotation.
    pub effect: String,
    /// The original ANS source text of the definition, verbatim.
    /// Empty when not retained (e.g. builtins).
    pub source: String,
    /// Optional one-line extra detail (e.g. a class's slot list, or
    /// a constant's value).  Empty when nothing to add.
    pub detail: String,
}

/// `CLASS:` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub name_span: Span,
    /// Optional `EXTENDS parent` clause.  Single inheritance only in
    /// sprint 1; Factor TUPLE: supports it directly via `<` syntax.
    pub extends: Option<String>,
    pub slots: Vec<SlotDef>,
    /// Span from `CLASS:` through the closing `;`.
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotDef {
    pub name: String,
    pub name_span: Span,
}

/// `GENERIC:` declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct GenericDef {
    pub name: String,
    pub name_span: Span,
    /// Required stack effect — drives dispatch arity and effect
    /// inference for callers.
    pub effect: StackEffect,
    pub span: Span,
}

/// `METHOD:` definition.
#[derive(Clone, Debug, PartialEq)]
pub struct MethodDef {
    pub generic_name: String,
    pub generic_name_span: Span,
    /// One specialiser per dispatched input.  Sprint 1: at most one
    /// (single dispatch on the first stack input).  The specialiser
    /// names a class to dispatch on.
    pub specializers: Vec<MethodSpecializer>,
    /// Full declared effect, including non-dispatched inputs and the
    /// outputs.  Specialiser-bearing inputs appear as `name:class` in
    /// source; we strip the `:class` part into MethodSpecializer and
    /// keep the bare names here.
    pub effect: StackEffect,
    pub body: Vec<Expr>,
    pub span: Span,
    /// Which auxiliary slot this method occupies.  Primary methods
    /// (the default `METHOD:` keyword) compute the result.  Before
    /// methods run before primary in most-specific-first order and
    /// return nothing.  After methods run after primary in
    /// least-specific-first order and return nothing.  Aux methods
    /// must agree with the generic's input arity (they observe the
    /// same arguments) but their declared effect's outputs are
    /// always empty.
    pub kind: MethodKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MethodKind {
    /// `METHOD:` — primary method.  Computes the value.
    Primary,
    /// `METHOD-BEFORE:` — runs before the primary dispatch in
    /// most-specific-first order, return value ignored.
    Before,
    /// `METHOD-AFTER:` — runs after the primary dispatch in
    /// least-specific-first order, return value ignored.
    After,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MethodSpecializer {
    /// Parameter position (0 = first input, etc.) — sprint 1 always 0.
    pub position: u32,
    /// Parameter name as written by the user (for diagnostics).
    pub param_name: String,
    /// Class name the method dispatches on.
    pub class_name: String,
    pub at: Span,
}

/// Raw Factor source injection — see [`Item::RawFactor`].
#[derive(Clone, Debug, PartialEq)]
pub struct RawFactorItem {
    pub source: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateDef {
    pub name: String,
    pub name_span: Span,
    pub effect: Option<StackEffect>,
    /// Expressions BETWEEN `create` and `does>`, excluding both
    /// markers.  Runs at template-instantiation time, consuming
    /// the args the caller pushed before the template name.
    /// First-cut grammar: a single multiplier word (`cells` or
    /// `chars`) followed by `allot`, or just `allot`.
    pub constructor: Vec<Expr>,
    /// Expressions AFTER `does>`, excluding the marker.  Becomes
    /// the runtime body of created instances.  The state's
    /// address is on top of the stack at entry; the body
    /// indexes / transforms / fetches as needed.
    pub does_body: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateInstanceDef {
    /// The newly-defined word name (the token after the template
    /// in source).
    pub name: String,
    pub name_span: Span,
    /// Name of the source template, for diagnostics.
    pub template_name: String,
    /// Total bytes to allocate for this instance's state, computed
    /// at expansion time from the args + constructor.
    pub allocated_bytes: u32,
    /// The does_body captured from the source template, expanded
    /// into the accessor.
    pub does_body: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectionDef {
    pub name: String,
    pub name_span: Span,
    pub kind: CollectionKind,
    /// Number of *elements* (not bytes).  array/farray count cells;
    /// cbuffer counts bytes.  Backing storage is `count * elt_size`
    /// bytes where elt_size is 8 (cells) or 1 (bytes).
    pub count: u32,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CollectionKind {
    /// `array`: n cells, accessed with `@`/`!`.  Integer semantics.
    Array,
    /// `farray`: n cells, accessed with `f@`/`f!`.  IEEE double.
    FArray,
    /// `cbuffer`: n bytes, accessed with `c@`/`c!`.
    CBuffer,
}

impl CollectionKind {
    /// Bytes per element.  Drives both backing-storage size and
    /// the `cells`/`chars` multiplier in the accessor.
    pub fn elt_size(self) -> u32 {
        match self {
            CollectionKind::Array | CollectionKind::FArray => 8,
            CollectionKind::CBuffer => 1,
        }
    }

    /// The ANS keyword used to introduce this collection.
    pub fn keyword(self) -> &'static str {
        match self {
            CollectionKind::Array   => "array",
            CollectionKind::FArray  => "farray",
            CollectionKind::CBuffer => "cbuffer",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateDef {
    pub name: String,
    pub name_span: Span,
    /// Total byte count to allocate for the buffer.  Always a
    /// multiple of 8 when the source used `CELLS ALLOT`; arbitrary
    /// bytes when plain `ALLOT`.  Zero means CREATE without any
    /// ALLOT — useful for marker-style words.
    pub allotted_bytes: u32,
    /// Span from `CREATE` through the last consumed ALLOT token
    /// (or the name token if there was no ALLOT).
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableDef {
    pub name: String,
    pub name_span: Span,
    /// Span from `VARIABLE` keyword through the name token.
    pub span: Span,
}

/// `<expr> VALUE name` — a polymorphic single-slot mutable, settable
/// via `TO name`.  Unlike VARIABLE, VALUE has no address — it's a
/// named getter/setter pair onto a Factor global.  Since Factor's
/// `get-global` / `set-global` are tag-polymorphic, the slot can
/// hold an integer, float, string, quotation, anything Factor
/// represents on the data stack.  The initial-value expression
/// runs once at load time; subsequent stores happen at any `TO`
/// use site.  No escape analysis, no @/!, no narrow/wide split —
/// that machinery belongs to VARIABLE which has to support address
/// arithmetic.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueDef {
    pub name: String,
    pub name_span: Span,
    /// Initial-value expression sequence.  Runs once at load time
    /// to seed the underlying Factor global.  May be any pure
    /// expression that produces exactly one item.
    pub initial: Vec<Expr>,
    /// Span from the first initial-value token through the name.
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

#[derive(Clone, Debug, PartialEq)]
pub enum ConstValue {
    Int(i64),
    Float(f64),
    /// Computed: the value is the result of evaluating the
    /// expression sequence at runtime once.  Emitted as
    /// `: name ( -- v ) <body> ; inline` so Factor's compiler
    /// constant-folds it to the same machine code as the
    /// literal form whenever the body is pure.
    ///
    /// Stores the pending expressions in source order (the
    /// stack-effect order they'd execute in plain Forth).
    Computed(Vec<Expr>),
}

impl ConstValue {
    pub fn as_int(&self) -> Option<i64> { match self { ConstValue::Int(v) => Some(*v), _ => None } }
    pub fn as_float(&self) -> Option<f64> { match self { ConstValue::Float(v) => Some(*v), _ => None } }
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
    /// Forth-2012 `{: name1 name2 :}` locals declared at the head of
    /// the body.  Each binds the corresponding input position
    /// (deepest-stack-item first) as a lexical local that the body
    /// references by name — re-entrant-safe by construction.  Empty
    /// here keeps the def at the plain `: name … ;` shape on emit;
    /// non-empty switches it to Factor's `::` form internally.
    pub locals: Vec<LocalDecl>,
    /// Body expressions, in source order.
    pub body: Vec<Expr>,
    /// Span from `:` through the `;`.
    pub span: Span,
}

/// One Forth-2012 local declared inside a `{: … :}` block.  Carries
/// source name + span so resolve / effect errors can point at the
/// right token.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalDecl {
    pub name: String,
    pub name_span: Span,
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

    /// `' name` — the ANS tick parsing word.  At parse time the
    /// next blank-delimited token is captured as `name`; at runtime
    /// the word's execution token (XT) is pushed onto the stack.
    /// Used with `EXECUTE`, `IS`, or stored in a variable for
    /// later dispatch.  Effect: `( -- xt )`.
    Tick { name: String, span: Span },

    /// `TO name` — store the top of stack into the VALUE named
    /// `name`.  Parsed like `'` (the next blank-delimited token is
    /// the target).  Emit lowers this to `<storage-symbol> set-global`
    /// on the VALUE's underlying Factor global.  Effect: `( x -- )`.
    To { name: String, span: Span },

    /// `SEE name` — the Programming-Tools introspection word.  Parsed
    /// like `'` and `TO` (the next blank-delimited token is the
    /// target).  At emit time the compiler looks up everything it
    /// knows about `name` — kind, declared stack effect, origin
    /// (user definition vs builtin), and the retained original ANS
    /// source — and emits code that prints that report.  Effect:
    /// `( -- )`.  This is a compile-time word: the report is built
    /// in the Rust front end (which holds the source and sema
    /// metadata) and lowered to a literal print, rather than
    /// introspecting the live Factor word at runtime.
    See { name: String, span: Span },

    /// `LET (inputs) -> (outputs) = expr,... [WHERE ...]* END` —
    /// the infix-algebraic sub-language.  Parsed by `let_lang`
    /// at lex time (the lexer captures the whole block as a
    /// single token); the parsed `LetForm` is carried here for
    /// codegen.  Effect: `( ..a in1..inN -- ..a out1..outM )`
    /// where N = inputs.len(), M = outputs.len().
    LetForm {
        form: super::let_lang::LetForm,
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
            Expr::Tick             { span, .. } => *span,
            Expr::To               { span, .. } => *span,
            Expr::See              { span, .. } => *span,
            Expr::LetForm          { span, .. } => *span,
        }
    }
}
