//! Parser for the LET DSL.
//!
//! Ported from WF64's `src/let_lang/parser.rs`.  The grammar is
//! identical because the LET surface is identical — only the
//! backend (Factor IR vs LLVM-MC) differs.  This file is meant
//! to be kept in lockstep with the WF64 version where the
//! grammar evolves; the Factor-specific work lives in
//! `codegen.rs`.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(f64),
    Var(String),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Pow,
    Eq, Ne, Lt, Gt, Le, Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetForm {
    pub inputs:  Vec<LetInput>,
    pub outputs: Vec<String>,
    pub results: Vec<Expr>,
    pub wheres:  Vec<(String, Expr)>,
}

/// One position in a LET input list.  Either a plain name bound from
/// the stack, or a name with a class annotation and a list of slot
/// names to destructure into additional locals.  The destructure form
/// is what makes LET-methods readable:
///
/// ```forth
/// LET ( a:point as ax ay   b:point as bx by ) -> ( d ) =
///     sqrt((bx - ax)^2 + (by - ay)^2)
/// END
/// ```
///
/// Each `name:class as slot1 slot2 ...` adds the top-level binding
/// AND introduces one local per slot, computed at LET entry by calling
/// the class's auto-generated getter (`class>slot`).  The body then
/// uses plain `ax`, `ay`, etc., as if they were regular LET locals —
/// no new syntactic convention inside the expression grammar.
#[derive(Debug, Clone, PartialEq)]
pub struct LetInput {
    /// The top-level local name bound from the stack.
    pub name: String,
    /// If `Some((class, slots))`, the binding is also destructured:
    /// after binding `name`, each `slot` becomes a local whose value
    /// is `name class>slot`.
    pub destructure: Option<DestructureClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DestructureClause {
    pub class: String,
    pub slots: Vec<String>,
}

impl LetInput {
    pub fn plain(name: String) -> Self {
        LetInput { name, destructure: None }
    }
}

#[derive(Debug, Clone)]
pub struct LetError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for LetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "LET error at byte {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for LetError {}

// ── Lexer ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen, RParen, Comma, Equals, Arrow,
    Plus, Minus, Star, Slash, StarStar,
    EqEq, NotEq, Less, Greater, LessEq, GreaterEq,
    Colon,
    LetKw, EndKw, WhereKw, AsKw,
    Ident(String),
    Num(f64),
    Eof,
}

struct Lexer<'s> {
    src: &'s [u8],
    pos: usize,
}

