//! ANS Forth tokeniser.  Phase 2.1.
//!
//! ANS Forth is whitespace-delimited, with a handful of parsing-word
//! prefixes that consume different shapes of input:
//!
//!   - `\` — line comment to end of line.
//!   - `(` — block comment to next `)` (must be space-delimited from
//!     surrounding text — `( a -- b )` not `(a--b)`).
//!   - `."` — runtime-emitting string; consumes until next `"`.
//!   - `S"` — counted-string literal; consumes until next `"`.
//!   - `C"` — counted-string literal (ANS optional); consumes until next `"`.
//!
//! Everything else is a whitespace-delimited token.  The lexer does
//! NOT try to classify those tokens as words-vs-numbers; that's the
//! parser's job, which has access to current BASE and the dictionary.
//! What we *do* recognise here are number prefixes (`$`, `%`, `#`,
//! `0x`) and float-shape (`.` or `e`/`E`) because the lexer needs to
//! know whether a `.` mid-token is decimal-point or part of a
//! Factor-style word name — ANS Forth `.` is the print-word, but
//! `1.5` is a float literal.
//!
//! ANS standard: tokens are case-insensitive.  We preserve original
//! case in the token text; case-folding is resolve's job.
//!
//! ## Source positions
//!
//! Every token carries a `Span` with 1-based line/column for both
//! ends, plus a 0-based byte offset for slicing.  Spans round-trip
//! through `&str` lifetimes; the lexer does not retain references to
//! the source after returning.

use super::error::{CompileError, Pos, Span};

/// Token kind.  The variants carry the *interpreted* form (numeric
/// value, decoded string), not the raw source slice.  Round-trip
/// rendering reconstructs source from the kind + span text where
/// needed (see `Token::source_text`).
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// Any word that wasn't a number, string, or comment.  Original
    /// case preserved.  Resolve normalises before dictionary lookup.
    Word(String),

    /// Integer literal.  `base` records the prefix actually used,
    /// so the IR-emitter can echo a faithful Factor-side literal
    /// (`HEX:`, `OCT:`, etc.) and diagnostics can quote the
    /// source-form.
    Int { value: i64, base: NumBase, raw: String },

    /// Float literal.  ANS `e`-notation: `1.5e`, `2e3`, `2.5e0`.
    /// We accept the Rust subset (`1.5`, `1.5e3`, `2e-1`) as well —
    /// no ANS conformance reason not to.
    Float { value: f64, raw: String },

    /// String literal.  `kind` distinguishes `."` (emit at runtime)
    /// from `S"` / `C"` (counted-string push).  The inner string is
    /// the *decoded* content (no surrounding quotes; no escapes
    /// processed — ANS strings are raw).
    Str { value: String, kind: StringKind },

    /// `\` line comment.  Content does NOT include the leading `\`
    /// or the trailing newline.  Preserved so we can round-trip
    /// source for diagnostics and (eventually) the formatter.
    LineComment(String),

    /// `( ... )` block comment.  Content does NOT include the
    /// surrounding parens.  May span multiple lines.
    BlockComment(String),

    /// `LET ( ... ) -> ( ... ) = ... END` — a captured LET-DSL
    /// block.  The contained string is the raw block text
    /// including the `LET` and `END` keywords; the let_lang
    /// sub-parser parses it independently with its own lexer
    /// (which understands infix operators, parens-as-grouping,
    /// `,`, `->`, `=`, identifier and number literals).
    LetBlock(String),
}

/// Integer literal base — the prefix the source used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumBase {
    /// No prefix — parsed in decimal at lex time.  (BASE is a
    /// runtime concept; we assume decimal at lex time and trust
    /// users that aren't toggling BASE.  Programs that change BASE
    /// mid-source need a small parser-time tracker; deferred.)
    Decimal,
    /// `$` prefix — hex.
    Hex,
    /// `%` prefix — binary.
    Binary,
    /// `#` prefix — explicit decimal.
    DecimalExplicit,
    /// `0x` prefix — hex, modern convention.  Used in the demo
    /// (`0x000000`).  Not in ANS but universally understood.
    Hex0x,
}

