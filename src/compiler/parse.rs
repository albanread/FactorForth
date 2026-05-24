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
use super::lex::{Tok, Token, StringKind};

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
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::ExpectedDefName { at } =>
                write!(f, "expected definition name after `:` at {at}"),
            ParseError::NestedColon { inner, .. } =>
                write!(f, "nested `:` at {inner}: ANS forbids defining a word inside another"),
            ParseError::StraySemicolon { at } =>
                write!(f, "stray `;` at {at}: no `:` to close"),
            ParseError::UnterminatedDefinition { opened_at } =>
                write!(f, "unterminated `:` definition opened at {opened_at}"),
            ParseError::MalformedStackEffect { at, reason } =>
                write!(f, "malformed stack effect at {at}: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

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
        loop {
            self.skip_comments();
            let Some(t) = self.peek() else { break; };
            match &t.kind {
                Tok::Word(w) if w == ":" => {
                    let def = self.colon_definition()?;
                    items.push(Item::Definition(def));
                }
                Tok::Word(w) if w == ";" => {
                    return Err(ParseError::StraySemicolon { at: t.span });
                }
                _ => {
                    // Collect a run of top-level expressions until we
                    // hit `:` or EOF.
                    let start_span = t.span;
                    let mut exprs: Vec<Expr> = Vec::new();
                    let mut end_span = start_span;
                    loop {
                        self.skip_comments();
                        let Some(t) = self.peek() else { break; };
                        match &t.kind {
                            Tok::Word(w) if w == ":" => break,
                            Tok::Word(w) if w == ";" =>
                                return Err(ParseError::StraySemicolon { at: t.span }),
                            _ => {
                                let e = self.expr_one()?;
                                end_span = e.span();
                                exprs.push(e);
                            }
                        }
                    }
                    if !exprs.is_empty() {
                        items.push(Item::TopLevel {
                            exprs,
                            span: Span { start: start_span.start, end: end_span.end },
                        });
                    }
                }
            }
        }
        Ok(Program { items })
    }

    /// Already at the `:` token.  Consumes through the matching `;`.
    fn colon_definition(&mut self) -> Result<Definition, ParseError> {
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

        Ok(Definition {
            name, name_span, effect, body,
            span: Span { start: colon_span.start, end: end_span.end },
        })
    }

    /// Parse a single expression: literal or word-ref.  Caller has
    /// already filtered out `:`, `;`, and comments.
    fn expr_one(&mut self) -> Result<Expr, ParseError> {
        let t = self.bump().expect("expr_one called at EOF");
        match &t.kind {
            Tok::Int { value, .. } => Ok(Expr::Lit(Literal::Int { value: *value, span: t.span })),
            Tok::Float { value, .. } => Ok(Expr::Lit(Literal::Float { value: *value, span: t.span })),
            Tok::Str { value, kind } => Ok(Expr::Lit(Literal::Str {
                value: value.clone(), kind: *kind, span: t.span,
            })),
            Tok::Word(w) => Ok(Expr::WordRef { name: w.clone(), span: t.span }),
            // Comments and unknown tokens shouldn't reach here.
            Tok::LineComment(_) | Tok::BlockComment(_) => {
                // Defensive: skip and retry.
                self.expr_one()
            }
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
}