impl<'s> Lexer<'s> {
    fn new(src: &'s str) -> Self { Self { src: src.as_bytes(), pos: 0 } }

    fn skip_ws(&mut self) {
        // Whitespace + `\` line comments only.  We DO NOT recognise
        // Forth-style `( ... )` block comments inside LET — the
        // parens are too important as the input/output list
        // delimiters, and `( a b )` is the natural way to write a
        // two-element list with breathing room.  Recognising it as
        // a comment would silently eat the list and produce a
        // baffling "expected LParen got Arrow" error.  Use `\`
        // line comments if you need to annotate inside a LET block.
        loop {
            while self.pos < self.src.len() {
                let c = self.src[self.pos];
                if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos < self.src.len() && self.src[self.pos] == b'\\' {
                let next = self.src.get(self.pos + 1).copied().unwrap_or(b' ');
                if next == b' ' || next == b'\t' || next == b'\n' || next == b'\r' {
                    while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                    continue;
                }
            }
            break;
        }
    }

    fn next_tok(&mut self) -> Result<(Tok, usize), LetError> {
        self.skip_ws();
        let start = self.pos;
        if self.pos >= self.src.len() {
            return Ok((Tok::Eof, start));
        }
        let c = self.src[self.pos];
        match c {
            b'(' => { self.pos += 1; Ok((Tok::LParen, start)) }
            b')' => { self.pos += 1; Ok((Tok::RParen, start)) }
            b',' => { self.pos += 1; Ok((Tok::Comma, start)) }
            b'=' => {
                if self.src.get(self.pos + 1) == Some(&b'=') {
                    self.pos += 2;
                    Ok((Tok::EqEq, start))
                } else {
                    self.pos += 1;
                    Ok((Tok::Equals, start))
                }
            }
            b'!' => {
                if self.src.get(self.pos + 1) == Some(&b'=') {
                    self.pos += 2;
                    Ok((Tok::NotEq, start))
                } else {
                    Err(LetError {
                        message: "stray '!' (did you mean '!='?)".into(),
                        pos: start,
                    })
                }
            }
            b'<' => {
                if self.src.get(self.pos + 1) == Some(&b'=') {
                    self.pos += 2;
                    Ok((Tok::LessEq, start))
                } else {
                    self.pos += 1;
                    Ok((Tok::Less, start))
                }
            }
            b'>' => {
                if self.src.get(self.pos + 1) == Some(&b'=') {
                    self.pos += 2;
                    Ok((Tok::GreaterEq, start))
                } else {
                    self.pos += 1;
                    Ok((Tok::Greater, start))
                }
            }
            b'+' => { self.pos += 1; Ok((Tok::Plus, start)) }
            b'-' => {
                if self.src.get(self.pos + 1) == Some(&b'>') {
                    self.pos += 2;
                    Ok((Tok::Arrow, start))
                } else {
                    self.pos += 1;
                    Ok((Tok::Minus, start))
                }
            }
            b'*' => {
                if self.src.get(self.pos + 1) == Some(&b'*') {
                    self.pos += 2;
                    Ok((Tok::StarStar, start))
                } else {
                    self.pos += 1;
                    Ok((Tok::Star, start))
                }
            }
            b'/' => { self.pos += 1; Ok((Tok::Slash, start)) }
            b':' => { self.pos += 1; Ok((Tok::Colon, start)) }
            // `^` is an alias for `**` — math users reach for it
            // first.  Both produce the same Pow op at parse time.
            b'^' => { self.pos += 1; Ok((Tok::StarStar, start)) }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let mut end = self.pos + 1;
                while end < self.src.len() {
                    let ch = self.src[end];
                    if ch.is_ascii_alphanumeric() || ch == b'_' { end += 1; }
                    else { break; }
                }
                let word = std::str::from_utf8(&self.src[self.pos..end])
                    .map_err(|_| LetError {
                        message: "non-UTF8 identifier".into(),
                        pos: start,
                    })?
                    .to_string();
                self.pos = end;
                let tok = if word.eq_ignore_ascii_case("let")    { Tok::LetKw }
                    else if word.eq_ignore_ascii_case("end")     { Tok::EndKw }
                    else if word.eq_ignore_ascii_case("where")   { Tok::WhereKw }
                    else if word.eq_ignore_ascii_case("as")      { Tok::AsKw }
                    else { Tok::Ident(word) };
                Ok((tok, start))
            }
            c if c.is_ascii_digit() || c == b'.' => {
                let mut end = self.pos;
                let mut has_dot = false;
                while end < self.src.len() {
                    let ch = self.src[end];
                    if ch.is_ascii_digit() { end += 1; }
                    else if ch == b'.' && !has_dot { has_dot = true; end += 1; }
                    else if ch == b'e' || ch == b'E' {
                        end += 1;
                        if end < self.src.len()
                            && (self.src[end] == b'+' || self.src[end] == b'-')
                        {
                            end += 1;
                        }
                    } else { break; }
                }
                if end == self.pos + 1 && self.src[self.pos] == b'.' {
                    return Err(LetError {
                        message: "lone '.' isn't a number".into(),
                        pos: start,
                    });
                }
                let s = std::str::from_utf8(&self.src[self.pos..end]).unwrap();
                let n: f64 = s.parse().map_err(|_| LetError {
                    message: format!("invalid number '{s}'"),
                    pos: start,
                })?;
                self.pos = end;
                Ok((Tok::Num(n), start))
            }
            _ => Err(LetError {
                message: format!("unexpected character '{}'", c as char),
                pos: start,
            }),
        }
    }
}

// ── Parser ───────────────────────────────────────────────────────────

struct Parser<'s> {
    lex: Lexer<'s>,
    cur: (Tok, usize),
}

impl<'s> Parser<'s> {
    fn new(src: &'s str) -> Result<Self, LetError> {
        let mut lex = Lexer::new(src);
        let cur = lex.next_tok()?;
        Ok(Self { lex, cur })
    }

