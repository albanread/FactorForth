//! Compiler errors and source positions.
//!
//! The error model is ANS-flavoured: messages refer to ANS Forth
//! concepts (stack underflow, unknown word, malformed number),
//! never to Factor.  Source positions are 1-based line/column for
//! direct interop with editors and existing Forth tooling.

use std::fmt;

/// 1-based source position.  `line` and `col` count from 1; `byte_offset`
/// is a 0-based UTF-8 byte index into the original source string and is
/// the canonical anchor when slicing for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
    pub byte_offset: u32,
}

impl Pos {
    pub const START: Pos = Pos { line: 1, col: 1, byte_offset: 0 };
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.col)
    }
}

/// Half-open source range `[start, end)`.  `end` is the position of
/// the first character *after* the token (or EOF if the token ran
/// to end of file).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Pos,
    pub end:   Pos,
}

impl Span {
    pub fn point(p: Pos) -> Span { Span { start: p, end: p } }

    /// Length in source bytes.
    pub fn len(&self) -> usize {
        (self.end.byte_offset - self.start.byte_offset) as usize
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.start)
    }
}

/// All compile errors flow through this enum.  Variants carry a Span
/// pointing at the offending text; the `Display` impl produces a
/// ready-to-print one-liner.  Multi-line "context excerpt" rendering
/// is the renderer's job, not the error's.
#[derive(Clone, Debug, PartialEq)]
pub enum CompileError {
    /// An unterminated string literal — opening `."` or `S"` with no
    /// matching `"` before end of file.
    UnterminatedString { kind: &'static str, opened_at: Span },

    /// An unterminated `( ... )` block comment.
    UnterminatedBlockComment { opened_at: Span },

    /// A number-literal-shaped token that the parser couldn't decode
    /// (digit out of base range, empty after prefix, etc.).
    MalformedNumber { token: String, at: Span, reason: &'static str },
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::UnterminatedString { kind, opened_at } =>
                write!(f, "unterminated {kind} string opened at {opened_at}"),
            CompileError::UnterminatedBlockComment { opened_at } =>
                write!(f, "unterminated ( comment opened at {opened_at}"),
            CompileError::MalformedNumber { token, at, reason } =>
                write!(f, "malformed number `{token}` at {at}: {reason}"),
        }
    }
}

impl std::error::Error for CompileError {}