/// String-literal kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringKind {
    /// `." ... "` — emit at runtime (ANS 6.2.0190).
    DotQuote,
    /// `S" ... "` — push counted string c-addr u (ANS 6.1.2165).
    SQuote,
    /// `C" ... "` — push counted-string address (ANS 6.2.0855).
    CQuote,
    /// `S$" ... "` — NewFactor managed-string literal (M2.x #43).
    /// Pushes a Factor `string` handle (immutable, GC-tracked,
    /// Unicode-aware) — the modern-Forth-application string type
    /// that sidesteps PAD, counted-strings, and lifetime traps.
    SDollarQuote,
}

/// A token plus its source span.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: Tok,
    pub span: Span,
}

// ─── The lexer ──────────────────────────────────────────────────────────────

/// Tokenise an ANS Forth source string.  Returns the full token list
/// or the first error encountered.  Comments and string literals
/// don't recover — once we open one and find no terminator, we error.
pub fn lex(source: &str) -> Result<Vec<Token>, CompileError> {
    let mut lx = Lexer::new(source);
    let mut out = Vec::new();
    while let Some(tok) = lx.next_token()? {
        out.push(tok);
    }
    Ok(out)
}

struct Lexer<'src> {
    src: &'src [u8],
    /// 0-based byte offset, currently-being-processed character.
    i: usize,
    /// 1-based line number (advances on `\n`).
    line: u32,
    /// 1-based column.  Reset to 1 after `\n`.
    col: u32,
}

impl<'src> Lexer<'src> {
    fn new(source: &'src str) -> Self {
        Lexer { src: source.as_bytes(), i: 0, line: 1, col: 1 }
    }

    fn pos(&self) -> Pos {
        Pos { line: self.line, col: self.col, byte_offset: self.i as u32 }
    }

    fn at_eof(&self) -> bool { self.i >= self.src.len() }

    fn peek(&self) -> Option<u8> { self.src.get(self.i).copied() }