    fn bump(&mut self) -> Result<(Tok, usize), LetError> {
        let prev = std::mem::replace(&mut self.cur, self.lex.next_tok()?);
        Ok(prev)
    }

    fn expect(&mut self, t: &Tok) -> Result<(), LetError> {
        if std::mem::discriminant(&self.cur.0) == std::mem::discriminant(t) {
            self.bump()?;
            Ok(())
        } else {
            Err(LetError {
                message: format!("expected {t:?}, got {:?}", self.cur.0),
                pos: self.cur.1,
            })
        }
    }

    /// Output list — plain identifiers, no destructuring.  Separators
    /// can be commas, whitespace, or both, matching the input list:
    /// `( c )`, `( sx sy )`, and `( sx, sy )` all parse.  (Yesterday's
    /// fix made the INPUT list separator-flexible but left this one
    /// comma-only — so `-> ( sx sy )` errored where `( a b )` inputs
    /// were fine.  Now both ends agree.)
    fn output_list(&mut self) -> Result<Vec<String>, LetError> {
        self.expect(&Tok::LParen)?;
        let mut out = Vec::new();
        loop {
            // Skip optional commas (treat them like whitespace).
            while self.cur.0 == Tok::Comma { self.bump()?; }
            if self.cur.0 == Tok::RParen { break; }
            if let Tok::Ident(name) = &self.cur.0 {
                let n = name.clone();
                self.bump()?;
                out.push(n);
            } else {
                return Err(LetError {
                    message: format!("expected identifier or ')', got {:?}", self.cur.0),
                    pos: self.cur.1,
                });
            }
        }
        self.expect(&Tok::RParen)?;
        Ok(out)
    }

    /// Input list — each entry is either a plain `name` or a
    /// destructuring form `name:class as slot1 slot2 ...`.
    /// Separators between entries can be commas, whitespace, or
    /// both — `(a b c)`, `(a, b, c)`, and `(a:point as x y  b)` all
    /// parse.  This is more Forth-natural than requiring commas
    /// everywhere; only the destructure clause is whitespace-
    /// sensitive (slot names after `as` end at the next non-Ident).
    fn input_list(&mut self) -> Result<Vec<LetInput>, LetError> {
        self.expect(&Tok::LParen)?;
        let mut out = Vec::new();
        loop {
            // Skip optional commas (treat them like whitespace).
            while self.cur.0 == Tok::Comma { self.bump()?; }
            if self.cur.0 == Tok::RParen { break; }
            let name = if let Tok::Ident(n) = &self.cur.0 {
                let n = n.clone();
                self.bump()?;
                n
            } else {
                return Err(LetError {
                    message: format!("expected identifier, got {:?}", self.cur.0),
                    pos: self.cur.1,
                });
            };
            // Optional destructure clause: `:class as slot1 slot2 ...`
            let destructure = if self.cur.0 == Tok::Colon {
                self.bump()?;
                let class = if let Tok::Ident(c) = &self.cur.0 {
                    let c = c.clone();
                    self.bump()?;
                    c
                } else {
                    return Err(LetError {
                        message: format!("expected class name after `:`, got {:?}", self.cur.0),
                        pos: self.cur.1,
                    });
                };
                let mut slots = Vec::new();
                if self.cur.0 == Tok::AsKw {
                    self.bump()?;
                    // Read slot names until we hit a non-Ident
                    // (comma, rparen, etc.).
                    while let Tok::Ident(s) = &self.cur.0 {
                        slots.push(s.clone());
                        self.bump()?;
                    }
                    if slots.is_empty() {
                        return Err(LetError {
                            message: "`as` requires at least one slot name".into(),
                            pos: self.cur.1,
                        });
                    }
                }
                Some(DestructureClause { class, slots })
            } else {
                None
            };
            out.push(LetInput { name, destructure });
        }
        self.expect(&Tok::RParen)?;
        Ok(out)
    }

