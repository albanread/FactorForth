//! Lower a parsed `LetForm` to a Factor IR string.
//!
//! The output is a single Factor expression of the shape:
//!
//! ```factor
//! [| in_1 in_2 ... |
//!     <where-1-body> :> where_1_name
//!     <where-2-body> :> where_2_name
//!     ...
//!     <output-1-body>
//!     <output-2-body>
//!     ...
//! ] call( in_1 in_2 ... -- out_1 out_2 ... )
//! ```
//!
//! Embedded in surrounding Forth code, the LET form consumes
//! `inputs.len()` cells from the data stack, runs the body
//! (Factor's compiler unboxes the floats through the chain),
//! and pushes `outputs.len()` cells on exit.

use std::fmt::Write;

use super::parser::{BinOp, Expr, LetForm};

/// Produce Factor IR text for a LET form.  Returns the
/// `[| inputs | ... ] call( ... )` block as a string ready to
/// drop into the surrounding emit.  Errors only on truly
/// pathological forms — most validation happens at parse time.
pub fn lower_to_factor(form: &LetForm) -> Result<String, String> {
    let mut out = String::with_capacity(128);

    // Header: `[| in_1 in_2 ... |`
    out.push_str("[| ");
    for inp in &form.inputs {
        out.push_str(&factor_local(inp));
        out.push(' ');
    }
    out.push_str("| ");

    // WHERE bindings — emit in source order.  Forward references
    // are NOT supported here (mirrors WF64's parser order); a
    // future enhancement could topo-sort them.
    for (name, body) in &form.wheres {
        emit_expr(body, &mut out);
        out.push(' ');
        out.push_str(":> ");
        out.push_str(&factor_local(name));
        out.push(' ');
    }

    // Result expressions, in declared order.  At call-end they
    // sit on the data stack with result[0] deepest, result[N-1]
    // on top — matching WF64's "outputs[0] is the rightmost
    // result = FP-stack TOS" convention.
    for r in &form.results {
        emit_expr(r, &mut out);
        out.push(' ');
    }

    // Closer + runtime-checked call effect.
    out.push_str("] call( ");
    for inp in &form.inputs {
        out.push_str(&factor_local(inp));
        out.push(' ');
    }
    out.push_str("-- ");
    for outp in &form.outputs {
        out.push_str(&factor_local(outp));
        out.push(' ');
    }
    out.push(')');

    Ok(out)
}

/// Mangle a LET identifier to a Factor-safe name.  LET identifiers
/// follow `[a-zA-Z_][a-zA-Z0-9_]*` per the parser, which is
/// already valid Factor; but we prefix to avoid colliding with
/// the surrounding ANS namespace (where the LET expression is
/// embedded).
fn factor_local(name: &str) -> String {
    format!("nfl-{name}")
}

/// Emit a LET expression as postfix Factor code.  This is where
/// the algebra-to-stack translation happens.
fn emit_expr(e: &Expr, out: &mut String) {
    match e {
        Expr::Lit(n) => {
            // Emit as Factor float literal.  `5.0` rather than `5`
            // so Factor's `+` etc. dispatch on the float path.
            // For exact integers we still want a `.0` suffix.
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                let _ = write!(out, "{:.1}", n);
            } else {
                let _ = write!(out, "{}", n);
            }
        }
        Expr::Var(name) => {
            out.push_str(&factor_local(name));
        }
        Expr::Neg(inner) => {
            emit_expr(inner, out);
            out.push_str(" math:neg");
        }
        Expr::Bin(op, l, r) => {
            emit_expr(l, out);
            out.push(' ');
            emit_expr(r, out);
            out.push(' ');
            out.push_str(factor_bin_op(*op));
        }
        Expr::Call(name, args) => {
            // Step A: only the SSE-direct intrinsics (sqrt abs min
            // max floor ceil round trunc) and a small libm subset
            // are recognised.  Unknown names produce a placeholder
            // that the eval will error on at runtime — Step C will
            // add libm dispatch.
            for a in args {
                emit_expr(a, out);
                out.push(' ');
            }
            out.push_str(factor_call_target(name));
        }
    }
}