    /// Advance one byte, updating line/col.  Forth source is ASCII
    /// in practice — anything past 0x7F we still byte-step over,
    /// which is harmless because we only ever compare with ASCII.
    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    /// Skip ASCII whitespace including newlines.
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                self.bump();
            } else { break; }
        }
    }

    /// Read everything from current position to (but not including)
    /// the next end-of-line.  Consumes the EOL terminator itself.
    fn read_to_eol(&mut self) -> String {
        let start = self.i;
        while let Some(b) = self.peek() {
            if b == b'\n' { break; }
            self.bump();
        }
        let s = std::str::from_utf8(&self.src[start..self.i])
            .unwrap_or("").trim_end_matches('\r').to_string();
        // Consume the '\n' if present.
        if self.peek() == Some(b'\n') { self.bump(); }
        s
    }

    /// Read until matching `)` or EOF.  `open_pos` is where the `(`
    /// was, used for the error message.  Returns the comment content
    /// (without the parens).
    fn read_paren_comment(&mut self, open_pos: Pos) -> Result<String, CompileError> {
        let start = self.i;
        loop {
            match self.peek() {
                None => return Err(CompileError::UnterminatedBlockComment {
                    opened_at: Span { start: open_pos, end: self.pos() },
                }),
                Some(b')') => {
                    let body = std::str::from_utf8(&self.src[start..self.i])
                        .unwrap_or("").to_string();
                    self.bump(); // consume ')'
                    return Ok(body);
                }
                Some(_) => { self.bump(); }
            }
        }
    }

    /// Read until matching `"` or EOF.  `open_pos` is where the
    /// opening quote prefix was, for the error message.  Returns
    /// the string body (no surrounding quotes).
    fn read_quoted_string(
        &mut self, kind: StringKind, open_pos: Pos,
    ) -> Result<String, CompileError> {
        let start = self.i;
        loop {
            match self.peek() {
                None => {
                    let kname = match kind {
                        StringKind::DotQuote     => "dot-quote (.\")",
                        StringKind::SQuote       => "S-quote (S\")",
                        StringKind::CQuote       => "C-quote (C\")",
                        StringKind::SDollarQuote => "S$-quote (S$\")",
                    };
                    return Err(CompileError::UnterminatedString {
                        kind: kname,
                        opened_at: Span { start: open_pos, end: self.pos() },
                    });
                }
                Some(b'"') => {
                    let body = std::str::from_utf8(&self.src[start..self.i])
                        .unwrap_or("").to_string();
                    self.bump();  // consume closing "
                    return Ok(body);
                }
                Some(_) => { self.bump(); }
            }
        }
    }

    /// Read a whitespace-delimited token.  Caller has verified the
    /// first byte is a non-whitespace, non-special starter.  Returns
    /// the raw token text; classification (number vs. word) happens
    /// in `classify_word_or_number`.
    fn read_word_raw(&mut self) -> String {
        let start = self.i;
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' { break; }
            self.bump();
        }
        std::str::from_utf8(&self.src[start..self.i]).unwrap_or("").to_string()
    }

    /// Top-level dispatch.  Returns Ok(Some(tok)) for a token,
    /// Ok(None) at EOF, or Err for an unrecoverable lex error.
    fn next_token(&mut self) -> Result<Option<Token>, CompileError> {
        self.skip_ws();
        let start = self.pos();
        let Some(b) = self.peek() else { return Ok(None); };

        // ── Single-character parsing-word starters ──
        //
        // These look like a one-byte token in the source but the byte
        // following has to be whitespace OR end-of-file for the
        // parsing word to fire.  Otherwise it's just the first byte
        // of a regular word (e.g. `\foo` is a single word, but `\ foo`
        // is "line comment containing 'foo'").
        //
        // ANS-strictly, `\` and `(` MUST be space-delimited.  We
        // honour that.

        match b {
            b'\\' if self.next_is_ws_or_eof(1) => {
                self.bump();   // consume '\'
                // Consume the single space ANS requires after '\'
                if self.peek() == Some(b' ') || self.peek() == Some(b'\t') {
                    self.bump();
                }
                let body = self.read_to_eol();
                let end = self.pos();
                return Ok(Some(Token {
                    kind: Tok::LineComment(body),
                    span: Span { start, end },
                }));
            }
            b'(' if self.next_is_ws_or_eof(1) => {
                self.bump();   // consume '('
                // Consume the single space ANS requires after '('
                if self.peek() == Some(b' ') || self.peek() == Some(b'\t') {
                    self.bump();
                }
                let body = self.read_paren_comment(start)?;
                let end = self.pos();
                return Ok(Some(Token {
                    kind: Tok::BlockComment(body),
                    span: Span { start, end },
                }));
            }
            _ => {}
        }

        // ── Three-character parsing-word starter: S$" ──
        //
        // Must come BEFORE the two-char S" check so we don't lex
        // `S$" hello"` as `S` and then a stray `$"`.  This is the
        // NewFactor-extension managed-string literal (#43).
        let three = self.peek3();
        if matches!(three, Some((b'S', b'$', b'"')) | Some((b's', b'$', b'"'))) {
            self.bump(); self.bump(); self.bump();
            if self.peek() == Some(b' ') || self.peek() == Some(b'\t') { self.bump(); }
            let body = self.read_quoted_string(StringKind::SDollarQuote, start)?;
            let end = self.pos();
            return Ok(Some(Token {
                kind: Tok::Str { value: body, kind: StringKind::SDollarQuote },
                span: Span { start, end },
            }));
        }

        // ── Two-character parsing-word starters ──
        //
        // `."`, `S"`, `C"` followed by space then string body then `"`.
        // The space-after is part of the parsing word's contract in
        // ANS; we accept zero-or-one separator space.
        let two = self.peek2();
        if two == Some((b'.', b'"')) {
            self.bump(); self.bump();
            if self.peek() == Some(b' ') || self.peek() == Some(b'\t') { self.bump(); }
            let body = self.read_quoted_string(StringKind::DotQuote, start)?;
            let end = self.pos();
            return Ok(Some(Token {
                kind: Tok::Str { value: body, kind: StringKind::DotQuote },
                span: Span { start, end },
            }));
        }
        if matches!(two, Some((b'S', b'"')) | Some((b's', b'"'))) {
            self.bump(); self.bump();
            if self.peek() == Some(b' ') || self.peek() == Some(b'\t') { self.bump(); }
            let body = self.read_quoted_string(StringKind::SQuote, start)?;
            let end = self.pos();
            return Ok(Some(Token {
                kind: Tok::Str { value: body, kind: StringKind::SQuote },
                span: Span { start, end },
            }));
        }
        if matches!(two, Some((b'C', b'"')) | Some((b'c', b'"'))) {
            self.bump(); self.bump();
            if self.peek() == Some(b' ') || self.peek() == Some(b'\t') { self.bump(); }
            let body = self.read_quoted_string(StringKind::CQuote, start)?;
            let end = self.pos();
            return Ok(Some(Token {
                kind: Tok::Str { value: body, kind: StringKind::CQuote },
                span: Span { start, end },
            }));
        }

        // ── Generic whitespace-delimited token: number or word ──
        let raw = self.read_word_raw();

        // Special case: `LET` opens a sub-language block that's
        // parsed by `let_lang` with its own lexer.  Capture every
        // byte up to and including the matching space-delimited
        // `END`, and emit one Tok::LetBlock token with the whole
        // text.  The sub-parser handles the infix grammar.
        if raw.eq_ignore_ascii_case("let") {
            let block_end_byte = self.find_let_end(self.i, start)?;
            // Slice the raw source: from where `let` started (its
            // byte offset is `start.byte_offset`) to past the
            // matching END.
            let let_start_byte = start.byte_offset as usize;
            let text = std::str::from_utf8(&self.src[let_start_byte..block_end_byte])
                .map_err(|_| CompileError::UnterminatedString {
                    kind: "LET-block",
                    opened_at: Span { start, end: self.pos() },
                })?
                .to_string();
            // Advance the cursor past END.  Re-scanning line/col is
            // expensive; for now just advance byte and accept that
            // post-LET diagnostics may have stale line/col.  TODO:
            // walk the captured text counting newlines and update
            // self.line / self.col here.
            self.i = block_end_byte;
            let end = self.pos();
            return Ok(Some(Token {
                kind: Tok::LetBlock(text),
                span: Span { start, end },
            }));
        }

        let end = self.pos();
        let span = Span { start, end };
        let kind = classify_word_or_number(&raw, span)?;
        Ok(Some(Token { kind, span }))
    }

    /// Scan forward from byte offset `body_start` for a matching
    /// space-delimited `END` (case-insensitive) — the terminator of
    /// a `LET ... END` block.  Returns the byte offset just past
    /// the `END` (i.e. where the next token starts).  `let_start`
    /// is the position of the opening `LET` for diagnostics.
    fn find_let_end(&self, body_start: usize, let_start: Pos)
        -> Result<usize, CompileError>
    {
        // Walk forward token-by-token via a simple whitespace
        // tokeniser.  We don't strip comments here — the let_lang
        // parser handles its own comments; we just need to find
        // the right `END` word.
        let mut i = body_start;
        let bytes = self.src;
        while i < bytes.len() {
            // Skip whitespace.
            while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                i += 1;
            }
            if i >= bytes.len() { break; }
            // Skip a `\` line comment so an "end" inside it
            // doesn't false-match.
            if bytes[i] == b'\\'
                && (i + 1 >= bytes.len()
                    || (bytes[i + 1] as char).is_whitespace())
            {
                while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
                continue;
            }
            // Skip a `( ... )` block comment likewise.
            if bytes[i] == b'('
                && (i + 1 < bytes.len()
                    && (bytes[i + 1] as char).is_whitespace())
            {
                while i < bytes.len() && bytes[i] != b')' { i += 1; }
                if i < bytes.len() { i += 1; }
                continue;
            }
            // Read a word — non-whitespace run.
            let word_start = i;
            while i < bytes.len() && !(bytes[i] as char).is_whitespace() {
                i += 1;
            }
            let word = &bytes[word_start..i];
            if word.eq_ignore_ascii_case(b"end") {
                return Ok(i);
            }
        }
        Err(CompileError::UnterminatedString {
            kind: "LET-block",
            opened_at: Span { start: let_start, end: self.pos() },
        })
    }

    /// Peek two characters as a tuple, or None if not enough left.
    fn peek2(&self) -> Option<(u8, u8)> {
        let a = *self.src.get(self.i)?;
        let b = *self.src.get(self.i + 1)?;
        Some((a, b))
    }

    /// Peek three characters as a tuple — used for the `S$"` /
    /// `s$"` triple-prefix that mustn't be eaten by the 2-char
    /// `S"` matcher.
    fn peek3(&self) -> Option<(u8, u8, u8)> {
        let a = *self.src.get(self.i)?;
        let b = *self.src.get(self.i + 1)?;
        let c = *self.src.get(self.i + 2)?;
        Some((a, b, c))
    }

    /// True if the byte at offset `i + n` is whitespace or end-of-input.
    fn next_is_ws_or_eof(&self, n: usize) -> bool {
        match self.src.get(self.i + n) {
            None => true,
            Some(b) => matches!(*b, b' ' | b'\t' | b'\r' | b'\n'),
        }
    }
}