    fn parse_form(&mut self) -> Result<LetForm, LetError> {
        self.expect(&Tok::LetKw)?;
        let inputs = self.input_list()?;
        self.expect(&Tok::Arrow)?;
        let outputs = self.output_list()?;
        self.expect(&Tok::Equals)?;
        let mut results = Vec::new();
        loop {
            results.push(self.parse_expr()?);
            if self.cur.0 == Tok::Comma { self.bump()?; }
            else { break; }
        }
        let mut wheres = Vec::new();
        while self.cur.0 == Tok::WhereKw {
            self.bump()?;
            let name = if let Tok::Ident(n) = &self.cur.0 {
                let n = n.clone();
                self.bump()?;
                n
            } else {
                return Err(LetError {
                    message: format!("WHERE expects identifier, got {:?}", self.cur.0),
                    pos: self.cur.1,
                });
            };
            self.expect(&Tok::Equals)?;
            let e = self.parse_expr()?;
            wheres.push((name, e));
        }
        self.expect(&Tok::EndKw)?;
        if outputs.len() != results.len() {
            return Err(LetError {
                message: format!(
                    "LET declares {} outputs but body has {} result expressions",
                    outputs.len(), results.len()
                ),
                pos: 0,
            });
        }
        Ok(LetForm { inputs, outputs, results, wheres })
    }

    fn parse_expr(&mut self) -> Result<Expr, LetError> { self.parse_compare() }