fn factor_bin_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "math:+",
        BinOp::Sub => "math:-",
        BinOp::Mul => "math:*",
        BinOp::Div => "math:/",
        BinOp::Pow => "math.functions:^",
        // Comparisons: Factor returns t/f; for LET we want 1.0/0.0
        // numeric.  Wrap each comparison in `1.0 0.0 ?` to convert.
        BinOp::Eq => "kernel:=  1.0 0.0 kernel:?",
        BinOp::Ne => "kernel:=  kernel:not  1.0 0.0 kernel:?",
        BinOp::Lt => "math.order:<   1.0 0.0 kernel:?",
        BinOp::Gt => "math.order:>   1.0 0.0 kernel:?",
        BinOp::Le => "math.order:<=  1.0 0.0 kernel:?",
        BinOp::Ge => "math.order:>=  1.0 0.0 kernel:?",
    }
}

/// Map a LET function name to a Factor word.  Step A handles the
/// common intrinsics + libm wrappers.  Step C will broaden the
/// coverage.
fn factor_call_target(name: &str) -> &'static str {
    match name {
        // SSE-direct.
        "sqrt"  => "math.functions:sqrt",
        "abs"   => "math:abs",
        "min"   => "math.order:min",
        "max"   => "math.order:max",
        "floor" => "math.functions:floor",
        "ceil"  => "math.functions:ceiling",
        "round" => "math.functions:round",
        "trunc" => "math.functions:truncate",
        // libm.
        "sin"   => "math.functions:sin",
        "cos"   => "math.functions:cos",
        "tan"   => "math.functions:tan",
        "asin"  => "math.functions:asin",
        "acos"  => "math.functions:acos",
        "atan"  => "math.functions:atan",
        "atan2" => "math.functions:atan2",
        "exp"   => "math.functions:exp",
        "log"   => "math.functions:log",
        "pow"   => "math.functions:^",
        "hypot" => "math.functions:hypot",
        // Pseudo-builtins.
        "pi"    => "math.constants:pi",   // no args; we still emit
                                         // call site, Factor pushes
        "e"     => "math.constants:e",
        // Anything else: emit as a bare word and let Factor's
        // parser complain at runtime.
        other => {
            // Returning &'static is hard for unknown names; this
            // branch is a stub — we'd need to take String here in
            // the proper impl.  For step A, treat unknown calls
            // as errors at lower time via an explicit check.
            // The match returns a known-bad name so the error is
            // obvious.
            let _ = other;
            "kernel:throw-LET-unknown-call"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse;
    use super::lower_to_factor;

    #[test]
    fn lowers_identity() {
        let f = parse("LET (x) -> (y) = x END").unwrap();
        let ir = lower_to_factor(&f).unwrap();
        // Expect a [| nfl-x | ... ] call( nfl-x -- nfl-y ) shape.
        assert!(ir.starts_with("[| nfl-x | "), "got {ir}");
        assert!(ir.contains("nfl-x"));
        assert!(ir.ends_with(")"), "got {ir}");
    }

    #[test]
    fn lowers_arithmetic() {
        let f = parse("LET (x) -> (y) = x * x + 1 END").unwrap();
        let ir = lower_to_factor(&f).unwrap();
        assert!(ir.contains("math:*"));
        assert!(ir.contains("math:+"));
        assert!(ir.contains("1.0"));
    }

    #[test]
    fn lowers_where_in_order() {
        let f = parse("LET (x) -> (y) = a + 1 WHERE a = x * 2 END").unwrap();
        let ir = lower_to_factor(&f).unwrap();
        // The where-binding for `a` must appear BEFORE the result.
        let a_bind = ir.find(":> nfl-a").expect("a binding present");
        let result_use = ir.rfind("nfl-a").expect("a use present");
        assert!(a_bind < result_use, "where-binding should precede use; got {ir}");
    }

    #[test]
    fn lowers_unary_minus() {
        let f = parse("LET (x) -> (y) = -x END").unwrap();
        let ir = lower_to_factor(&f).unwrap();
        assert!(ir.contains("math:neg"), "got {ir}");
    }
}