// ─── Number-vs-word classification ──────────────────────────────────────────

/// Look at a raw whitespace-delimited token and decide whether it's
/// a number literal (in which case parse the value) or a plain word.
/// Returns the resulting `Tok`.  Float detection wins over int: any
/// token containing `.` or unbracketed `e`/`E` (where the surrounding
/// chars are number-shaped) gets float-parsed.
fn classify_word_or_number(raw: &str, span: Span) -> Result<Tok, CompileError> {
    if raw.is_empty() {
        // Shouldn't happen — caller checked, but be defensive.
        return Ok(Tok::Word(String::new()));
    }

    // Comment-marker words are never numbers.  Already handled by the
    // top-level lexer when space-delimited, but a `\foo` token (no
    // space) reaches here.

    // Try float first — distinguishing feature is presence of `.` or `e`
    // in a number-shaped context.
    if looks_like_float(raw) {
        match parse_float(raw) {
            Some(value) => return Ok(Tok::Float { value, raw: raw.to_string() }),
            None => {
                return Err(CompileError::MalformedNumber {
                    token: raw.to_string(), at: span,
                    reason: "looks like a float but didn't parse",
                });
            }
        }
    }

    // Integer with explicit base prefix.
    //
    // `$<hex-digits>` is a hex literal (`$FF` = 255).  But many of
    // our managed-string words ALSO start with `$` (`$.`, `$+`,
    // `$len`, etc.).  Only treat as a hex number when the rest
    // is non-empty AND entirely hex-digit characters; otherwise
    // fall through to the word arm.  Matches the policy already
    // in place for `#` (decimal prefix vs `#S`/`#>`/...).
    if let Some(rest) = raw.strip_prefix('$') {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return parse_int_with_base(rest, 16, NumBase::Hex, raw, span);
        }
    }
    if let Some(rest) = raw.strip_prefix('%') {
        if !rest.is_empty() && rest.bytes().all(|b| matches!(b, b'0' | b'1')) {
            return parse_int_with_base(rest, 2, NumBase::Binary, raw, span);
        }
    }
    if let Some(rest) = raw.strip_prefix('#') {
        // `#<digits>` is an explicit-decimal literal (`#42`).
        // BUT — `#S`, `#>`, `#`, and other `#`-prefixed words are
        // user-visible ANS words (the pictured-numeric-output DSL).
        // Only treat as a number when the rest is non-empty AND
        // entirely digit characters; otherwise let it fall through
        // to the word arm.
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            return parse_int_with_base(rest, 10, NumBase::DecimalExplicit, raw, span);
        }
    }
    if let Some(rest) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        return parse_int_with_base(rest, 16, NumBase::Hex0x, raw, span);
    }

    // Negative-prefix forms (`-12`, `-$ff`) — handle by re-trying
    // after the `-` and negating.  We only treat as negative-number
    // if the rest parses as a number; otherwise it's a word (e.g.
    // `-rot`, `?-`).
    if let Some(rest) = raw.strip_prefix('-') {
        if !rest.is_empty()
            && (rest.starts_with(|c: char| c.is_ascii_digit())
                || rest.starts_with(['$', '%', '#'])
                || rest.starts_with("0x") || rest.starts_with("0X"))
        {
            if let Ok(tok) = classify_word_or_number(rest, span) {
                if let Tok::Int { value, base, raw: _ } = tok {
                    return Ok(Tok::Int {
                        value: value.wrapping_neg(),
                        base,
                        raw: raw.to_string(),
                    });
                }
                if let Tok::Float { value, raw: _ } = tok {
                    return Ok(Tok::Float {
                        value: -value,
                        raw: raw.to_string(),
                    });
                }
            }
        }
        // fall through to word
    }

    // Bare decimal integer (no prefix, all digits)?
    if raw.bytes().all(|b| b.is_ascii_digit()) {
        return parse_int_with_base(raw, 10, NumBase::Decimal, raw, span);
    }

    Ok(Tok::Word(raw.to_string()))
}

