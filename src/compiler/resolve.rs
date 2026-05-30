//! Word resolution.  ANS Forth word name → Factor target binding.
//!
//! The dictionary is the **single source of truth** for which ANS
//! words NewFactor supports.  Resolve looks up each `WordRef` in
//! this dictionary and annotates it with its emit target:
//!
//!   - `Target::Builtin { factor_name }` — emit the name verbatim.
//!     The Factor IR's `USING:` line imports `vocab` so it's in scope.
//!   - `Target::QualifiedBuiltin { vocab, factor_name }` — emit as
//!     `vocab:factor_name` to dodge ambiguity (e.g. `.` collides
//!     between `prettyprint` and `forth.runtime`).
//!   - `Target::UserDefined { factor_name }` — emit verbatim; the
//!     name is a Forth-side `:` definition we've already compiled.
//!
//! Unknown words produce `ResolveError::UnknownWord` with the ANS
//! token's span — never any Factor frame.
//!
//! The dictionary in this file is the Phase 2.3 minimum: the words
//! needed to run `: square ( n -- n^2 ) dup * ; 5 square .`.
//! Later milestones grow it; the goal is for the table to stay
//! data-driven so adding ANS words doesn't touch this code.

use std::collections::HashMap;

use super::ast::{CaseArm, Definition, Expr, Item, Literal, Program};
use super::error::Span;

/// Which Factor name to emit for a given ANS word.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Emit `factor_name` bare; vocab must be in `USING:`.
    Builtin { vocab: &'static str, factor_name: &'static str },
    /// Emit `vocab:factor_name` — always disambiguates.
    QualifiedBuiltin { vocab: &'static str, factor_name: &'static str },
    /// User-defined word from this compilation unit.
    UserDefined { factor_name: String },
    /// Lexical local bound by the enclosing `:: name (…) … ;`
    /// (lowered from a Forth `{: … :}` block).  Emitted as the raw
    /// (lowercased) local name — NOT mangled — so it lines up with
    /// the binding Factor's `::` parsed from the effect annotation.
    Local { name: String },
}

impl Target {
    /// What goes into the IR for this word reference.
    pub fn to_factor_token(&self) -> String {
        match self {
            Target::Builtin { factor_name, .. } => (*factor_name).to_string(),
            Target::QualifiedBuiltin { vocab, factor_name } =>
                format!("{vocab}:{factor_name}"),
            Target::UserDefined { factor_name } => factor_name.clone(),
            Target::Local { name } => name.clone(),
        }
    }

    /// Which Factor vocab the IR needs to import (`USING:` clause).
    /// `None` for user-defined (they're emitted into the current
    /// vocab, no import needed).
    pub fn vocab(&self) -> Option<&'static str> {
        match self {
            Target::Builtin { vocab, .. } => Some(*vocab),
            Target::QualifiedBuiltin { vocab, .. } => Some(*vocab),
            Target::UserDefined { .. } => None,
            // Locals are bound in the enclosing `::`; they live in
            // the current vocab and need no extra USING entry.
            Target::Local { .. } => None,
        }
    }
}

/// Resolution errors.  Carries the offending span; the Display impl
/// produces ANS-style messages with line/column.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    UnknownWord { name: String, at: Span },
    RedefinedWord { name: String, at: Span, prev: Span },
    /// A `:` definition uses `RECURSE` but has no `( a -- b )`
    /// stack-effect annotation.  Factor's strict effect checker
    /// can't compile recursive bodies without one — surface the
    /// requirement here with a clear message rather than letting
    /// Factor reject the IR.
    RecurseNeedsEffect { word: String, at: Span },
    /// `TO name` where `name` isn't a VALUE (no such name, or it's
    /// a regular word / VARIABLE / CONSTANT).  Catching this here
    /// gives a useful message before Factor produces a "no word
    /// nf-value-X" parse error on the emitted IR.
    ToNotValue { name: String, at: Span },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownWord { name, at } =>
                write!(f, "unknown word `{name}` at {at}"),
            ResolveError::RedefinedWord { name, at, prev } =>
                write!(f, "redefinition of `{name}` at {at} (previously defined at {prev})"),
            ResolveError::RecurseNeedsEffect { word, at } =>
                write!(f, "`{word}` at {at} uses RECURSE but has no stack-effect annotation \
                           — add `( ... -- ... )` after the name"),
            ResolveError::ToNotValue { name, at } =>
                write!(f, "`TO {name}` at {at}: `{name}` is not a VALUE (TO only works on VALUEs)"),
        }
    }
}

impl std::error::Error for ResolveError {}

// ─── Built-in dictionary ────────────────────────────────────────────────────

