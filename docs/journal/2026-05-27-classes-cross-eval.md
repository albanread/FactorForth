# 2026-05-27 — classes: cross-eval persistence shipped

Seventh entry today (or thereabouts).  The pain point surfaced
by yesterday's perf benchmark — "you can define a CLASS in eval
N but its constructor `<classname>` isn't visible in eval N+1"
— is fixed.  Classes now persist across evals, exactly like
VARIABLE, CONSTANT, VALUE, and template definitions do.

## The change

Three new things had to thread through the existing
CompileContext / build_with_prior_state plumbing:

  - `CompileContext.classes: HashMap<String, Vec<String>>` —
    lowercased class name → flat slot list (parent + own).  This
    is the minimum metadata needed to size the constructor
    and emit the accessor names in subsequent compiles.
  - `build_with_prior_state` gains a `prior_classes` parameter
    and feeds it through to `lower_classes::compute_class_slots`
    (which already accepted prior classes but was being called
    with an empty map).
  - `sema` populates `user_words` and `user_effects` with the
    synthesised constructor + accessor names for every class
    in this compile, using the FLAT slot list.  Without this,
    `<oldclass>` etc resolve correctly but their effects were
    Unknown, which broke Factor-side effect inference for any
    body that called them.

About 60 lines of changes across `mod.rs`, `sema.rs`,
`effect.rs`, and `newfactor_ui.rs` (the F7 checker needed the
new `&snap.classes` argument too).

## The shape: "Factor's image is the truth"

The pattern matches everything we already do for cross-eval
persistence:

  - Factor's runtime keeps the actual *thing* alive (tuple
    class for CLASS, global cell for VALUE, dictionary entry
    for `:` defs, etc.)
  - We keep the *minimum metadata* on the Rust side needed to
    compile new code that *references* the existing thing
  - On every successful compile, we merge new metadata into
    the CompileContext so the next compile sees it

For classes that minimum is just the slot name list.  We don't
need to store the parent class chain (Factor has it), the
methods (Factor dispatches them), or the slot types (there
aren't any — slots are tag-erased).

## Method addition across evals

The test `method_added_in_later_eval` shows the realistic REPL
pattern: declare the class and generic in one eval, attach
methods incrementally as you go:

```forth
\ Eval 1
CLASS: shape ;
CLASS: square EXTENDS shape  SLOT: side  ;
GENERIC: area ( s -- a )

\ Eval 2 — only emits the new method, nothing else
METHOD: area ( s:square -- a )
    square>side dup f* ;

\ Eval 3 — uses the method
4.0e <square> area .       \ 16.0
```

The IR for eval 2 is *just*:

```factor
USING: accessors classes.tuple forth.runtime generic generic.standard io kernel math math.constants math.functions math.order namespaces ;
IN: scratchpad
M: square area square>side dup forth.runtime:f* ;
flush
```

One `M:` line.  No TUPLE: redefinition.  No accessor regeneration.
Pure incremental method addition against persistent class +
generic state.  This is how Factor users have always worked,
now available to ANS Forth users.

## Stats

  - 75 runtime tests (was 73), all green
  - 122/122 lib unit tests
  - Two new tests in `diag_classes_cross_eval.rs`:
    `class_visible_in_later_evals`, `method_added_in_later_eval`
  - Release binary 2.03 MB

## What's left in sprint 2

Sprint 2 was tracked as #64.  Two of its bullets shipped earlier
today (parent-class accessor flattening + constructor sizing
across EXTENDS).  Cross-eval persistence shipped just now.
Remaining:

  - Multi-method dispatch (GENERIC#: arity-N)
  - `:before` / `:after` / `:around` method combinations
  - `SUPER:` for calling parent's method body
  - Slot initial values (SLOT: x INIT 0.0e)
  - Per-class TYPEOF codes + CLASS-OF

Plus the new ask just from the user: **LET-methods** — extending
the LET infix DSL with `obj.slot` syntax so method bodies can be
written in algebraic form.  Two design shapes sketched in the
conversation; implementation is sprint-3 territory.

## Reflection

The architectural pattern keeps proving itself: each cross-eval
feature is "thread a HashMap through the build pipeline."  The
substrate (Factor's image-resident state) handles the hard part;
our Rust code just remembers what to refer to.

This is the seventh thing today that worked exactly the same
way:

  1. `ctx.user_words` for `:` defs
  2. `ctx.user_effects` for declared effects
  3. `ctx.templates` for CREATE/DOES> templates
  4. `ctx.values` for VALUE names (this morning)
  5. EDITOR_SNAPSHOT for the F7 checker (this afternoon)
  6. `Sema.class_slots` for flattened slot lists (earlier today)
  7. `ctx.classes` for cross-eval class persistence (just now)

Same shape every time.  Each is ~30 lines plus tests.  Each one
makes the language *feel* more like a real interactive system
than a batch compiler.

Tomorrow's pattern is almost certainly going to be the same
shape applied to whatever the next feature is.  Probably LET-
methods, given the user just asked.

— end of cross-eval entry