/// Returns true if the token "looks like" a float — contains `.` or
/// `e`/`E` in a sensible position.  False positives are filtered out
/// by parse_float returning None.
fn looks_like_float(raw: &str) -> bool {
    let b = raw.as_bytes();
    if b.is_empty() { return false; }
    // Tokens with explicit non-decimal prefixes are integers, even
    // if they happen to contain `e`/`E`/`.` digit characters
    // (`0xCAFE`, `$E0`, etc.).  Rule them out up front.
    if raw.starts_with("0x") || raw.starts_with("0X") { return false; }
    if matches!(b[0], b'$' | b'%' | b'#') { return false; }
    if (b[0] == b'-' || b[0] == b'+') && b.len() >= 2 {
        if matches!(b[1], b'$' | b'%' | b'#') { return false; }
        if raw[1..].starts_with("0x") || raw[1..].starts_with("0X") { return false; }
    }
    let first = b[0];
    if !(first.is_ascii_digit() || first == b'-' || first == b'+' || first == b'.') {
        return false;
    }
    // Must contain a digit somewhere.
    if !b.iter().any(|c| c.is_ascii_digit()) { return false; }
    // Heuristic: contains a `.` OR ends with `e`/`E` OR contains
    // `e`/`E` followed by digit-or-sign.
    let has_dot = b.iter().any(|c| *c == b'.');
    let has_exp = b.iter().enumerate().any(|(i, c)| {
        if *c == b'e' || *c == b'E' {
            // ANS allows `1.5e` with no exponent digits.
            i + 1 == b.len()
                || matches!(b.get(i + 1), Some(d) if d.is_ascii_digit() || *d == b'-' || *d == b'+')
        } else { false }
    });
    has_dot || has_exp
}

