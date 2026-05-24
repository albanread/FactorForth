//! `newfactor` — headless CLI for compiling and running ANS Forth.
//!
//! Today (Phase 2) the CLI is a **compiler-driver + diagnostic tool**.
//! Phase 3 adds the embedded-VM session so `newfactor <file.f>` will
//! actually execute the program; for now the binary is the
//! programmer's window into the compiler's internal state.
//!
//! Usage:
//!
//! ```text
//! newfactor <source.f>                — compile to IR, print to stdout
//! newfactor --eval "EXPR"             — same, with EXPR as inline source
//! newfactor --dump=tokens <source.f>  — token stream
//! newfactor --dump=ast    <source.f>  — parsed AST
//! newfactor --dump=sema   <source.f>  — semantic database
//! newfactor --dump=effects <source.f> — user-word effects
//! newfactor --dump=ir     <source.f>  — emitted Factor IR
//! newfactor --dump=all    <source.f>  — every stage, in order
//! ```
//!
//! These dumps exist for human debugging *and* AI-assisted code
//! review — paste a dump into a session and the receiver can reason
//! about what the compiler sees without re-deriving from source.

use std::process::ExitCode;

use newfactor::compiler::{
    self, dump as cdump, emit, lex, parse,
    sema::build as build_sema, EmitOpts,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DumpStage { Tokens, Ast, Sema, Effects, Ir, All }

fn parse_dump_stage(s: &str) -> Option<DumpStage> {
    Some(match s {
        "tokens"  | "lex"   => DumpStage::Tokens,
        "ast"     | "parse" => DumpStage::Ast,
        "sema"              => DumpStage::Sema,
        "effects" | "effect" => DumpStage::Effects,
        "ir"      | "emit"  => DumpStage::Ir,
        "all"               => DumpStage::All,
        _ => return None,
    })
}

struct Args {
    /// Inline source via --eval=...; takes precedence over source file.
    eval: Option<String>,
    /// Source file path.
    source_path: Option<String>,
    /// --dump=<stage>.
    dump: Option<DumpStage>,
    /// --help requested.
    help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        eval: None, source_path: None, dump: None, help: false,
    };
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < raw.len() {
        let a = &raw[i];
        if a == "--help" || a == "-h" {
            args.help = true;
        } else if let Some(rest) = a.strip_prefix("--eval=") {
            args.eval = Some(rest.to_string());
        } else if a == "--eval" {
            i += 1;
            args.eval = Some(raw.get(i).cloned()
                .ok_or("--eval needs an argument")?);
        } else if let Some(rest) = a.strip_prefix("--dump=") {
            args.dump = Some(parse_dump_stage(rest)
                .ok_or_else(|| format!("unknown dump stage `{rest}`; \
                                       try tokens/ast/sema/effects/ir/all"))?);
        } else if a == "--dump" {
            i += 1;
            let s = raw.get(i).cloned()
                .ok_or("--dump needs an argument")?;
            args.dump = Some(parse_dump_stage(&s)
                .ok_or_else(|| format!("unknown dump stage `{s}`"))?);
        } else if a.starts_with('-') {
            return Err(format!("unknown option `{a}`"));
        } else {
            if args.source_path.is_some() {
                return Err("multiple source files not supported".into());
            }
            args.source_path = Some(a.clone());
        }
        i += 1;
    }
    Ok(args)
}

fn print_help() {
    println!("{} {} — ANS Forth compiler", newfactor::NAME, env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("  newfactor [OPTIONS] [SOURCE_FILE]");
    println!();
    println!("OPTIONS:");
    println!("  --eval=EXPR       compile/dump EXPR as inline source");
    println!("  --dump=STAGE      print a phase dump instead of the IR");
    println!("                    stage = tokens | ast | sema | effects | ir | all");
    println!("  --help            show this message");
    println!();
    println!("Without --dump, prints the compiled Factor IR.  In Phase 3");
    println!("the CLI will hand the IR to the embedded VM and execute.");
}

fn read_source(args: &Args) -> Result<String, String> {
    if let Some(e) = &args.eval { return Ok(e.clone()); }
    let path = args.source_path.as_ref()
        .ok_or("no source: pass a file path or --eval=EXPR")?;
    std::fs::read_to_string(path)
        .map_err(|e| format!("read {path}: {e}"))
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    if args.help {
        print_help();
        return Ok(());
    }
    if args.eval.is_none() && args.source_path.is_none() {
        print_help();
        return Ok(());
    }
    let source = read_source(&args)?;

    // Tokenise + parse first; both are cheap and every dump path
    // needs them.
    let toks = lex(&source).map_err(|e| e.to_string())?;

    // Tokens-only dump: don't bother parsing.
    if args.dump == Some(DumpStage::Tokens) {
        print!("{}", cdump::dump_tokens(&toks));
        return Ok(());
    }

    let prog = parse(&toks).map_err(|e| e.to_string())?;

    if args.dump == Some(DumpStage::Ast) {
        print!("{}", cdump::dump_ast(&prog));
        return Ok(());
    }

    // Sema requires resolve to succeed; if not, surface the error.
    let sema = build_sema(prog.clone()).map_err(|e| e.to_string())?;

    match args.dump {
        Some(DumpStage::Sema) => {
            print!("{}", cdump::dump_sema(&sema));
            return Ok(());
        }
        Some(DumpStage::Effects) => {
            print!("{}", cdump::dump_effects(&sema));
            return Ok(());
        }
        _ => {}
    }

    // Effect diagnostics are warnings: surface to stderr but keep
    // going.  The IR uses the synth annotation, which is correct
    // by construction regardless of what the user declared.
    for w in &sema.effect_errors {
        eprintln!("newfactor: {w}");
    }

    let ir = emit(&sema, &EmitOpts::default());

    match args.dump {
        Some(DumpStage::Ir) => {
            print!("{}", cdump::dump_ir(&ir));
        }
        Some(DumpStage::All) => {
            print!("{}", cdump::dump_all(&toks, &prog, &sema, Some(&ir)));
        }
        Some(_) => {} // already handled
        None => {
            // No dump requested: print the raw IR (no header).
            // Phase 3 will instead hand this to the embedded VM.
            println!("{ir}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("newfactor: {e}");
            ExitCode::FAILURE
        }
    }
}