    fn parse_compare(&mut self) -> Result<Expr, LetError> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.cur.0 {
                Tok::Less      => BinOp::Lt,
                Tok::Greater   => BinOp::Gt,
                Tok::LessEq    => BinOp::Le,
                Tok::GreaterEq => BinOp::Ge,
                Tok::EqEq      => BinOp::Eq,
                Tok::NotEq     => BinOp::Ne,
                _ => break,
            };
            self.bump()?;
            let rhs = self.parse_add()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr, LetError> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.cur.0 {
                Tok::Plus  => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump()?;
            let rhs = self.parse_mul()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, LetError> {
        let mut lhs = self.parse_pow()?;
        loop {
            let op = match self.cur.0 {
                Tok::Star  => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                _ => break,
            };
            self.bump()?;
            let rhs = self.parse_pow()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_pow(&mut self) -> Result<Expr, LetError> {
        let lhs = self.parse_unary()?;
        if self.cur.0 == Tok::StarStar {
            self.bump()?;
            let rhs = self.parse_pow()?;
            Ok(Expr::Bin(BinOp::Pow, Box::new(lhs), Box::new(rhs)))
        } else {
            Ok(lhs)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, LetError> {
        if self.cur.0 == Tok::Minus {
            self.bump()?;
            let e = self.parse_unary()?;
            Ok(Expr::Neg(Box::new(e)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, LetError> {
        let (tok, pos) = self.bump()?;
        match tok {
            Tok::Num(n) => Ok(Expr::Lit(n)),
            Tok::Ident(name) => {
                if self.cur.0 == Tok::LParen {
                    self.bump()?;
                    let mut args = Vec::new();
                    if self.cur.0 != Tok::RParen {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.cur.0 == Tok::Comma { self.bump()?; }
                            else { break; }
                        }
                    }
                    self.expect(&Tok::RParen)?;
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Tok::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen)?;
                Ok(e)
            }
            other => Err(LetError {
                message: format!("unexpected token in expression: {other:?}"),
                pos,
            }),
        }
    }
}

pub fn parse(source: &str) -> Result<LetForm, LetError> {
    let mut p = Parser::new(source)?;
    let form = p.parse_form()?;
    if p.cur.0 != Tok::Eof {
        return Err(LetError {
            message: format!("trailing tokens after END: {:?}", p.cur.0),
            pos: p.cur.1,
        });
    }
    Ok(form)
}

// ── Unit tests (port of WF64's) ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> LetForm {
        parse(s).unwrap_or_else(|e| panic!("parse failed: {e}"))
    }

    #[test]
    fn parses_minimal_let() {
        let f = p("LET (r) -> (a) = r END");
        assert_eq!(f.inputs.len(), 1);
        assert_eq!(f.inputs[0].name, "r");
        assert!(f.inputs[0].destructure.is_none());
        assert_eq!(f.outputs, vec!["a"]);
        assert_eq!(f.results.len(), 1);
        assert!(f.wheres.is_empty());
    }

    #[test]
    fn parses_space_separated_inputs() {
        let f = p("LET (a b) -> (c) = a + b END");
        assert_eq!(f.inputs.len(), 2);
        assert_eq!(f.inputs[0].name, "a");
        assert_eq!(f.inputs[1].name, "b");
    }

    #[test]
    fn parses_space_separated_outputs() {
        // Output list accepts spaces, like the input list (regression:
        // it used to be comma-only, so `-> (sx sy)` errored).
        let f = p("LET (a b) -> (sx sy) = a + b, a - b END");
        assert_eq!(f.outputs, vec!["sx", "sy"]);
        assert_eq!(f.results.len(), 2);
    }

    #[test]
    fn parses_comma_separated_outputs() {
        // Commas still work too — both separators are accepted.
        let f = p("LET (a b) -> (sx, sy) = a + b, a - b END");
        assert_eq!(f.outputs, vec!["sx", "sy"]);
    }

    #[test]
    fn parses_destructure_clause() {
        let f = p("LET (p:point as x y) -> (m) = x + y END");
        assert_eq!(f.inputs.len(), 1);
        assert_eq!(f.inputs[0].name, "p");
        let d = f.inputs[0].destructure.as_ref().unwrap();
        assert_eq!(d.class, "point");
        assert_eq!(d.slots, vec!["x", "y"]);
    }

    #[test]
    fn parses_destructure_and_plain_mixed() {
        let f = p("LET (a:point as ax ay, b) -> (d) = ax + b END");
        assert_eq!(f.inputs.len(), 2);
        assert_eq!(f.inputs[0].name, "a");
        let d0 = f.inputs[0].destructure.as_ref().unwrap();
        assert_eq!(d0.slots, vec!["ax", "ay"]);
        assert_eq!(f.inputs[1].name, "b");
        assert!(f.inputs[1].destructure.is_none());
    }

    #[test]
    fn parses_runtime_test_shape() {
        // Exact text the runtime test would capture (with leading
        // whitespace as it appears inside an indented method body).
        let src = "LET ( a b ) -> ( c ) =\n                sqrt(a^2 + b^2)\n            END";
        let f = parse(src).unwrap_or_else(|e| panic!("parse failed on indented LET: {e}"));
        assert_eq!(f.inputs.len(), 2);
    }

    #[test]
    fn parses_with_spaces_around_parens() {
        // Same as parses_space_separated_inputs but WITH inner
        // padding spaces.
        let f = p("LET ( a b ) -> ( c ) = a END");
        assert_eq!(f.inputs.len(), 2);
    }


    #[test]
    fn parses_arithmetic_with_precedence() {
        let f = p("LET (x) -> (y) = 1 + 2 * 3 END");
        match &f.results[0] {
            Expr::Bin(BinOp::Add, ..) => {}
            other => panic!("expected Add at top, got {other:?}"),
        }
    }

    #[test]
    fn parses_where_clauses() {
        let f = p("LET (x, y) -> (mag) = m WHERE m = x*x + y*y END");
        assert_eq!(f.wheres.len(), 1);
        assert_eq!(f.wheres[0].0, "m");
    }

    #[test]
    fn parses_pow_right_associative() {
        let f = p("LET (x) -> (y) = x ** 2 ** 3 END");
        match &f.results[0] {
            Expr::Bin(BinOp::Pow, _, r) => match r.as_ref() {
                Expr::Bin(BinOp::Pow, _, _) => {}
                _ => panic!("inner Pow expected on right"),
            },
            _ => panic!("Pow at top expected"),
        }
    }

    #[test]
    fn parses_unary_minus() {
        let f = p("LET (x) -> (y) = -x END");
        match &f.results[0] {
            Expr::Neg(_) => {}
            _ => panic!("expected Neg"),
        }
    }

    #[test]
    fn parses_function_call() {
        let f = p("LET (x) -> (y) = sin(x) END");
        match &f.results[0] {
            Expr::Call(name, args) => {
                assert_eq!(name, "sin");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn lexes_line_comments() {
        let f = p("LET (x) -> (y) = x \\ ignored\n END");
        assert_eq!(f.inputs.len(), 1);
    }

    #[test]
    fn rejects_arity_mismatch() {
        let e = parse("LET (x) -> (a, b) = x END").unwrap_err();
        assert!(e.message.contains("outputs"));
    }
}