/// The static dictionary of ANS → Factor mappings.  Grows as
/// milestones land.  Lookup is case-insensitive on the ANS side.
///
/// Vocabs used (must match what `emit::vocabs_needed` knows about):
///
///   - `kernel`           dup, drop, swap, over, rot
///   - `math`             +, -, *, /, mod
///   - `math.order`       <, >, <=, >=
///   - `forth.runtime`    ANS-specific I/O, memory, booleans
///   - `io`               flush
///
/// Words listed as `QualifiedBuiltin` are ambiguous in Factor's
/// default search path (e.g. `.` exists in `prettyprint` and in
/// `forth.runtime`); emit fully qualified to avoid the parser's
/// "resolves to more than one word" error.
pub fn builtin_table() -> HashMap<&'static str, Target> {
    use Target::*;
    let entries: &[(&str, Target)] = &[
        // Stack words ─ kernel
        ("dup",  Builtin { vocab: "kernel", factor_name: "dup"  }),
        ("drop", Builtin { vocab: "kernel", factor_name: "drop" }),
        ("swap", Builtin { vocab: "kernel", factor_name: "swap" }),
        ("over", Builtin { vocab: "kernel", factor_name: "over" }),
        ("rot",  Builtin { vocab: "kernel", factor_name: "rot"  }),
        ("nip",  Builtin { vocab: "kernel", factor_name: "nip"  }),
        ("tuck", Builtin { vocab: "kernel", factor_name: "tuck" }),
        // ANS stack words defined in forth.runtime.
        //
        // `?DUP` is intentionally NOT exposed here.  It's
        // stack-effect-polymorphic ( x -- 0 | x x ) which Factor's
        // strict static inference rejects: the runtime.factor body
        // `dup [ dup ] when` has uneven `if` branches.  Modern Forth
        // code prefers `dup IF ... THEN` over `?dup IF ... THEN`;
        // we'll revisit if a real-world ANS program needs it (e.g.
        // an emit-time inline-rewrite to `dup [ dup ] when` after
        // we disable per-def Factor inference, or as a macro).
        ("depth", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "depth" }),

        // ANS return-stack words.  Factor's data-stack-only model means
        // these route through a separate Forth-return-stack tuple
        // (see forth.runtime §2).  They're NOT Factor's own >r/r>/r@
        // (which manipulate the retainstack and are restricted).
        (">r",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: ">r"    }),
        ("r>",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "r>"    }),
        ("r@",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "r@"    }),
        ("rdrop", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "rdrop" }),
        ("2>r",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "2>r"   }),
        ("2r>",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "2r>"   }),

        // Arithmetic ─ math (Factor's `+ - * /` are not in `kernel`)
        ("+",    Builtin { vocab: "math", factor_name: "+"   }),
        ("-",    Builtin { vocab: "math", factor_name: "-"   }),
        ("*",    Builtin { vocab: "math", factor_name: "*"   }),
        ("/",    Builtin { vocab: "math", factor_name: "/i"  }),  // ANS / is integer-divide
        // ANS MOD is floored-division remainder (sign follows divisor).
        // Factor's math:mod is truncated (sign follows dividend) — wrong
        // for ANS.  Our forth.runtime:floored-mod implements the right
        // semantics for negative operands.  Task #42.
        ("mod",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "floored-mod" }),
        ("negate", Builtin { vocab: "math", factor_name: "neg" }),
        ("abs",  Builtin { vocab: "math", factor_name: "abs" }),
        ("min",  Builtin { vocab: "math.order", factor_name: "min" }),
        ("max",  Builtin { vocab: "math.order", factor_name: "max" }),

        // Comparisons return ANS -1 / 0 (NOT Factor's t / f).
        // M3.0.2 (#40): each comparator routes through an `ans*`
        // wrapper in forth.runtime that suffixes `bool>flag` to
        // convert Factor's boolean to ANS's -1 / 0 representation.
        // emit.rs::Expr::If wraps the consumed flag with a
        // `math:zero? not` before Factor's `kernel:if` so that
        // ANS 0 (false) becomes Factor's `f` and any nonzero value
        // becomes `t`.
        ("=",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans=" }),
        ("<>",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans<>" }),
        ("<",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans<" }),
        (">",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans>" }),
        ("<=",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans<=" }),
        (">=",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans>=" }),
        ("0=",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans0=" }),
        ("0<",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans0<" }),
        ("0>",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans0>" }),

        // Bitwise (ANS AND/OR/XOR/NOT are bitwise, not logical).
        ("and", Builtin { vocab: "math.bitwise", factor_name: "bitand" }),
        ("or",  Builtin { vocab: "math.bitwise", factor_name: "bitor"  }),
        ("xor", Builtin { vocab: "math.bitwise", factor_name: "bitxor" }),
        ("invert", Builtin { vocab: "math.bitwise", factor_name: "bitnot" }),

        // DO/LOOP support — I, J, LEAVE, UNLOOP.  The loop driver
        // itself (`do-loop` / `?do-loop`) is invoked from emit, not
        // through a name lookup; the user never writes its name.
        ("i",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "i" }),
        ("j",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "j" }),
        ("leave",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "leave" }),
        ("unloop", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "pop-loop-frame" }),

        // I/O — `.` collides with prettyprint, so always FQ
        (".",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "." }),
        ("u.", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "u." }),
        ("cr", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "cr" }),
        ("emit", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "emit" }),
        ("space", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "space" }),
        ("spaces", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "spaces" }),

        // ANS string vocabulary.  `type`, `cmove`, `fill` operate
        // on (c-addr, u) pairs; PAD doesn't exist in our model so
        // there are no clobbering surprises.  `bl` is the ASCII
        // space constant for buffer-clearing idioms.
        ("type",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "type" }),
        ("cmove", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "cmove" }),
        ("fill",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "fill" }),
        ("bl",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "bl" }),

        // Host I/O — KEY blocks for one byte; ACCEPT reads up to
        // u bytes into c-addr.  Both ultimately call the rt_*
        // extern functions through forth.runtime.
        ("key",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "key" }),
        ("accept", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "accept" }),

        // Pictured numeric output — the ANS DSL for formatting
        // numbers as strings.  All five build incrementally into
        // a session-scoped accumulator; #> closes and yields
        // (c-addr u).  See forth.runtime for the model.
        ("<#",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "<#" }),
        ("#",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "#" }),
        ("#s",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "#S" }),
        ("sign",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "sign" }),
        ("hold",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "hold" }),
        ("#>",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "#>" }),
        ("n>$",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "n>$" }),

        // Base-switching shortcuts (these aren't strictly ANS —
        // the spec just says `BASE` is settable — but they're so
        // universally provided that programs assume them.)
        ("hex",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "hex" }),
        ("decimal", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "decimal" }),
        ("binary",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "binary" }),
        ("octal",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "octal" }),

        // Memory model — `@` collides with math.ratios, so always FQ
        ("@",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "@" }),
        ("!",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-!" }),
        ("c@",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "c@" }),
        ("c!",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-c!" }),
        ("+!",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-+!" }),

        // Cell/char arithmetic — used to index into CREATE'd
        // arrays.  Implementations live in forth.runtime; they
        // also work on addresses produced by VARIABLE / CREATE.
        ("cell+",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "cell+" }),
        ("char+",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "char+" }),
        ("cells",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "cells" }),
        ("chars",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "chars" }),
        ("floats", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "floats" }),

        // ALLOT and HERE — primarily appear inside template
        // constructors where they're parser-level markers; the
        // forth.runtime versions are no-ops/stubs so non-template
        // uses don't crash.  Real allocation happens via
        // CollectionDef / TemplateInstance at sema time.
        ("allot",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "allot" }),
        ("here",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "here" }),

        // Float memory ops — used by `farray` instances.
        ("f@", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "f@" }),
        ("f!", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-f!" }),

        // ANS double-cell ↔ single-cell ↔ float conversions.
        // S>D / D>S are identity in our unified 64-bit cell model,
        // but ANS programs name them and we silently elide them.
        // D>F / F>D bridge to Factor's float type.
        ("s>d", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "s>d" }),
        ("d>s", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "d>s" }),
        ("d>f", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "d>f" }),
        ("f>d", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "f>d" }),

        // ANS float-arithmetic surface.  These are ALIAS:'d to the
        // integer versions in forth.runtime — Factor's polymorphic
        // `+` / `-` / `*` / `/` dispatch on the runtime types of
        // the values on the stack, so the same word handles floats
        // when the operands are floats.  Exposing them under the
        // ANS names so user code can write `F+` and have it resolve.
        ("f+", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "f+" }),
        ("f-", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "f-" }),
        ("f*", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "f*" }),
        ("f/", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "f/" }),
        // Float comparators use ans-* wrappers like the integer
        // versions — same boolean convention (-1 / 0).
        ("f<", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ansf<" }),
        ("f>", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ansf>" }),
        ("f=", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ansf=" }),

        // ANS execution token caller.  `EXECUTE` invokes an xt left
        // on the data stack.  Factor has the same notion (quotations
        // executed via `call`); ans-execute wraps it with the
        // expected ANS stack effect annotation.
        ("execute", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans-execute" }),

        // ── M2.x #39 ANS Core completeness ─────────────────────────
        // Arithmetic shortcuts.
        ("1+", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "1+" }),
        ("1-", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "1-" }),
        // Factor has `math:2/` (defined as `-1 shift`) but NO `math:2*`
        // — we wrap it as `ans2*` in forth.runtime to use `1 shift`.
        ("2*", QualifiedBuiltin  { vocab: "forth.runtime", factor_name: "ans2*" }),
        ("2/", QualifiedBuiltin  { vocab: "math",          factor_name: "2/" }),
        // Floored /MOD, */, */MOD — consistent with our MOD semantics.
        ("/mod",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans/mod" }),
        ("*/",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans*/" }),
        ("*/mod", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans*/mod" }),

        // Bit-shifts.  Factor's `shift` is signed-count bidirectional;
        // ANS LSHIFT / RSHIFT take an unsigned count with direction in
        // the word name.  Our wrappers in forth.runtime do the routing.
        ("lshift", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans-lshift" }),
        ("rshift", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans-rshift" }),

        // Double-stack manipulation.  Factor's `kernel` has 2DUP and
        // 2DROP with ANS semantics.  But Factor's `2over` has a
        // DIFFERENT signature `( x y z -- x y z x y )` — it's
        // `over over` — not ANS `2OVER ( a b c d -- a b c d a b )`.
        // And Factor's core ships no `2swap` at all.  We wrap our
        // own using locals.
        ("2dup",  Builtin            { vocab: "kernel",        factor_name: "2dup" }),
        ("2drop", Builtin            { vocab: "kernel",        factor_name: "2drop" }),
        ("2swap", QualifiedBuiltin   { vocab: "forth.runtime", factor_name: "ans2swap" }),
        ("2over", QualifiedBuiltin   { vocab: "forth.runtime", factor_name: "ans2over" }),

        // More stack ops from Factor's kernel.  `pick` is NOT here —
        // ANS `pick` takes a count (n PICK duplicates the n+1th item);
        // Factor's `pick` is hardwired to the 3rd item.  Filed as a
        // separate impl ticket — needs Factor's `get-datastack` /
        // index-from-top access.
        ("-rot",  Builtin { vocab: "kernel", factor_name: "-rot" }),

        // Pair fetch/store — ANS cell-pair access on a single address.
        ("2@", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans2@" }),
        ("2!", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans2!" }),

        // Memory clear — ERASE is FILL with 0.
        ("erase", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans-erase" }),

        // Inequality predicate against zero.  Returns ANS -1/0.
        ("0<>", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "ans0<>" }),

        // ── M2.x #32 ANS File Access (minimal) ─────────────────────
        // INCLUDED reads a file at (c-addr u) and evaluates it.
        // The Forth 2012 test runner uses this to load each .fth
        // module; everything else in the File Access Word Set is
        // deferred until needed.
        ("included", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-included" }),

        // ── M2.x #43 managed strings ($-vocab) ─────────────────────
        // Backed by Factor's native immutable `string` type — GC'd,
        // Unicode-aware, no PAD, no counted-string footguns.
        ("$len",       QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$len" }),
        ("$clen",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$clen" }),
        ("$+",         QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$+" }),
        ("$upper",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$upper" }),
        ("$lower",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$lower" }),
        ("$find",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$find" }),
        ("$contains?", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$contains?" }),
        ("$starts?",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$starts?" }),
        ("$ends?",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$ends?" }),
        ("$slice",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$slice" }),
        ("$cmp",       QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$cmp" }),
        ("$hash",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$hash" }),
        ("$.",         QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$." }),
        ("$.cr",       QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$.cr" }),
        ("int>$",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "int>$" }),
        ("$>int",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$>int" }),
        (">$",         QualifiedBuiltin { vocab: "forth.runtime", factor_name: ">$" }),
        ("$>addr",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "$>addr" }),

        // ANS EXIT — early return from the current colon definition.
        // Maps to Factor's continuations:return, which has effect
        // ( -- * ).  The `*` tells Factor's inferencer the rest of
        // the branch is unreachable, so e.g.  `dup 0= IF drop exit
        // THEN  + ` type-checks even though the IF branch doesn't
        // produce the same net effect as the THEN-fall-through path
        // — return signals "I never come back" and Factor honours
        // that.  Requires the enclosing colon body to be wrapped in
        // `[ ... ] with-return`; see emit.rs for the wrap logic
        // (only applied when EXIT is actually used, to avoid the
        // callcc allocation otherwise).
        ("exit",       QualifiedBuiltin { vocab: "continuations", factor_name: "return" }),

        // Type introspection — works with the polymorphic VALUE
        // design.  `TYPEOF ( x -- code )` returns a small stable
        // integer the user can CASE on; `INT?`/`FLOAT?`/`STRING?`/
        // `XT?`/`ADDR?` are ANS-style predicates returning -1/0.
        // The type-code constants are session-boot-defined Factor
        // CONSTANT:s so they fold to literal integers at JIT time.
        ("typeof",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-typeof"      }),
        ("int?",       QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-int?"        }),
        ("float?",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-float?"      }),
        ("string?",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-string?"     }),
        ("xt?",        QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-xt?"         }),
        ("addr?",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-addr-pred?"  }),
        // Programming-Tools word set.  `.s` is the non-destructive
        // stack print; `words` lists the user's own definitions;
        // `dump` is re-imagined to inspect the VALUE on top of stack
        // (type tag + value + hex/ASCII for strings/addrs) rather
        // than ANS's raw-memory `( addr u -- )` form, which makes no
        // sense against our opaque nf-addr model.  All boot-defined
        // in forth.runtime (see TOOLS_SETUP_SRC in session.rs).
        (".s",         QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-.s"          }),
        ("words",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-words"       }),
        ("dump",       QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-dump"        }),

        // CoreProtocols Layer 1 — mutable fixed-cell store backing
        // `grid` / `vector`.  A fixed-length Factor array with
        // settable elements, held in a slot.  `<cells>` allocates n
        // zeroed cells; `cells@`/`cells!` index it (0-based).
        ("<cells>",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-cells-new"   }),
        ("cells@",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-cells-at"    }),
        ("cells!",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-cells-set"   }),

        // Growable backing for `darray` (Layer 1's vector).
        ("<rawvec>",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-rawvec"      }),
        ("rawvec-push",QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-rawvec-push" }),
        ("rawvec-len", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-rawvec-len"  }),
        ("rawvec-at",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-rawvec-at"   }),
        ("rawvec-set", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-rawvec-set"  }),
        // Effect-annotated 1-in/0-out xt call — makes `each` inferable.
        ("call1",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-call1"       }),
        ("call1>",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-call1>"      }),
        ("call2>",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-call2>"      }),
        ("call2",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-call2"       }),
        ("(clone)",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-clone"       }),
        // dict (hashtable) backing primitives.
        ("<hash>",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-new"    }),
        ("hash-at",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-at"     }),
        ("hash!",      QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-set"    }),
        ("hash-key?",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-key?"   }),
        ("hash-del",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-del"    }),
        ("hash-len",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-len"    }),
        ("hash-keys",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-keys"   }),
        ("hash-vals",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-hash-values" }),
        // set (hash-set) backing primitives.
        ("<hashset>",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-set-new"     }),
        ("hs-add",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-set-add"     }),
        ("hs-in?",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-set-has?"    }),
        ("hs-del",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-set-del"     }),
        ("hs-len",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-set-len"     }),
        ("hs-members", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "nf-set-members" }),

        ("int-type",    QualifiedBuiltin { vocab: "forth.runtime", factor_name: "int-type"      }),
        ("float-type",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "float-type"    }),
        ("string-type", QualifiedBuiltin { vocab: "forth.runtime", factor_name: "string-type"   }),
        ("xt-type",     QualifiedBuiltin { vocab: "forth.runtime", factor_name: "xt-type"       }),
        ("addr-type",   QualifiedBuiltin { vocab: "forth.runtime", factor_name: "addr-type"     }),
        ("other-type",  QualifiedBuiltin { vocab: "forth.runtime", factor_name: "other-type"    }),

        // ── Graphics (forth.wf64-gfx) ────────────────────────────────
        //
        // Surface the iGui pane API to user Forth.  Backed by the
        // rt_gpane_* FFI exports in wf64::runtime (which queue into
        // a thread-safe SurfaceCmd batch and PostMessageW to the
        // GUI thread — Factor never touches Direct2D directly).
        ("gpane-open",        QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-open" }),
        ("gpane-begin",       QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-begin" }),
        ("gpane-present",     QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-present" }),
        ("gpane-clear",       QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-clear" }),
        ("gpane-fill-rect",   QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-fill-rect" }),
        ("gpane-stroke-rect", QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-stroke-rect" }),
        ("gpane-line",        QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-line" }),
        ("gpane-fill-circle", QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-fill-circle" }),
        ("gpane-next-event",  QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "gpane-next-event" }),

        // Doc-pane: a Forth-writable Markdown document window, backed
        // by the rt_doc_* FFI exports (igui::doc_pane).
        ("doc-open",          QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "doc-open" }),
        ("doc-set",           QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "doc-set" }),
        ("doc-append",        QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "doc-append" }),

        // Event-kind constants returned by gpane-next-event.
        // Factor side has them as EV_NONE etc.; ANS-side spelling
        // is the conventional lowercase-hyphen form.
        ("ev-none",        QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_NONE" }),
        ("ev-key",         QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_KEY" }),
        ("ev-char",        QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_CHAR" }),
        ("ev-mouse",       QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_MOUSE" }),
        ("ev-focus",       QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_FOCUS" }),
        ("ev-resize",      QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_RESIZE" }),
        ("ev-close",       QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_CLOSE" }),
        ("ev-frame-close", QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_FRAME_CLOSE" }),
        ("ev-tick",        QualifiedBuiltin { vocab: "forth.wf64-gfx", factor_name: "EV_TICK" }),
    ];
    entries.iter().map(|(k, v)| (*k, v.clone())).collect()
}

// ─── Resolution driver ──────────────────────────────────────────────────────

/// A program after resolution: every word reference annotated with
/// its emit target, every definition's name reachable as a built-in.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub program: Program,
    /// Per-`Expr::WordRef` → resolved target.  Keyed by source span
    /// (which is unique-per-token) so we don't have to mutate the AST.
    pub word_targets: HashMap<Span, Target>,
    /// User-defined word names from this compilation unit, with the
    /// span of their `:` definition for redefinition diagnostics.
    pub user_words: HashMap<String, Span>,
}

pub fn resolve(prog: Program) -> Result<Resolved, ResolveError> {
    let empty = HashMap::new();
    resolve_with_prior(prog, &empty)
}

/// Like [`resolve`] but seeded with names defined in PRIOR compilations
/// within the same interactive session.  When a user types `: foo ... ;`
/// in one REPL eval and then references `foo` in the next, the second
/// compile needs to know `foo` is a user word (not undefined).
///
/// `prior_user_words` carries names defined in previous compiles.
/// Lookup combines them with names defined in THIS compile's items.
/// Redefinition checking only fires WITHIN this compile — redefining
/// a prior word is allowed (Factor accepts redefinition; ANS Forth
/// programs commonly do it interactively).
///
/// The returned `Resolved.user_words` contains only THIS compile's
/// new definitions, so the host can merge them into its persistent
/// dictionary.
pub fn resolve_with_prior(
    prog: Program,
    prior_user_words: &HashMap<String, Span>,
) -> Result<Resolved, ResolveError> {
    resolve_with_prior_and_values(prog, prior_user_words, &HashMap::new())
}

/// Like [`resolve_with_prior`] but also accepts a set of VALUE names
/// defined in prior compiles.  TO references resolve against this
/// set (plus this compile's local VALUEs); a `TO name` whose target
/// isn't a VALUE produces `ToNotValue` at resolve time rather than
/// a Factor-side parse error later.
pub fn resolve_with_prior_and_values(
    prog: Program,
    prior_user_words: &HashMap<String, Span>,
    prior_value_names: &HashMap<String, Span>,
) -> Result<Resolved, ResolveError> {
    let empty_classes: HashMap<String, Vec<String>> = HashMap::new();
    resolve_with_prior_and_values_and_classes(
        prog, prior_user_words, prior_value_names, &empty_classes,
    )
}

/// Like [`resolve_with_prior_and_values`] but additionally accepts
/// a `class_slots` map (lowercased class name → flattened slot list,
/// parent slots first).  Used to register the right synthesised
/// constructor / accessor names per class.
pub fn resolve_with_prior_and_values_and_classes(
    prog: Program,
    prior_user_words: &HashMap<String, Span>,
    prior_value_names: &HashMap<String, Span>,
    class_slots: &HashMap<String, Vec<String>>,
) -> Result<Resolved, ResolveError> {
    let builtins = builtin_table();
    let mut user_words: HashMap<String, Span> = HashMap::new();
    // Local VALUE names this compile defines.  Combined with
    // `prior_value_names` below to form the lookup set for TO.
    let mut local_value_names: HashMap<String, Span> = HashMap::new();

    // Pass 1: collect user-defined word names so forward references
    // and recursion resolve correctly.  ANS Forth allows defining a
    // word that calls a later-defined word so long as parsing order
    // doesn't matter at runtime; we follow that.
    //
    // `:` defs, VARIABLE, and CONSTANT/FCONSTANT all introduce a
    // name into the user dictionary.  Redefinition checks fire for
    // all three uniformly.
    let mut register = |name: &str, at: Span, items: &mut HashMap<String, Span>| {
        let lc = name.to_ascii_lowercase();
        if let Some(prev) = items.get(&lc) {
            return Err(ResolveError::RedefinedWord {
                name: name.to_string(), at, prev: *prev,
            });
        }
        items.insert(lc, at);
        Ok(())
    };
    for item in &prog.items {
        match item {
            Item::Definition(d)        => register(&d.name, d.name_span, &mut user_words)?,
            Item::Variable(v)          => register(&v.name, v.name_span, &mut user_words)?,
            Item::Constant(c)          => register(&c.name, c.name_span, &mut user_words)?,
            Item::Create(cd)           => register(&cd.name, cd.name_span, &mut user_words)?,
            Item::Collection(cl)       => register(&cl.name, cl.name_span, &mut user_words)?,
            Item::Template(t)          => register(&t.name, t.name_span, &mut user_words)?,
            Item::TemplateInstance(ti) => register(&ti.name, ti.name_span, &mut user_words)?,
            Item::Value(v) => {
                register(&v.name, v.name_span, &mut user_words)?;
                // Also remember it as a VALUE specifically so TO
                // resolution can verify the target.
                local_value_names.insert(v.name.to_ascii_lowercase(), v.name_span);
            }
            Item::Class(c) => {
                // Look up the flattened slot list from the
                // pre-computed map and register every synthesised
                // accessor name (own + inherited).
                let class_lc = c.name.to_ascii_lowercase();
                let empty: Vec<String> = Vec::new();
                let all_slots = class_slots.get(&class_lc).unwrap_or(&empty);
                for (name, span) in super::lower_classes::class_synthesised_names(c, all_slots) {
                    register(&name, span, &mut user_words)?;
                }
            }
            Item::Generic(g) => {
                register(&g.name, g.name_span, &mut user_words)?;
            }
            // Method extends an existing generic — same name, no
            // separate registration.
            Item::Method(_) => {}
            // Raw Factor injection contributes no Forth-visible name.
            Item::RawFactor(_) => {}
            Item::TopLevel { .. } => {}
            // NEEDS is expanded away pre-resolve (exhaustiveness only).
            Item::Needs { .. } => {}
        }
    }

    // Build the combined lookup set: prior session-level names
    // PLUS this compile's new definitions.  Within-compile redef
    // is rejected above; across-compile redef is allowed and the
    // new def shadows the old (Factor will warn at load time but
    // accept it).
    let mut combined: HashMap<String, Span> = prior_user_words.clone();
    for (k, v) in &user_words {
        combined.insert(k.clone(), *v);
    }

    // Combined VALUE-name lookup for TO resolution.  Prior + local.
    let mut combined_values: HashMap<String, Span> = prior_value_names.clone();
    for (k, v) in &local_value_names {
        combined_values.insert(k.clone(), *v);
    }

    // Pass 2: resolve every WordRef in every body and at top level.
    let mut word_targets: HashMap<Span, Target> = HashMap::new();
    // Empty locals scope reused for every item that doesn't bind any.
    let no_locals: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &prog.items {
        match item {
            Item::Definition(d) => {
                // Build the locals scope as the UNION of
                //   * head-of-body locals (`Definition.locals`)
                //   * every mid-body `{: … :}` block's names
                // Collecting them up front keeps resolve simple: one
                // scope per def, valid throughout the body (a slight
                // relaxation of strict lexical order — the user just
                // writes their locals declarations in order, which is
                // the natural thing).
                let mut scope: std::collections::HashSet<String> = d.locals.iter()
                    .map(|l| l.name.to_ascii_lowercase())
                    .collect();
                collect_body_locals(&d.body, &mut scope);
                if scope.is_empty() {
                    resolve_exprs(&d.body, &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?;
                } else {
                    resolve_exprs(&d.body, &builtins, &combined, &combined_values, &scope, &mut word_targets)?;
                }
            }
            Item::TopLevel { exprs, .. } => resolve_exprs(exprs, &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?,
            // Computed-value CONSTANT/FCONSTANT bodies contain user-
            // visible word references (e.g. `3.5e 240e f/ FCONSTANT
            // mb-dx` references `f/`).  Resolve them too.  Literal-
            // valued constants have no body to walk.
            Item::Constant(c) => {
                if let crate::compiler::ast::ConstValue::Computed(exprs) = &c.value {
                    resolve_exprs(exprs, &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?;
                }
            }
            // VALUE has an initial-value body — resolve any word
            // references in there too.
            Item::Value(v) => {
                resolve_exprs(&v.initial, &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?;
            }
            // Variable, Create, Collection carry no expressions to
            // resolve — their bodies are AST-level data, not user-
            // visible word references.
            Item::Variable(_) | Item::Create(_) | Item::Collection(_) => {}
            // Templates have a constructor and does_body, both of
            // which may reference builtins.  Walk them so resolve
            // catches typos.
            Item::Template(t) => {
                resolve_exprs(&t.constructor, &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?;
                resolve_exprs(&t.does_body,   &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?;
            }
            // Template instances inherit the resolved does_body
            // from their source template; no separate resolution.
            Item::TemplateInstance(_) => {}
            // Class / Generic carry no Forth-side body.  Their
            // slot/effect metadata is parsed at AST time and consumed
            // by lower_classes / emit directly.
            Item::Class(_) | Item::Generic(_) | Item::RawFactor(_)
            | Item::Needs { .. } => {}
            // Method body resolves like a `:` body — same rules.
            // Methods don't yet support `{: … :}` locals; the
            // specializer-bound input names already act as bindings
            // via Factor's multi-methods machinery, and the lib so
            // far hasn't needed extras here.
            Item::Method(m) => {
                resolve_exprs(&m.body, &builtins, &combined, &combined_values, &no_locals, &mut word_targets)?;
            }
        }
    }

    // Return only THIS compile's new names — the host merges into
    // its persistent dictionary.
    Ok(Resolved { program: prog, word_targets, user_words })
}

/// Walk an expression tree and add every mid-body `{: … :}` block's
/// names to `scope`.  Used by `resolve_with_prior_*` to build the
/// union-scope for a definition's body in one pass before
/// `resolve_exprs` is called.
fn collect_body_locals(exprs: &[Expr], scope: &mut std::collections::HashSet<String>) {
    for e in exprs {
        match e {
            Expr::Locals { names, .. } => {
                for l in names {
                    scope.insert(l.name.to_ascii_lowercase());
                }
            }
            Expr::If { then_body, else_body, .. } => {
                collect_body_locals(then_body, scope);
                if let Some(eb) = else_body { collect_body_locals(eb, scope); }
            }
            Expr::BeginUntil { body, .. }
            | Expr::BeginAgain { body, .. }
            | Expr::DoLoop { body, .. } => {
                collect_body_locals(body, scope);
            }
            Expr::BeginWhileRepeat { pred, body, .. } => {
                collect_body_locals(pred, scope);
                collect_body_locals(body, scope);
            }
            Expr::Case { arms, default, .. } => {
                for arm in arms {
                    collect_body_locals(&arm.match_expr, scope);
                    collect_body_locals(&arm.body, scope);
                }
                if let Some(d) = default { collect_body_locals(d, scope); }
            }
            _ => {}
        }
    }
}

fn resolve_exprs(
    exprs: &[Expr],
    builtins: &HashMap<&'static str, Target>,
    user_words: &HashMap<String, Span>,
    value_names: &HashMap<String, Span>,
    locals: &std::collections::HashSet<String>,
    out: &mut HashMap<Span, Target>,
) -> Result<(), ResolveError> {
    for e in exprs {
        match e {
            Expr::Lit(_) => {}
            Expr::WordRef { name, span } => {
                let lc = name.to_ascii_lowercase();
                // Lexical locals (from a `{: … :}` declaration on the
                // enclosing colon-def) shadow ALL other names — same
                // rule any sensible language has.  Emitted raw so the
                // Factor-side `::` binding matches.
                if locals.contains(&lc) {
                    out.insert(*span, Target::Local { name: lc });
                } else if user_words.contains_key(&lc) {
                    // User-defined wins over builtins for the same name —
                    // ANS Forth's "most recent definition wins" rule.
                    out.insert(*span, Target::UserDefined {
                        factor_name: factor_user_name(&lc),
                    });
                } else if let Some(t) = builtins.get(lc.as_str()) {
                    out.insert(*span, t.clone());
                } else {
                    return Err(ResolveError::UnknownWord {
                        name: name.clone(), at: *span,
                    });
                }
            }
            Expr::To { name, span } => {
                let lc = name.to_ascii_lowercase();
                if !value_names.contains_key(&lc) {
                    return Err(ResolveError::ToNotValue {
                        name: name.clone(),
                        at: *span,
                    });
                }
                // We don't insert into word_targets — emit derives
                // the storage symbol from the name directly.  Resolve's
                // job here is just the existence check.
            }
            Expr::See { .. } => {
                // SEE's target is not a word reference — it's an
                // introspection target resolved at emit time against
                // the doc store.  No resolution needed here; an
                // unknown target is reported at emit (and prints a
                // friendly "unknown word" rather than failing compile).
            }
            Expr::If { then_body, else_body, .. } => {
                resolve_exprs(then_body, builtins, user_words, value_names, locals, out)?;
                if let Some(eb) = else_body {
                    resolve_exprs(eb, builtins, user_words, value_names, locals, out)?;
                }
            }
            Expr::BeginUntil { body, .. } |
            Expr::BeginAgain { body, .. } => {
                resolve_exprs(body, builtins, user_words, value_names, locals, out)?;
            }
            Expr::BeginWhileRepeat { pred, body, .. } => {
                resolve_exprs(pred, builtins, user_words, value_names, locals, out)?;
                resolve_exprs(body, builtins, user_words, value_names, locals, out)?;
            }
            Expr::DoLoop { body, .. } => {
                resolve_exprs(body, builtins, user_words, value_names, locals, out)?;
            }
            Expr::Case { arms, default, .. } => {
                for arm in arms {
                    resolve_exprs(&arm.match_expr, builtins, user_words, value_names, locals, out)?;
                    resolve_exprs(&arm.body, builtins, user_words, value_names, locals, out)?;
                }
                if let Some(d) = default {
                    resolve_exprs(d, builtins, user_words, value_names, locals, out)?;
                }
            }
            Expr::Tick { name, span } => {
                // ' name pushes the XT of `name`.  Same resolution
                // rules as a bare WordRef — must resolve to a known
                // word.  The resolved target is recorded against
                // this span so emit can render `\ <factor-name>`.
                let lc = name.to_ascii_lowercase();
                if locals.contains(&lc) {
                    // Tick on a local — odd but well-defined: emit the
                    // raw local name; Factor's `::` will pick it up.
                    out.insert(*span, Target::Local { name: lc });
                } else if user_words.contains_key(&lc) {
                    out.insert(*span, Target::UserDefined {
                        factor_name: factor_user_name(&lc),
                    });
                } else if let Some(t) = builtins.get(lc.as_str()) {
                    out.insert(*span, t.clone());
                } else {
                    return Err(ResolveError::UnknownWord {
                        name: name.clone(), at: *span,
                    });
                }
            }
            Expr::LetForm { .. } => {
                // LET forms are self-contained — no external word
                // references (only operators and built-in function
                // names that the let_lang codegen resolves itself).
                // No resolver work needed.
            }
            Expr::Locals { .. } => {
                // Mid-body `{: … :}` block.  Its names are already
                // in the locals scope (the caller builds the union
                // up front before walking) — no per-WordRef work to
                // do here; emit lowers each name to a `:>` binding.
            }
        }
    }
    Ok(())
}

/// Reserved prefix attached to every user-defined name in the emitted
/// Factor IR.  Picked to be bulletproof against vocab collisions: no
/// Factor word starts with `z-`, and the separator-character makes it
/// not-clever — we never have to reason about what `USING:` happens
/// to pull in, even if `sequences` / `sorting` show up later.
/// One-line change if we ever want a different scheme.
pub(crate) const USER_NAME_PREFIX: &str = "z-";

/// Mangle an ANS word name into a Factor-safe identifier.  This is
/// **the single chokepoint** every emit site routes through — and
/// the resolver hands the same mangled name back for caller-side
/// references — so a definition and its references always agree.
///
/// Two jobs in order:
///
///   1. Rewrite any ANS identifier that breaks Factor's parser
///      (`->` is the locals-arrow inside `::` defs, `}t` / `t{` are
///      flaky `}`-prefixed parser tokens) to a safe spelling.
///   2. Prefix every name with [`USER_NAME_PREFIX`] so it can never
///      collide with a Factor vocabulary word — `compare` becomes
///      `z-compare`, `area` becomes `z-area`, `<point>` becomes
///      `z-<point>`, and so on.  The IR is machine-only; the user
///      keeps writing the ANS name.
///
/// Names that are **not** user-defined — slot names embedded in
/// TUPLE: declarations, Factor's own `>>x` / `x>>` accessors, `boa`,
/// the `object` catch-all in specializer lists, and qualified
/// builtins like `kernel:dup` — are emitted raw at their source sites
/// and do not pass through here.
pub(crate) fn factor_user_name(ans_lc: &str) -> String {
    let safe = match ans_lc {
        "->" => "arrow",
        "}t" => "end-test",
        "t{" => "begin-test",
        other => other,
    };
    format!("{USER_NAME_PREFIX}{safe}")
}

// (vocabs_needed moved to emit.rs as part of the sema refactor —
// it operates over &Sema now, and lives next to the emit code that
// uses it.)

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{lex, parse};

    fn resolve_str(src: &str) -> Result<Resolved, ResolveError> {
        let toks = lex(src).unwrap();
        let prog = parse(&toks).unwrap();
        resolve(prog)
    }

    #[test]
    fn known_builtin_resolves() {
        let r = resolve_str("dup *").unwrap();
        assert_eq!(r.word_targets.len(), 2);
    }

    #[test]
    fn ans_dot_resolves_to_forth_runtime() {
        let r = resolve_str("42 .").unwrap();
        let target = r.word_targets.values().next().unwrap();
        assert!(matches!(target,
            Target::QualifiedBuiltin { vocab: "forth.runtime", factor_name: "." }));
    }

    #[test]
    fn user_defined_resolves() {
        let r = resolve_str(": square dup * ; 5 square").unwrap();
        // The `square` call at top level must resolve to user-defined,
        // and the factor_name must be the mangled form so it lines up
        // with the mangled definition the emitter writes.
        let expected = factor_user_name("square");
        let found = r.word_targets.values()
            .any(|t| matches!(t, Target::UserDefined { factor_name } if factor_name == &expected));
        assert!(found, "expected square to resolve as UserDefined with mangled name {expected:?}");
    }

    #[test]
    fn case_insensitive_lookup() {
        let r = resolve_str("DUP +").unwrap();
        assert_eq!(r.word_targets.len(), 2);
    }

    #[test]
    fn unknown_word_errors() {
        let err = resolve_str("blortz").unwrap_err();
        assert!(matches!(err, ResolveError::UnknownWord { ref name, .. } if name == "blortz"));
    }

    #[test]
    fn redefinition_errors() {
        let err = resolve_str(": foo 1 ; : foo 2 ;").unwrap_err();
        assert!(matches!(err, ResolveError::RedefinedWord { .. }));
    }

    // vocabs_needed lives in emit now; its tests moved there too.
}
