//! `let_lang` — NewFactor's port of WF64's LET sub-language.
//!
//! `LET ( inputs ) -> ( outputs ) = expr_list WHERE ... END`
//! compiles to a Factor `[| inputs | body ] call( ... -- ... )`
//! that the surrounding ANS Forth code embeds inline.  Factor's
//! optimiser unboxes the floats through the expression chain, so
//! the runtime cost is competitive with hand-written XMM asm.
//!
//! Grammar (informal, mirrors WF64's parser.rs):
//!
//! ```text
//! let-form  := 'LET' '(' ident-list ')' '->' '(' ident-list ')' '='
//!              expr (',' expr)* where-clause* 'END'
//! where     := 'WHERE' ident '=' expr
//! expr      := compare-expr
//! compare   := add (('<' | '>' | '<=' | '>=' | '==' | '!=') add)*
//! add       := mul (('+' | '-') mul)*
//! mul       := pow (('*' | '/') pow)*
//! pow       := unary ('**' pow)?               (right-associative)
//! unary     := '-' unary | postfix
//! postfix   := primary call-args?
//! primary   := number | ident | '(' expr ')'
//! ```
//!
//! Why a sub-language (rather than just expecting users to write
//! postfix): for math-heavy code like Mandelbrot kernels, the
//! postfix form drowns the algebra in stack juggling.  LET keeps
//! the algebra readable while NewFactor's compiler lowers it into
//! the postfix world cleanly.

pub mod parser;
pub mod codegen;

pub use parser::{parse, BinOp, Expr, LetError, LetForm};
pub use codegen::lower_to_factor;
