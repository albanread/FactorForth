//! Parser — token stream → AST.  Phase 2.2: non-control-flow only.
//!
//! The parser is recursive descent over a flat token slice.  ANS
//! Forth is structurally simple enough that we don't need a Pratt
//! parser or operator precedence — it's all left-to-right word
//! sequencing, interrupted by `:`/`;` for definitions and by
//! parsing-word annotations (stack effects) that the lexer has
//! already extracted into `BlockComment` tokens.
//!
//! Comments (line and block) are skipped EXCEPT when a block
//! comment immediately follows a `:` name and parses as a stack
//! effect — then we capture it on the `Definition`.

use super::ast::*;
use super::error::{CompileError, Pos, Span};
use super::lex::{StringKind, Tok, Token};

/// Parse a full token stream into a `Program`.
pub fn parse(toks: &[Token]) -> Result<Program, ParseError> {
    let mut p = Parser { toks, i: 0 };
    p.program()
}

// ─── Error type ─────────────────────────────────────────────────────────────

/// Parse-stage errors.  We extend `CompileError` rather than reuse
/// it so the lex/parse layers stay independently testable.  At the
/// top level (the public `compile()` driver, Phase 2.3) we'll unify.
#[derive(Clone, Debug, PartialEq)]
pub enum ParseError {
    /// `:` not followed by an identifier name.
    ExpectedDefName { at: Span },
    /// `VARIABLE` / `CONSTANT` / `FCONSTANT` not followed by a name.
    ExpectedDefiningName { keyword: &'static str, at: Span },
    /// `CONSTANT` / `FCONSTANT` with no preceding value expression.
    ConstantWithoutValue { keyword: &'static str, at: Span },
    /// CONSTANT's preceding expression is not a simple literal.
    /// Computed CONSTANT/FCONSTANT values (multi-token expressions)
    /// are a later milestone — see PLAN.md.
    NonLiteralConstantValue { keyword: &'static str, at: Span },
    /// Found `:` while already inside a `:` definition (ANS forbids
    /// nested colons).
    NestedColon { outer: Span, inner: Span },
    /// `;` outside any definition.
    StraySemicolon { at: Span },
    /// Reached EOF inside a `: ... ;` definition.
    UnterminatedDefinition { opened_at: Span },
    /// `( --- )` annotation without the `--` separator (we treat as
    /// a comment instead, but `name ( a b )` is ambiguous; we accept
    /// it as a comment and warn out-of-band).  This variant is for
    /// `( -- -- )` style errors.
    MalformedStackEffect { at: Span, reason: &'static str },

    /// A control-flow terminator (ELSE, THEN, UNTIL, WHILE, REPEAT,
    /// AGAIN) appeared without its matching opener.  Carries the
    /// stray keyword and its span.
    StrayControlWord { word: String, at: Span },

    /// EOF reached inside a control-flow block waiting for one of
    /// `expected` terminators.
    UnterminatedControl {
        opener: String,
        opened_at: Span,
        expected: &'static [&'static str],
    },
    /// The LET sub-parser rejected the block contents.  Carries
    /// the inner parser's message verbatim.
    LetSyntax { at: Span, reason: String },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::ExpectedDefName { at } =>
                write!(f, "expected definition name after `:` at {at}"),
            ParseError::ExpectedDefiningName { keyword, at } =>
                write!(f, "expected name after `{keyword}` at {at}"),
            ParseError::ConstantWithoutValue { keyword, at } =>
                write!(f, "`{keyword}` at {at} needs a value before it (e.g. `64 CONSTANT max`)"),
            ParseError::NonLiteralConstantValue { keyword, at } =>
                write!(f, "`{keyword}` at {at}: only literal values are supported in this milestone"),
            ParseError::NestedColon { inner, .. } =>
                write!(f, "nested `:` at {inner}: ANS forbids defining a word inside another"),
            ParseError::StraySemicolon { at } =>
                write!(f, "stray `;` at {at}: no `:` to close"),
            ParseError::UnterminatedDefinition { opened_at } =>
                write!(f, "unterminated `:` definition opened at {opened_at}"),
            ParseError::MalformedStackEffect { at, reason } =>
                write!(f, "malformed stack effect at {at}: {reason}"),
            ParseError::StrayControlWord { word, at } =>
                write!(f, "stray `{word}` at {at}: no matching opener"),
            ParseError::UnterminatedControl { opener, opened_at, expected } => {
                write!(f, "unterminated `{opener}` opened at {opened_at}; expected one of: ")?;
                for (i, e) in expected.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "`{e}`")?;
                }
                Ok(())
            }
            ParseError::LetSyntax { at, reason } =>
                write!(f, "LET syntax error at {at}: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Result of parsing a `:` definition.  Most are plain Definitions;
/// those containing both CREATE and DOES> become Templates.
pub enum ColonResult {
    Def(Definition),
    Template(TemplateDef),
}

/// Conversion for the eventual unified error type.
impl From<ParseError> for CompileError {
    fn from(_: ParseError) -> Self {
        // Phase 2.2 stub.  Phase 2.3 introduces a wrapping
        // `Compile` variant that carries either lex or parse errors;
        // for now we just punt with a synthetic position.
        CompileError::MalformedNumber {
            token: String::new(),
            at: Span::point(Pos::START),
            reason: "parse error (see ParseError display)",
        }
    }
}

// ─── The parser ─────────────────────────────────────────────────────────────

struct Parser<'t> {
    toks: &'t [Token],
    i: usize,
}

impl<'t> Parser<'t> {
    fn peek(&self) -> Option<&'t Token> { self.toks.get(self.i) }
    fn bump(&mut self) -> Option<&'t Token> {
        let t = self.toks.get(self.i)?;
        self.i += 1;
        Some(t)
    }

    /// Skip line and block comments — used at top level and between
    /// expressions inside a body.  Returns the count skipped (for
    /// detecting "comment right after `:` name" which is the
    /// stack-effect case).
    fn skip_comments(&mut self) -> usize {
        let mut n = 0;
        while let Some(t) = self.peek() {
            match &t.kind {
                Tok::LineComment(_) | Tok::BlockComment(_) => {
                    self.i += 1; n += 1;
                }
                _ => break,
            }
        }
        n
    }

    fn program(&mut self) -> Result<Program, ParseError> {
        let mut items: Vec<Item> = Vec::new();
        // `pending` accumulates top-level expressions until either
        // EOF, a `:`/defining word boundary, or end-of-input.  The
        // defining words `CONSTANT`/`FCONSTANT` consume the most
        // recent pending expression as their value.
        let mut pending: Vec<Expr> = Vec::new();
        let mut pending_start: Option<Span> = None;

        let flush_pending = |items: &mut Vec<Item>,
                             pending: &mut Vec<Expr>,
                             pending_start: &mut Option<Span>| {
            if pending.is_empty() { return; }
            let start = pending_start.unwrap();
            let end = pending.last().unwrap().span();
            items.push(Item::TopLevel {
                exprs: std::mem::take(pending),
                span: Span { start: start.start, end: end.end },
            });
            *pending_start = None;
        };

        loop {
            self.skip_comments();
            let Some(t) = self.peek() else { break; };
            // Defining-word recognition.  Match case-insensitively.
            let opener = match &t.kind {
                Tok::Word(w) => Some(w.to_ascii_lowercase()),
                _ => None,
            };
            match opener.as_deref() {
                Some(":") => {
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    match self.colon_definition()? {
                        ColonResult::Def(d)      => items.push(Item::Definition(d)),
                        ColonResult::Template(t) => items.push(Item::Template(t)),
                    }
                }
                Some(";") => {
                    return Err(ParseError::StraySemicolon { at: t.span });
                }
                Some("variable") => {
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump(); // consume `variable`
                    let name_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName { keyword: "VARIABLE", at: kw_span },
                    )?;
                    let (name, name_span) = match &name_tok.kind {
                        Tok::Word(w) => (w.clone(), name_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: "VARIABLE", at: kw_span,
                        }),
                    };
                    self.bump();
                    items.push(Item::Variable(VariableDef {
                        name, name_span,
                        span: Span { start: kw_span.start, end: name_span.end },
                    }));
                }
                Some("needs") => {
                    // `NEEDS path` — include-once.  The next blank-
                    // delimited token is the file path (one token, like
                    // gforth's `require`; use INCLUDED for paths with
                    // spaces).  Resolved by the expand-needs pass.
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump(); // consume `needs`
                    let path_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName { keyword: "NEEDS", at: kw_span },
                    )?;
                    let (path, path_span) = match &path_tok.kind {
                        Tok::Word(w) => (w.clone(), path_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: "NEEDS", at: kw_span,
                        }),
                    };
                    self.bump(); // consume the path token
                    items.push(Item::Needs {
                        path,
                        span: Span { start: kw_span.start, end: path_span.end },
                    });
                }
                Some("create") => {
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump();
                    let name_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName { keyword: "CREATE", at: kw_span },
                    )?;
                    let (name, name_span) = match &name_tok.kind {
                        Tok::Word(w) => (w.clone(), name_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: "CREATE", at: kw_span,
                        }),
                    };
                    self.bump();
                    // Look-ahead: capture optional `N ALLOT` or
                    // `N CELLS ALLOT` immediately after the name.
                    // Both forms only match when N is a literal int.
                    let mut allotted: u32 = 0;
                    let mut end_span = name_span;
                    loop {
                        // Save current position to roll back on mismatch.
                        let save_i = self.i;
                        self.skip_comments();
                        let Some(n_tok) = self.peek() else { self.i = save_i; break; };
                        let n_val = match &n_tok.kind {
                            Tok::Int { value, .. } if *value >= 0 => *value as u32,
                            _ => { self.i = save_i; break; }
                        };
                        let n_span = n_tok.span;
                        self.bump();
                        // Check for CELLS optionally, then ALLOT.
                        self.skip_comments();
                        let mut multiplier: u32 = 1;
                        if let Some(t2) = self.peek() {
                            if let Tok::Word(w2) = &t2.kind {
                                if w2.eq_ignore_ascii_case("cells") {
                                    multiplier = 8;
                                    self.bump();
                                    self.skip_comments();
                                } else if w2.eq_ignore_ascii_case("chars") {
                                    multiplier = 1; // ANS: 1 char = 1 byte
                                    self.bump();
                                    self.skip_comments();
                                }
                            }
                        }
                        // Require ALLOT now.
                        let Some(allot_tok) = self.peek() else {
                            self.i = save_i; break;
                        };
                        match &allot_tok.kind {
                            Tok::Word(w) if w.eq_ignore_ascii_case("allot") => {
                                allotted = allotted.saturating_add(
                                    n_val.saturating_mul(multiplier),
                                );
                                end_span = allot_tok.span;
                                self.bump();
                            }
                            _ => { self.i = save_i; break; }
                        }
                        let _ = n_span;
                        // Loop to pick up further N ALLOT runs if any.
                    }
                    items.push(Item::Create(CreateDef {
                        name, name_span, allotted_bytes: allotted,
                        span: Span { start: kw_span.start, end: end_span.end },
                    }));
                }
                Some(kw @ ("array" | "farray" | "cbuffer")) => {
                    let kw_static: &'static str = match kw {
                        "array"   => "ARRAY",
                        "farray"  => "FARRAY",
                        "cbuffer" => "CBUFFER",
                        _ => unreachable!(),
                    };
                    let kw_span = t.span;
                    // Size: the immediately-preceding pending expr.
                    // Must be a non-negative literal integer.
                    let size_expr = pending.pop().ok_or(
                        ParseError::ConstantWithoutValue { keyword: kw_static, at: kw_span },
                    )?;
                    let count = match &size_expr {
                        Expr::Lit(Literal::Int { value, .. }) if *value >= 0 => *value as u32,
                        _ => return Err(ParseError::NonLiteralConstantValue {
                            keyword: kw_static, at: size_expr.span(),
                        }),
                    };
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    self.bump(); // consume the collection keyword
                    let name_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName { keyword: kw_static, at: kw_span },
                    )?;
                    let (name, name_span) = match &name_tok.kind {
                        Tok::Word(w) => (w.clone(), name_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: kw_static, at: kw_span,
                        }),
                    };
                    self.bump();
                    let kind = match kw {
                        "array"   => CollectionKind::Array,
                        "farray"  => CollectionKind::FArray,
                        "cbuffer" => CollectionKind::CBuffer,
                        _ => unreachable!(),
                    };
                    items.push(Item::Collection(CollectionDef {
                        name, name_span, kind, count,
                        span: Span { start: size_expr.span().start, end: name_span.end },
                    }));
                }
                Some(kw @ ("constant" | "fconstant")) => {
                    let kw_static: &'static str = if kw == "constant" { "CONSTANT" } else { "FCONSTANT" };
                    let kw_span = t.span;
                    if pending.is_empty() {
                        return Err(ParseError::ConstantWithoutValue {
                            keyword: kw_static, at: kw_span,
                        });
                    }
                    // Two shapes:
                    //   (1) `<literal> CONSTANT name`     - literal-only,
                    //       emitted as a Factor CONSTANT: (compile-time).
                    //   (2) `<expr> ... [F]CONSTANT name` - one or more
                    //       expressions, emitted as `: name body ;
                    //       inline` (Factor's compiler folds pure
                    //       bodies to the same machine code as the
                    //       literal form).
                    //
                    // We detect the literal shape by checking that the
                    // ONLY pending expression is a Lit.  Anything else
                    // (multiple tokens, a Word call, etc.) goes through
                    // the Computed path.
                    let value_start = pending.first().unwrap().span().start;
                    let value = if pending.len() == 1 {
                        match &pending[0] {
                            Expr::Lit(Literal::Int   { value, .. }) => Some(ConstValue::Int(*value)),
                            Expr::Lit(Literal::Float { value, .. }) => Some(ConstValue::Float(*value)),
                            _ => None,
                        }
                    } else { None };
                    let value = match value {
                        Some(v) => { pending.clear(); v }
                        None => {
                            // Consume the entire pending vec as the
                            // expression body.
                            ConstValue::Computed(std::mem::take(&mut pending))
                        }
                    };
                    pending_start = None;
                    self.bump(); // consume `constant` / `fconstant`
                    let name_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName { keyword: kw_static, at: kw_span },
                    )?;
                    let (name, name_span) = match &name_tok.kind {
                        Tok::Word(w) => (w.clone(), name_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: kw_static, at: kw_span,
                        }),
                    };
                    self.bump();
                    let flavour = if kw == "fconstant" {
                        ConstFlavour::Float
                    } else { ConstFlavour::Cell };
                    items.push(Item::Constant(ConstantDef {
                        name, name_span, value, flavour,
                        span: Span { start: value_start, end: name_span.end },
                    }));
                }
                Some("class:") => {
                    // CLASS: name [EXTENDS parent] SLOT: x SLOT: y ... ;
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump();  // consume `class:`
                    let class_def = self.class_definition(kw_span)?;
                    items.push(Item::Class(class_def));
                }
                Some("generic:") => {
                    // GENERIC: name ( a b -- d )
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump();  // consume `generic:`
                    let gen_def = self.generic_declaration(kw_span)?;
                    items.push(Item::Generic(gen_def));
                }
                Some("method:") => {
                    // METHOD: gname ( a:class -- d ) body ;
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump();  // consume `method:`
                    let method_def = self.method_definition(
                        kw_span, "METHOD:", super::ast::MethodKind::Primary)?;
                    items.push(Item::Method(method_def));
                }
                Some("method-before:") => {
                    // METHOD-BEFORE: gname ( a:class -- ) body ;
                    // Runs before primary dispatch, return value ignored.
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump();
                    let method_def = self.method_definition(
                        kw_span, "METHOD-BEFORE:", super::ast::MethodKind::Before)?;
                    items.push(Item::Method(method_def));
                }
                Some("method-after:") => {
                    // METHOD-AFTER: gname ( a:class -- ) body ;
                    // Runs after primary dispatch, return value ignored.
                    flush_pending(&mut items, &mut pending, &mut pending_start);
                    let kw_span = t.span;
                    self.bump();
                    let method_def = self.method_definition(
                        kw_span, "METHOD-AFTER:", super::ast::MethodKind::After)?;
                    items.push(Item::Method(method_def));
                }
                Some("value") => {
                    // `<expr> VALUE name` — capture all pending
                    // expressions as the initial-value body.  Unlike
                    // CONSTANT we don't peep for a literal vs. computed
                    // split: VALUE is settable, so emit always uses
                    // the same SYMBOL/get-global/set-global shape and
                    // the initial-value path runs the body once at
                    // load time.
                    let kw_span = t.span;
                    if pending.is_empty() {
                        return Err(ParseError::ConstantWithoutValue {
                            keyword: "VALUE", at: kw_span,
                        });
                    }
                    let value_start = pending.first().unwrap().span().start;
                    let initial = std::mem::take(&mut pending);
                    pending_start = None;
                    self.bump(); // consume `value`
                    let name_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName { keyword: "VALUE", at: kw_span },
                    )?;
                    let (name, name_span) = match &name_tok.kind {
                        Tok::Word(w) => (w.clone(), name_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: "VALUE", at: kw_span,
                        }),
                    };
                    self.bump();
                    items.push(Item::Value(ValueDef {
                        name, name_span, initial,
                        span: Span { start: value_start, end: name_span.end },
                    }));
                }
                _ => {
                    // Regular expression: accumulate into pending.
                    if pending_start.is_none() {
                        pending_start = Some(t.span);
                    }
                    let e = self.expr_one()?;
                    pending.push(e);
                }
            }
        }
        flush_pending(&mut items, &mut pending, &mut pending_start);
        Ok(Program { items })
    }

    /// Already at the `:` token.  Consumes through the matching `;`.
    /// Returns either a regular Definition or a Template, depending
    /// on whether the body contained both CREATE and DOES>.
    fn colon_definition(&mut self) -> Result<ColonResult, ParseError> {
        let colon = self.bump().expect("colon present");
        let colon_span = colon.span;

        // Name token must follow immediately (ignoring nothing — `:` is
        // a parsing word and ANS says the *next* whitespace-delimited
        // token is the name, period).
        let name_tok = match self.peek() {
            Some(t) => t,
            None => return Err(ParseError::ExpectedDefName { at: colon_span }),
        };
        let (name, name_span) = match &name_tok.kind {
            Tok::Word(w) => (w.clone(), name_tok.span),
            // Numbers / strings as the next token = no name given.
            _ => return Err(ParseError::ExpectedDefName { at: colon_span }),
        };
        self.bump();

        // Optional stack effect: a `( ... -- ... )` block comment
        // immediately following the name.  Other comments (line
        // comments, parens without `--`) are skipped but NOT attached
        // as effect.
        let mut effect: Option<StackEffect> = None;
        if let Some(Token { kind: Tok::BlockComment(body), span }) = self.peek() {
            if let Some(parsed) = parse_stack_effect(body, *span) {
                effect = Some(parsed);
                self.bump();
            }
            // If it wasn't an effect-shape, fall through and let
            // skip_comments handle it in the body loop.
        }

        // Body expressions until `;`.
        let mut body: Vec<Expr> = Vec::new();
        let end_span;
        loop {
            self.skip_comments();
            let Some(t) = self.peek() else {
                return Err(ParseError::UnterminatedDefinition { opened_at: colon_span });
            };
            match &t.kind {
                Tok::Word(w) if w == ";" => {
                    end_span = t.span;
                    self.bump();
                    break;
                }
                Tok::Word(w) if w == ":" => {
                    return Err(ParseError::NestedColon {
                        outer: colon_span, inner: t.span,
                    });
                }
                _ => {
                    let e = self.expr_one()?;
                    body.push(e);
                }
            }
        }

        let span = Span { start: colon_span.start, end: end_span.end };

        // Detect template shape: the body contains both `create`
        // and `does>` (case-insensitive).  If so, split and emit
        // Item::Template instead of Item::Definition.
        let create_at = body.iter().position(|e| matches!(e,
            Expr::WordRef { name: n, .. } if n.eq_ignore_ascii_case("create")));
        let does_at = body.iter().position(|e| matches!(e,
            Expr::WordRef { name: n, .. } if n.eq_ignore_ascii_case("does>")));
        match (create_at, does_at) {
            (Some(ci), Some(di)) if ci < di => {
                // Constructor = exprs strictly between CREATE and DOES>.
                // (Anything before CREATE is "pre-create setup" — rare,
                // we don't yet model it.  Anything after DOES> is the
                // runtime body.)
                let constructor: Vec<Expr> =
                    body.iter().skip(ci + 1).take(di - ci - 1).cloned().collect();
                let does_body: Vec<Expr> =
                    body.iter().skip(di + 1).cloned().collect();
                return Ok(ColonResult::Template(TemplateDef {
                    name, name_span, effect,
                    constructor, does_body, span,
                }));
            }
            _ => {}
        }
        // Default: regular `: name body ;` definition.
        Ok(ColonResult::Def(Definition {
            name, name_span, effect, body, span,
        }))
    }

    /// Already past `CLASS:`.  Consumes through the matching `;`.
    /// Grammar: `name [EXTENDS parent] (SLOT: slot-name)* ;`.
    fn class_definition(&mut self, kw_span: Span) -> Result<ClassDef, ParseError> {
        // Class name.
        let name_tok = self.peek().ok_or(
            ParseError::ExpectedDefiningName { keyword: "CLASS:", at: kw_span },
        )?;
        let (name, name_span) = match &name_tok.kind {
            Tok::Word(w) => (w.clone(), name_tok.span),
            _ => return Err(ParseError::ExpectedDefiningName {
                keyword: "CLASS:", at: kw_span,
            }),
        };
        self.bump();
        self.skip_comments();

        // Optional EXTENDS clause.
        let mut extends: Option<String> = None;
        if let Some(t) = self.peek() {
            if let Tok::Word(w) = &t.kind {
                if w.eq_ignore_ascii_case("extends") {
                    self.bump();  // consume `extends`
                    self.skip_comments();
                    let parent_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName {
                            keyword: "EXTENDS", at: kw_span,
                        },
                    )?;
                    match &parent_tok.kind {
                        Tok::Word(w) => { extends = Some(w.clone()); }
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: "EXTENDS", at: kw_span,
                        }),
                    }
                    self.bump();
                }
            }
        }

        // Body: SLOT: x SLOT: y ... ; (with comments interleaved).
        let mut slots: Vec<SlotDef> = Vec::new();
        let end_span;
        loop {
            self.skip_comments();
            let Some(t) = self.peek() else {
                return Err(ParseError::UnterminatedDefinition { opened_at: kw_span });
            };
            match &t.kind {
                Tok::Word(w) if w == ";" => {
                    end_span = t.span;
                    self.bump();
                    break;
                }
                Tok::Word(w) if w.eq_ignore_ascii_case("slot:") => {
                    self.bump();  // consume `slot:`
                    self.skip_comments();
                    let slot_tok = self.peek().ok_or(
                        ParseError::ExpectedDefiningName {
                            keyword: "SLOT:", at: kw_span,
                        },
                    )?;
                    let (slot_name, slot_span) = match &slot_tok.kind {
                        Tok::Word(w) => (w.clone(), slot_tok.span),
                        _ => return Err(ParseError::ExpectedDefiningName {
                            keyword: "SLOT:", at: kw_span,
                        }),
                    };
                    self.bump();
                    slots.push(SlotDef { name: slot_name, name_span: slot_span });
                }
                _ => {
                    // Stray content inside a CLASS body — flag clearly.
                    return Err(ParseError::StrayControlWord {
                        word: match &t.kind {
                            Tok::Word(w) => w.clone(),
                            _ => "<non-word>".to_string(),
                        },
                        at: t.span,
                    });
                }
            }
        }

        Ok(ClassDef {
            name, name_span, extends, slots,
            span: Span { start: kw_span.start, end: end_span.end },
        })
    }

    /// Already past `GENERIC:`.  Consumes `name ( effect )`.
    fn generic_declaration(&mut self, kw_span: Span) -> Result<GenericDef, ParseError> {
        let name_tok = self.peek().ok_or(
            ParseError::ExpectedDefiningName { keyword: "GENERIC:", at: kw_span },
        )?;
        let (name, name_span) = match &name_tok.kind {
            Tok::Word(w) => (w.clone(), name_tok.span),
            _ => return Err(ParseError::ExpectedDefiningName {
                keyword: "GENERIC:", at: kw_span,
            }),
        };
        self.bump();
        // Required stack effect annotation as a `( ... -- ... )` block
        // comment immediately after.
        let Some(Token { kind: Tok::BlockComment(body), span: eff_span }) = self.peek() else {
            return Err(ParseError::MalformedStackEffect {
                at: name_span,
                reason: "GENERIC: requires a stack effect annotation",
            });
        };
        let effect = parse_stack_effect(body, *eff_span).ok_or(
            ParseError::MalformedStackEffect {
                at: *eff_span,
                reason: "GENERIC: stack effect must contain `--`",
            },
        )?;
        let end = *eff_span;
        self.bump();
        Ok(GenericDef {
            name, name_span, effect,
            span: Span { start: kw_span.start, end: end.end },
        })
    }

    /// Already past `METHOD:` (or `METHOD-BEFORE:` / `METHOD-AFTER:`).
    /// Consumes `gname ( a:cls -- ... ) body ;`.  The `kw_static` and
    /// `kind` parameters discriminate primary vs aux flavour: the
    /// keyword text is used for diagnostics; the kind is recorded on
    /// the resulting MethodDef and drives emit-time routing to the
    /// primary or shadow `:before`/`:after` generic.
    fn method_definition(
        &mut self,
        kw_span: Span,
        kw_static: &'static str,
        kind: super::ast::MethodKind,
    ) -> Result<MethodDef, ParseError> {
        let name_tok = self.peek().ok_or(
            ParseError::ExpectedDefiningName { keyword: kw_static, at: kw_span },
        )?;
        let (gname, gname_span) = match &name_tok.kind {
            Tok::Word(w) => (w.clone(), name_tok.span),
            _ => return Err(ParseError::ExpectedDefiningName {
                keyword: kw_static, at: kw_span,
            }),
        };
        self.bump();
        // Required effect annotation, with specialisers in the input
        // list as `name:classname`.
        let Some(Token { kind: Tok::BlockComment(body), span: eff_span }) = self.peek() else {
            return Err(ParseError::MalformedStackEffect {
                at: gname_span,
                reason: "METHOD: requires a stack effect annotation",
            });
        };
        let raw_effect = parse_stack_effect(body, *eff_span).ok_or(
            ParseError::MalformedStackEffect {
                at: *eff_span,
                reason: "METHOD: stack effect must contain `--`",
            },
        )?;
        let eff_span_copy = *eff_span;
        self.bump();

        // Extract specialisers: any input of the form `name:class`
        // contributes a MethodSpecializer, and the bare `name` part
        // becomes the canonical input name.
        let mut specializers: Vec<MethodSpecializer> = Vec::new();
        let mut clean_inputs: Vec<String> = Vec::with_capacity(raw_effect.inputs.len());
        for (i, raw) in raw_effect.inputs.iter().enumerate() {
            if let Some((pname, cls)) = raw.split_once(':') {
                if !pname.is_empty() && !cls.is_empty() {
                    specializers.push(MethodSpecializer {
                        position: i as u32,
                        param_name: pname.to_string(),
                        class_name: cls.to_string(),
                        at: eff_span_copy,
                    });
                    clean_inputs.push(pname.to_string());
                    continue;
                }
            }
            clean_inputs.push(raw.clone());
        }
        let effect = StackEffect {
            inputs: clean_inputs,
            outputs: raw_effect.outputs.clone(),
            span: raw_effect.span,
        };

        // Body until `;`.
        let mut body: Vec<Expr> = Vec::new();
        let end_span;
        loop {
            self.skip_comments();
            let Some(t) = self.peek() else {
                return Err(ParseError::UnterminatedDefinition { opened_at: kw_span });
            };
            match &t.kind {
                Tok::Word(w) if w == ";" => {
                    end_span = t.span;
                    self.bump();
                    break;
                }
                _ => {
                    let e = self.expr_one()?;
                    body.push(e);
                }
            }
        }

        Ok(MethodDef {
            generic_name: gname,
            generic_name_span: gname_span,
            specializers,
            effect,
            body,
            span: Span { start: kw_span.start, end: end_span.end },
            kind,
        })
    }

    /// Parse a single expression: literal, word-ref, or a structured
    /// control-flow block.  Caller has already filtered out `:`, `;`,
    /// and (at the outer level) terminators.  When we encounter a
    /// stray terminator here, that's an error — the parent should
    /// have caught it via `parse_block_until`.
    fn expr_one(&mut self) -> Result<Expr, ParseError> {
        // Peek first to handle control-flow openers; only bump on
        // simple expressions.
        let t = self.peek().expect("expr_one called at EOF");
        let t_span = t.span;
        match &t.kind {
            Tok::Int { value, .. } => {
                let v = *value; self.bump();
                Ok(Expr::Lit(Literal::Int { value: v, span: t_span }))
            }
            Tok::Float { value, .. } => {
                let v = *value; self.bump();
                Ok(Expr::Lit(Literal::Float { value: v, span: t_span }))
            }
            Tok::Str { value, kind } => {
                let v = value.clone(); let k = *kind; self.bump();
                Ok(Expr::Lit(Literal::Str { value: v, kind: k, span: t_span }))
            }
            Tok::LetBlock(text) => {
                // The let_lang parser handles everything inside LET..END.
                // Errors propagate as ParseError::LetSyntax.
                let text = text.clone();
                self.bump();
                match super::let_lang::parse(&text) {
                    Ok(form) => Ok(Expr::LetForm { form, span: t_span }),
                    Err(e) => Err(ParseError::LetSyntax {
                        at: t_span,
                        reason: e.message,
                    }),
                }
            }
            Tok::Word(w) => {
                let lc = w.to_ascii_lowercase();
                match lc.as_str() {
                    "if"    => { self.bump(); self.parse_if(t_span) }
                    "begin" => { self.bump(); self.parse_begin(t_span) }
                    "do"    => { self.bump(); self.parse_do(t_span, false) }
                    "?do"   => { self.bump(); self.parse_do(t_span, true)  }
                    "case"  => { self.bump(); self.parse_case(t_span) }
                    // ' (tick) — ANS parsing word: consume next token
                    // and emit Expr::Tick { name } at runtime pushing
                    // the target's execution token.  M2.x #33.
                    "'" => {
                        self.bump(); // consume the `'`
                        let name_tok = self.peek().ok_or(
                            ParseError::ExpectedDefiningName {
                                keyword: "'", at: t_span,
                            },
                        )?;
                        let (name, end) = match &name_tok.kind {
                            Tok::Word(w) => (w.clone(), name_tok.span.end),
                            _ => return Err(ParseError::ExpectedDefiningName {
                                keyword: "'", at: t_span,
                            }),
                        };
                        self.bump(); // consume the target name
                        Ok(Expr::Tick {
                            name,
                            span: Span { start: t_span.start, end },
                        })
                    }
                    // `TO name` — store-to-VALUE parsing word.  Like
                    // `'`, consumes the next blank-delimited token as
                    // its target.  Resolve checks the name binds to a
                    // VALUE; emit lowers to `<storage> set-global` on
                    // the underlying Factor global.
                    "to" => {
                        self.bump(); // consume `to`
                        let name_tok = self.peek().ok_or(
                            ParseError::ExpectedDefiningName {
                                keyword: "TO", at: t_span,
                            },
                        )?;
                        let (name, end) = match &name_tok.kind {
                            Tok::Word(w) => (w.clone(), name_tok.span.end),
                            _ => return Err(ParseError::ExpectedDefiningName {
                                keyword: "TO", at: t_span,
                            }),
                        };
                        self.bump(); // consume target name
                        Ok(Expr::To {
                            name,
                            span: Span { start: t_span.start, end },
                        })
                    }
                    // `SEE name` — introspection parsing word.  Like
                    // `'` and `TO`, consumes the next blank-delimited
                    // token as its target.  Emit builds a compile-time
                    // report (kind / effect / origin / source) for the
                    // named word and lowers it to a literal print.
                    "see" => {
                        self.bump(); // consume `see`
                        let name_tok = self.peek().ok_or(
                            ParseError::ExpectedDefiningName {
                                keyword: "SEE", at: t_span,
                            },
                        )?;
                        let (name, end) = match &name_tok.kind {
                            Tok::Word(w) => (w.clone(), name_tok.span.end),
                            _ => return Err(ParseError::ExpectedDefiningName {
                                keyword: "SEE", at: t_span,
                            }),
                        };
                        self.bump(); // consume target name
                        Ok(Expr::See {
                            name,
                            span: Span { start: t_span.start, end },
                        })
                    }
                    // Terminators leaking through to here means they
                    // weren't inside a matching opener.
                    "else" | "then" | "until" | "while" | "repeat"
                    | "again" | "loop" | "+loop"
                    | "of" | "endof" | "default" | "other" | "endcase" => {
                        Err(ParseError::StrayControlWord {
                            word: lc, at: t_span,
                        })
                    }
                    _ => {
                        let name = w.clone(); self.bump();
                        Ok(Expr::WordRef { name, span: t_span })
                    }
                }
            }
            Tok::LineComment(_) | Tok::BlockComment(_) => {
                self.bump();
                self.expr_one()
            }
        }
    }

    /// Parse the body of `IF ... [ELSE ...] THEN`.  Caller has already
    /// consumed the `if` token at `if_span`.
    fn parse_if(&mut self, if_span: Span) -> Result<Expr, ParseError> {
        let (then_body, term) = self.parse_block_until(
            "if", if_span, &["else", "then"],
        )?;
        let term_word = match &term.kind {
            Tok::Word(w) => w.to_ascii_lowercase(),
            _ => unreachable!("parse_block_until only returns word terminators"),
        };
        let (else_body, end_span) = if term_word == "else" {
            let (eb, then_tok) = self.parse_block_until("else", term.span, &["then"])?;
            (Some(eb), then_tok.span)
        } else {
            (None, term.span)
        };
        Ok(Expr::If {
            then_body, else_body,
            span: Span { start: if_span.start, end: end_span.end },
        })
    }

    /// Parse `BEGIN ... (UNTIL | AGAIN | WHILE ... REPEAT)`.  Caller
    /// has consumed the `begin` token at `begin_span`.
    fn parse_begin(&mut self, begin_span: Span) -> Result<Expr, ParseError> {
        let (body, term) = self.parse_block_until(
            "begin", begin_span, &["until", "again", "while"],
        )?;
        let term_word = match &term.kind {
            Tok::Word(w) => w.to_ascii_lowercase(),
            _ => unreachable!(),
        };
        let span = Span { start: begin_span.start, end: term.span.end };
        match term_word.as_str() {
            "until" => Ok(Expr::BeginUntil { body, span }),
            "again" => Ok(Expr::BeginAgain { body, span }),
            "while" => {
                // What we parsed before WHILE is the predicate.
                // Now parse the body up to REPEAT.
                let (loop_body, repeat_tok) =
                    self.parse_block_until("while", term.span, &["repeat"])?;
                Ok(Expr::BeginWhileRepeat {
                    pred: body, body: loop_body,
                    span: Span { start: begin_span.start, end: repeat_tok.span.end },
                })
            }
            _ => unreachable!(),
        }
    }

    /// Parse `DO ... LOOP` or `DO ... +LOOP` (also `?DO` variant).
    /// Caller has consumed the `do`/`?do` token at `do_span`.
    fn parse_do(&mut self, do_span: Span, is_qdo: bool) -> Result<Expr, ParseError> {
        let opener = if is_qdo { "?do" } else { "do" };
        let (body, term) = self.parse_block_until(
            opener, do_span, &["loop", "+loop"],
        )?;
        let term_word = match &term.kind {
            Tok::Word(w) => w.to_ascii_lowercase(),
            _ => unreachable!(),
        };
        let loop_kind = match term_word.as_str() {
            "loop"  => LoopKind::Plus1,
            "+loop" => LoopKind::PlusN,
            _ => unreachable!(),
        };
        Ok(Expr::DoLoop {
            is_qdo, body, loop_kind,
            span: Span { start: do_span.start, end: term.span.end },
        })
    }

    /// Parse `CASE ... ENDCASE`.  Caller has consumed the `case`
    /// token at `case_span`.
    ///
    /// Each round either:
    ///   - sees `OF`: the just-parsed expressions are the match expr
    ///     for a new arm; then parse the arm body up to `ENDOF`.
    ///   - sees `DEFAULT` / `OTHER`: parse an explicit default body
    ///     up to `ENDCASE`.
    ///   - sees `ENDCASE`: the just-parsed expressions (possibly
    ///     empty) are the implicit default branch; we're done.
    ///
    /// Nested CASEs inside an arm body are absorbed by `expr_one`'s
    /// recursive call back into `parse_case`.
    fn parse_case(&mut self, case_span: Span) -> Result<Expr, ParseError> {
        let mut arms: Vec<CaseArm> = Vec::new();
        let mut default: Option<Vec<Expr>> = None;
        let end_span;
        loop {
            let (exprs, term) = self.parse_block_until(
                "case", case_span, &["of", "default", "other", "endcase"],
            )?;
            let term_word = match &term.kind {
                Tok::Word(w) => w.to_ascii_lowercase(),
                _ => unreachable!(),
            };
            if term_word == "of" {
                // exprs is the match expr for the upcoming arm.
                let arm_start = exprs.first().map(|e| e.span())
                    .unwrap_or(term.span);
                let (body, endof) = self.parse_block_until(
                    "of", term.span, &["endof"],
                )?;
                arms.push(CaseArm {
                    match_expr: exprs,
                    body,
                    span: Span { start: arm_start.start, end: endof.span.end },
                });
            } else if term_word == "default" || term_word == "other" {
                let (body, endcase) = self.parse_block_until(
                    &term_word, term.span, &["endcase"],
                )?;
                default = Some(body);
                end_span = endcase.span;
                break;
            } else {
                // endcase: exprs is the default (may be empty).
                if !exprs.is_empty() {
                    default = Some(exprs);
                }
                end_span = term.span;
                break;
            }
        }
        Ok(Expr::Case {
            arms, default,
            span: Span { start: case_span.start, end: end_span.end },
        })
    }

    /// Parse a sequence of expressions until we encounter one of
    /// `terminators` at this nesting level (recursive openers like
    /// nested IFs / BEGINs are absorbed by `expr_one`).  Returns
    /// the parsed body and the *consumed* terminator token.
    fn parse_block_until(
        &mut self,
        opener: &str,
        opened_at: Span,
        terminators: &'static [&'static str],
    ) -> Result<(Vec<Expr>, &'t Token), ParseError> {
        let mut body: Vec<Expr> = Vec::new();
        loop {
            self.skip_comments();
            let t = match self.peek() {
                Some(t) => t,
                None => return Err(ParseError::UnterminatedControl {
                    opener: opener.to_string(),
                    opened_at,
                    expected: terminators,
                }),
            };
            if let Tok::Word(w) = &t.kind {
                let lc = w.to_ascii_lowercase();
                if terminators.iter().any(|s| *s == lc) {
                    let term = self.bump().unwrap();
                    return Ok((body, term));
                }
                // `:` and `;` are hard stops — they can't appear
                // inside a control-flow block.  Re-route to the
                // appropriate error.
                if w == ";" {
                    return Err(ParseError::UnterminatedControl {
                        opener: opener.to_string(),
                        opened_at,
                        expected: terminators,
                    });
                }
            }
            body.push(self.expr_one()?);
        }
    }
}