/// Parse a float, accepting both Rust-style `1.5e3` and ANS-style
/// `1.5e` (no exponent digits → treated as `1.5e0`).
fn parse_float(raw: &str) -> Option<f64> {
    // Strip a trailing lone `e`/`E` if present (ANS bare-e form).
    let stripped = if let Some(s) = raw.strip_suffix('e').or_else(|| raw.strip_suffix('E')) {
        // Only strip if there's a digit before — `e` alone isn't a number.
        if s.bytes().last().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
            s
        } else { raw }
    } else { raw };
    stripped.parse::<f64>().ok()
}

/// Parse `digits` in the given numeric base, with overflow handled
/// as wrapping i64 — ANS Forth treats cells as raw machine integers.
fn parse_int_with_base(
    digits: &str, base: u32, base_tag: NumBase, raw: &str, span: Span,
) -> Result<Tok, CompileError> {
    if digits.is_empty() {
        return Err(CompileError::MalformedNumber {
            token: raw.to_string(), at: span,
            reason: "number prefix with no digits",
        });
    }
    let mut acc: i64 = 0;
    for &c in digits.as_bytes() {
        let d = match c {
            b'0'..=b'9' => (c - b'0') as u32,
            b'a'..=b'f' => 10 + (c - b'a') as u32,
            b'A'..=b'F' => 10 + (c - b'A') as u32,
            b'_'        => continue,           // accept underscore separators
            _ => return Err(CompileError::MalformedNumber {
                token: raw.to_string(), at: span,
                reason: "non-digit character in number",
            }),
        };
        if d >= base {
            return Err(CompileError::MalformedNumber {
                token: raw.to_string(), at: span,
                reason: "digit out of base range",
            });
        }
        acc = acc.wrapping_mul(base as i64).wrapping_add(d as i64);
    }
    Ok(Tok::Int { value: acc, base: base_tag, raw: raw.to_string() })
}