/// Try to parse a block-comment body as a stack effect.  The body
/// must contain `--` surrounded by whitespace; otherwise we treat
/// the block as a plain comment and return None.
///
/// Accepts irregular whitespace: `( a b --c )`, `(a b -- c)`, etc.
pub(crate) fn parse_stack_effect(body: &str, span: Span) -> Option<StackEffect> {
    // Find a free-standing `--`.  We require word-boundary whitespace
    // (or start/end-of-string) on both sides to avoid matching things
    // like `2--3`.  Simple linear scan:
    let bytes = body.as_bytes();
    let mut split: Option<usize> = None;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1] == b'-' {
            let left_ok  = i == 0 || bytes[i - 1].is_ascii_whitespace();
            let right_ok = i + 2 == bytes.len() || bytes[i + 2].is_ascii_whitespace();
            if left_ok && right_ok { split = Some(i); break; }
        }
        i += 1;
    }
    let split = split?;
    let inputs: Vec<String> = body[..split].split_whitespace().map(str::to_string).collect();
    let outputs: Vec<String> = body[split + 2 ..].split_whitespace().map(str::to_string).collect();
    Some(StackEffect { inputs, outputs, span })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::lex::lex;

    fn parse_str(src: &str) -> Result<Program, ParseError> {
        let toks = lex(src).unwrap();
        parse(&toks)
    }

    #[test]
    fn empty_input() {
        let prog = parse_str("").unwrap();
        assert!(prog.items.is_empty());
    }

    #[test]
    fn only_comments() {
        let prog = parse_str("\\ a comment\n( another )").unwrap();
        assert!(prog.items.is_empty());
    }

    #[test]
    fn top_level_literal() {
        let prog = parse_str("42").unwrap();
        let Item::TopLevel { exprs, .. } = &prog.items[0] else { panic!() };
        assert!(matches!(&exprs[0], Expr::Lit(Literal::Int { value: 42, .. })));
    }

    #[test]
    fn simple_definition() {
        let prog = parse_str(": square ( n -- n^2 ) dup * ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        assert_eq!(d.name, "square");
        let eff = d.effect.as_ref().expect("effect parsed");
        assert_eq!(eff.inputs, vec!["n".to_string()]);
        assert_eq!(eff.outputs, vec!["n^2".to_string()]);
        assert_eq!(d.body.len(), 2);
        assert!(matches!(&d.body[0], Expr::WordRef { name, .. } if name == "dup"));
        assert!(matches!(&d.body[1], Expr::WordRef { name, .. } if name == "*"));
    }

    #[test]
    fn definition_without_effect() {
        let prog = parse_str(": foo 1 2 + ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        assert_eq!(d.name, "foo");
        assert!(d.effect.is_none());
        assert_eq!(d.body.len(), 3);
    }

    #[test]
    fn definition_with_non_effect_comment() {
        // `( a stack-shaped comment )` lacks `--` so it's a plain
        // comment, not a stack effect.
        let prog = parse_str(": foo ( just notes ) dup ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        assert!(d.effect.is_none(), "non-effect comment shouldn't attach as effect");
        assert_eq!(d.body.len(), 1);
    }

    #[test]
    fn multiple_definitions_and_toplevel() {
        let prog = parse_str(": one 1 ; : two 2 ; one two +").unwrap();
        assert_eq!(prog.items.len(), 3);
        assert!(matches!(prog.items[0], Item::Definition(_)));
        assert!(matches!(prog.items[1], Item::Definition(_)));
        let Item::TopLevel { exprs, .. } = &prog.items[2] else { panic!() };
        assert_eq!(exprs.len(), 3);
    }

    #[test]
    fn nested_colon_rejected() {
        let err = parse_str(": outer : inner ; ;").unwrap_err();
        assert!(matches!(err, ParseError::NestedColon { .. }));
    }

    #[test]
    fn stray_semicolon_rejected() {
        let err = parse_str(": foo ; ;").unwrap_err();
        assert!(matches!(err, ParseError::StraySemicolon { .. }));
    }

    #[test]
    fn unterminated_definition_rejected() {
        let err = parse_str(": foo dup *").unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedDefinition { .. }));
    }

    #[test]
    fn empty_stack_effect() {
        let prog = parse_str(": foo ( -- ) ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let eff = d.effect.as_ref().unwrap();
        assert!(eff.inputs.is_empty());
        assert!(eff.outputs.is_empty());
    }

    #[test]
    fn float_literal_in_body() {
        let prog = parse_str(": pi 3.14 ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        assert!(matches!(&d.body[0], Expr::Lit(Literal::Float { value, .. })
                                       if (*value - 3.14).abs() < 1e-9));
    }

    #[test]
    fn string_literal_in_body() {
        let prog = parse_str(": greet .\" hi\" ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        assert!(matches!(&d.body[0],
            Expr::Lit(Literal::Str { value, kind: StringKind::DotQuote, .. })
              if value == "hi"));
    }

    // ── Control-flow parsing (M2.4) ────────────────────────────────

    #[test]
    fn if_then_no_else() {
        let prog = parse_str(": abs dup 0 < if negate then ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        // dup 0 < then If
        assert_eq!(d.body.len(), 4);
        let Expr::If { then_body, else_body, .. } = &d.body[3] else {
            panic!("expected If node at end, got {:?}", d.body[3]);
        };
        assert!(else_body.is_none());
        assert_eq!(then_body.len(), 1);
        assert!(matches!(&then_body[0], Expr::WordRef { name, .. } if name == "negate"));
    }

    #[test]
    fn if_else_then() {
        let prog = parse_str(": sign dup 0 < if drop -1 else drop 1 then ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::If { then_body, else_body, .. } = d.body.last().unwrap() else {
            panic!("expected If node");
        };
        let eb = else_body.as_ref().expect("else branch");
        assert_eq!(then_body.len(), 2);   // drop -1
        assert_eq!(eb.len(), 2);          // drop  1
    }

    #[test]
    fn nested_if() {
        let prog = parse_str(": sign dup 0 < if -1 else dup 0 > if 1 else 0 then then ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        // outer IF should have inner IF in else-branch
        let Expr::If { else_body: Some(eb), .. } = d.body.last().unwrap() else {
            panic!("outer if missing else");
        };
        // eb should contain `dup`, `0`, `>`, inner-IF
        assert!(eb.iter().any(|e| matches!(e, Expr::If { .. })),
                "expected nested If in else branch");
    }

    #[test]
    fn begin_until() {
        let prog = parse_str(": countdown begin 1 - dup 0 = until ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::BeginUntil { body, .. } = d.body.last().unwrap() else {
            panic!("expected BeginUntil");
        };
        // 1 - dup 0 =  → five exprs (int, -, dup, int, =)
        assert_eq!(body.len(), 5);
    }

    #[test]
    fn begin_while_repeat() {
        let prog = parse_str(": foo begin dup 0 > while 1 - repeat ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::BeginWhileRepeat { pred, body, .. } = d.body.last().unwrap() else {
            panic!("expected BeginWhileRepeat");
        };
        assert_eq!(pred.len(), 3);   // dup 0 >
        assert_eq!(body.len(), 2);   // 1 -
    }

    #[test]
    fn begin_again() {
        let prog = parse_str(": forever begin 1 again ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::BeginAgain { body, .. } = d.body.last().unwrap() else {
            panic!("expected BeginAgain");
        };
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn stray_else_rejected() {
        let err = parse_str(": foo else ;").unwrap_err();
        assert!(matches!(err, ParseError::StrayControlWord { ref word, .. } if word == "else"));
    }

    #[test]
    fn unterminated_if_rejected() {
        let err = parse_str(": foo if 1 ;").unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedControl { ref opener, .. } if opener == "if"));
    }

    #[test]
    fn unterminated_begin_rejected() {
        let err = parse_str(": foo begin 1 ;").unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedControl { ref opener, .. } if opener == "begin"));
    }

    // ── DO/LOOP parsing (M2.5) ─────────────────────────────────────

    #[test]
    fn do_loop_basic() {
        let prog = parse_str(": sum 0 swap 0 do i + loop ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::DoLoop { is_qdo, body, loop_kind, .. } = d.body.last().unwrap() else {
            panic!("expected DoLoop");
        };
        assert!(!*is_qdo);
        assert_eq!(*loop_kind, LoopKind::Plus1);
        assert_eq!(body.len(), 2);          // i +
    }

    #[test]
    fn qdo_loop() {
        let prog = parse_str(": sum 0 swap 0 ?do i + loop ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::DoLoop { is_qdo, loop_kind, .. } = d.body.last().unwrap() else { panic!() };
        assert!(*is_qdo);
        assert_eq!(*loop_kind, LoopKind::Plus1);
    }

    #[test]
    fn plus_loop_terminator() {
        let prog = parse_str(": odd 10 0 ?do i . 2 +loop ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::DoLoop { loop_kind, body, .. } = d.body.last().unwrap() else { panic!() };
        assert_eq!(*loop_kind, LoopKind::PlusN);
        // body: i . 2  — the 2 is the step expression
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn nested_do_loops() {
        let prog = parse_str(": matrix 3 0 do 3 0 do i j * . loop loop ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::DoLoop { body: outer, .. } = d.body.last().unwrap() else { panic!() };
        // Outer body: 3, 0, DoLoop(inner)
        assert!(outer.iter().any(|e| matches!(e, Expr::DoLoop { .. })),
                "expected inner DoLoop inside outer");
    }

    #[test]
    fn stray_loop_rejected() {
        let err = parse_str(": foo loop ;").unwrap_err();
        assert!(matches!(err, ParseError::StrayControlWord { ref word, .. } if word == "loop"));
    }

    #[test]
    fn unterminated_do_rejected() {
        let err = parse_str(": foo 10 0 do i + ;").unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedControl { ref opener, .. } if opener == "do"));
    }

    // ── CASE/OF/ENDOF/ENDCASE parsing (M2.6) ───────────────────────

    #[test]
    fn case_two_arms_no_default() {
        let prog = parse_str(
            ": classify case 1 of .\" one\" endof 2 of .\" two\" endof endcase ;"
        ).unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, default, .. } = d.body.last().unwrap() else {
            panic!("expected Case");
        };
        assert_eq!(arms.len(), 2);
        assert!(default.is_none());
        // arm 0: match_expr [1], body [."one"]
        let a0 = &arms[0];
        assert_eq!(a0.match_expr.len(), 1);
        assert!(matches!(&a0.match_expr[0], Expr::Lit(Literal::Int { value: 1, .. })));
        assert_eq!(a0.body.len(), 1);
    }

    #[test]
    fn case_with_default() {
        let prog = parse_str(
            ": c case 1 of .\" one\" endof .\" other\" endcase ;"
        ).unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, default, .. } = d.body.last().unwrap() else { panic!() };
        assert_eq!(arms.len(), 1);
        let def = default.as_ref().expect("default present");
        assert_eq!(def.len(), 1);
    }

    #[test]
    fn case_with_explicit_default_keyword() {
        let prog = parse_str(
            ": c case 1 of .\" one\" endof default .\" other\" endcase ;"
        ).unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, default, .. } = d.body.last().unwrap() else { panic!() };
        assert_eq!(arms.len(), 1);
        let def = default.as_ref().expect("default present");
        assert_eq!(def.len(), 1);
    }

    #[test]
    fn case_with_explicit_other_keyword() {
        let prog = parse_str(
            ": c case 1 of .\" one\" endof other .\" other\" endcase ;"
        ).unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, default, .. } = d.body.last().unwrap() else { panic!() };
        assert_eq!(arms.len(), 1);
        let def = default.as_ref().expect("default present");
        assert_eq!(def.len(), 1);
    }

    #[test]
    fn case_complex_match_expr() {
        // Match value computed at runtime: `2 *` means "match-against
        // 2 times the previously-pushed value".  ANS allows this.
        let prog = parse_str(": c case 2 * of 1 endof endcase ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, .. } = d.body.last().unwrap() else { panic!() };
        // The match_expr has TWO expressions: 2, *
        assert_eq!(arms[0].match_expr.len(), 2);
    }

    #[test]
    fn nested_case() {
        let prog = parse_str(
            ": c case 1 of case 11 of .\" 11\" endof endcase endof endcase ;"
        ).unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, .. } = d.body.last().unwrap() else { panic!() };
        // outer arm body should contain a nested Case
        assert!(arms[0].body.iter().any(|e| matches!(e, Expr::Case { .. })),
                "expected nested Case in outer arm");
    }

    #[test]
    fn empty_case() {
        // Vacuous CASE — just drops the dispatch value.  ANS permits it.
        let prog = parse_str(": c case endcase ;").unwrap();
        let Item::Definition(d) = &prog.items[0] else { panic!() };
        let Expr::Case { arms, default, .. } = d.body.last().unwrap() else { panic!() };
        assert!(arms.is_empty());
        assert!(default.is_none());
    }

    #[test]
    fn stray_endof_rejected() {
        let err = parse_str(": foo endof ;").unwrap_err();
        assert!(matches!(err, ParseError::StrayControlWord { ref word, .. } if word == "endof"));
    }

    #[test]
    fn stray_default_rejected() {
        let err = parse_str(": foo default ;").unwrap_err();
        assert!(matches!(err, ParseError::StrayControlWord { ref word, .. } if word == "default"));
    }

    #[test]
    fn unterminated_case_rejected() {
        let err = parse_str(": foo case 1 of .\" hi\" endof ;").unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedControl { ref opener, .. } if opener == "case"));
    }
}