// ─── Inline unit tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<Tok> {
        lex(s).expect("lex").into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn empty_input() {
        assert_eq!(lex("").unwrap(), Vec::new());
        assert_eq!(lex("   \t\n  ").unwrap(), Vec::new());
    }

    #[test]
    fn plain_words() {
        assert_eq!(
            kinds("dup drop swap"),
            vec![
                Tok::Word("dup".into()),
                Tok::Word("drop".into()),
                Tok::Word("swap".into()),
            ],
        );
    }

    #[test]
    fn decimal_int() {
        let t = kinds("42 -7");
        assert!(matches!(&t[0], Tok::Int { value: 42, base: NumBase::Decimal, .. }));
        assert!(matches!(&t[1], Tok::Int { value: -7, base: NumBase::Decimal, .. }));
    }

    #[test]
    fn hex_int() {
        let t = kinds("$ff 0xCAFE 0x000000");
        assert!(matches!(&t[0], Tok::Int { value: 255, base: NumBase::Hex, .. }));
        assert!(matches!(&t[1], Tok::Int { value: 0xCAFE, base: NumBase::Hex0x, .. }));
        assert!(matches!(&t[2], Tok::Int { value: 0, base: NumBase::Hex0x, .. }));
    }

    #[test]
    fn binary_int() {
        let t = kinds("%1010");
        assert!(matches!(&t[0], Tok::Int { value: 10, base: NumBase::Binary, .. }));
    }

    #[test]
    fn floats() {
        let t = kinds("1.5 2.5e 3e0 -1.25");
        assert!(matches!(&t[0], Tok::Float { value, .. } if (*value - 1.5).abs() < 1e-12));
        assert!(matches!(&t[1], Tok::Float { value, .. } if (*value - 2.5).abs() < 1e-12));
        assert!(matches!(&t[2], Tok::Float { value, .. } if (*value - 3.0).abs() < 1e-12));
        assert!(matches!(&t[3], Tok::Float { value, .. } if (*value - -1.25).abs() < 1e-12));
    }

    #[test]
    fn line_comment() {
        let t = kinds("\\ this is a comment\n42");
        assert_eq!(t.len(), 2);
        assert!(matches!(&t[0], Tok::LineComment(s) if s == "this is a comment"));
        assert!(matches!(&t[1], Tok::Int { value: 42, .. }));
    }

    #[test]
    fn block_comment() {
        let t = kinds(": square ( n -- n^2 ) dup * ;");
        // We expect: : square (block n -- n^2) dup * ;
        assert!(matches!(&t[0], Tok::Word(s) if s == ":"));
        assert!(matches!(&t[1], Tok::Word(s) if s == "square"));
        assert!(matches!(&t[2], Tok::BlockComment(s) if s.trim() == "n -- n^2"));
        assert!(matches!(&t[3], Tok::Word(s) if s == "dup"));
        assert!(matches!(&t[4], Tok::Word(s) if s == "*"));
        assert!(matches!(&t[5], Tok::Word(s) if s == ";"));
    }

    #[test]
    fn multi_line_block_comment() {
        let t = kinds("foo (  one\ntwo\nthree ) bar");
        assert!(matches!(&t[0], Tok::Word(s) if s == "foo"));
        assert!(matches!(&t[1], Tok::BlockComment(s) if s.contains("one") && s.contains("three")));
        assert!(matches!(&t[2], Tok::Word(s) if s == "bar"));
    }

    #[test]
    fn dot_quote_string() {
        let t = kinds(".\" hello world\" cr");
        assert!(matches!(&t[0], Tok::Str { value, kind: StringKind::DotQuote }
                                 if value == "hello world"));
        assert!(matches!(&t[1], Tok::Word(s) if s == "cr"));
    }

    #[test]
    fn s_quote_string() {
        let t = kinds("S\" hello\"");
        assert!(matches!(&t[0], Tok::Str { value, kind: StringKind::SQuote }
                                 if value == "hello"));
    }

    #[test]
    fn unterminated_string_errors() {
        let err = lex(".\" never closed").unwrap_err();
        assert!(matches!(err, CompileError::UnterminatedString { .. }),
                "got: {err:?}");
    }

    #[test]
    fn unterminated_block_comment_errors() {
        let err = lex("( open forever").unwrap_err();
        assert!(matches!(err, CompileError::UnterminatedBlockComment { .. }),
                "got: {err:?}");
    }

    #[test]
    fn dollar_non_hex_is_word_not_error() {
        // M2.x #43: the `$` prefix is shared between hex literals
        // (`$FF`) and the managed-string vocab (`$.`, `$+`, `$len`,
        // etc.).  Anything that's not entirely hex-digit-tail falls
        // through to the word arm rather than erroring as a
        // malformed hex number.  Resolution catches unknown words
        // later if the user really did mean `$gg` to be a number.
        let toks = lex("$gg").unwrap();
        assert!(matches!(toks[0].kind, Tok::Word(ref s) if s == "$gg"),
                "expected Word(\"$gg\"), got {:?}", toks[0].kind);
    }

    #[test]
    fn dollar_hex_still_parses() {
        let toks = lex("$ff").unwrap();
        assert!(matches!(toks[0].kind, Tok::Int { value: 0xff, .. }),
                "expected Int 0xff, got {:?}", toks[0].kind);
    }

    #[test]
    fn span_positions() {
        let t = lex("dup\n  drop").unwrap();
        assert_eq!(t[0].span.start.line, 1);
        assert_eq!(t[0].span.start.col, 1);
        assert_eq!(t[1].span.start.line, 2);
        assert_eq!(t[1].span.start.col, 3);
    }

    #[test]
    fn paren_is_not_comment_without_trailing_ws() {
        // ANS-strict: `(foo)` is a single word.  We honour that.
        let t = kinds("(foo)");
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], Tok::Word(s) if s == "(foo)"));
    }

    #[test]
    fn backslash_word() {
        // `\foo` (no space) is a word; only `\ foo` (with space) is a comment.
        let t = kinds("\\foo");
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], Tok::Word(s) if s == "\\foo"));
    }

    #[test]
    fn mandelbrot_demo_lexes() {
        // The headline demonstration's source must tokenise cleanly.
        let src = include_str!("../../demos/gfx-mandelbrot.f");
        let toks = lex(src).expect("demo should lex");
        // Spot-check: there's a `: mb-colour ( n -- rgb )` definition.
        let has_mb_colour = toks.iter().any(|t|
            matches!(&t.kind, Tok::Word(s) if s == "mb-colour"));
        assert!(has_mb_colour, "expected mb-colour word in demo tokens");
        // And a 0x000000 hex literal.
        let has_zero_rgb = toks.iter().any(|t|
            matches!(&t.kind, Tok::Int { value: 0, base: NumBase::Hex0x, .. }));
        assert!(has_zero_rgb, "expected 0x000000 in demo tokens");
    }
}
