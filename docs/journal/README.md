# Journal

Development notes, dated.  This is where the *thinking* lives — the
stuff that's too long for a commit message, too narrative for the
PLAN, and too rough for a finished design doc.

## Format

- One markdown file per day-ish, named `YYYY-MM-DD-slug.md`.
- Top of each file: date, what was shipped, what's still open.
- Free-form below.  Conversation, sketches, hand-traces, decisions
  that *almost* happened, false starts that didn't make it to
  `dead-ends.md` because they self-corrected fast.

## Why

Commits are terse and opaque.  `git log` tells you *what* changed
but not *why*, and certainly not what alternatives we considered
or what surprised us.  This folder is for that — written like
letters to whoever picks the project up later (which might be us
in six months).

## Related

- `MANIFESTO.md` — what we're building and why.  Stable.
- `PLAN.md` — the phase-by-phase schedule.  Updated as milestones
  land.
- `docs/dead-ends.md` — designs we tried and abandoned, with
  enough detail to re-find them.  Different from the journal: dead
  ends are crystallised conclusions; the journal is the working-out.

## Entries

- `2026-05-24-phase-1-and-2.3-shipped.md` — Phase 0 wrap, Phase 1
  vocabs + image + smoke, Phase 2 milestones 2.1–2.3 (lex, parse,
  resolve, emit) all the way to `5 square .` → `25` end-to-end.
  Also the variable-narrowing optimisation chat.
- `2026-05-24-m2.4-control-flow.md` — M2.4 control flow.  The
  WHILE-hangs-on-`[ dup ]` bug, watchdogs everywhere, and why
  emitting `[ pred zero? if-branch ] loop` beats Factor's
  built-in `while` for ANS predicates.
- `2026-05-24-m2.5-do-loop.md` — M2.5 DO/LOOP.  Three surprises:
  save-image-and-exit zeros our special-object slots (lazy
  init in accessors), `inline` is mandatory for words taking
  quotation arguments, and three LEAVE designs — only the
  flag-based one preserves the accumulator that ANS code keeps
  on the data stack across iterations.
- `2026-05-24-m2.6-case.md` — M2.6 CASE.  Structurally easy
  (recursive nested-IF chain); surfaced a latent vocabs_needed
  bug where emit-time fixed-string vocabs (forth.runtime,
  kernel, math, io) weren't always brought into USING:.  Fix
  was hardcoding them as baseline; cleaner refactor deferred.
- `2026-05-24-m2.7-effect-inference.md` — M2.7 first cut.
  Straight-line bodies get rigorous effect inference; control-
  flow bodies yield Unknown and the check is skipped.
  `: bad ( -- ) 1 2 ;` now reports the mismatch in pure Rust
  before any IR generation.  Sketches the control-flow formulas
  for the follow-up.
- `2026-05-24-sema-refactor.md` — Sema spine, phase-dump
  infrastructure, CLI integration.  And the corpus-design
  conversation: corpus is canonical, Sema is `analyze(corpus)`,
  FORGET is out, replay is the tool we have if we ever want
  it.  Phase 3's REPL gets a clean monotonic accumulation model
  on this foundation.
- `2026-05-24-m2.8-variables-and-constants.md` — VARIABLE,
  CONSTANT, FCONSTANT.  Escape analysis picks narrow vs wide;
  narrow uses Factor SYMBOL: + get-global/set-global/change-
  global via a peep-emit, wide gets a backing nf-addr.  The
  `+!` argument-order gotcha (change-global's signature is
  variable-then-quot) cost one debug cycle.
- `2026-05-24-effects-as-warnings.md` — the design pivot:
  synth-from-body is the ground truth, user's annotation is
  documentation, mismatches are warnings.  Control-flow effect
  formulas land (IF/THEN, IF/ELSE/THEN, BEGIN/UNTIL,
  BEGIN/WHILE/REPEAT, DO/LOOP).  Two separate effect maps in
  Sema: `user_effects` for caller typing, `body_effects` for
  ground truth.
- `2026-05-24-m2.9-collections.md` — "ANS Forth for applications,
  not micros."  Standard defining-words (`array`, `farray`,
  `cbuffer`) ship as built-in parser patterns; users get typed
  collections without ever writing CREATE/DOES>.  The user's
  three-step reframing of M2.9 from "model the byte-poking
  primitives" to "provide the common defining-words and reduce
  the need for byte-poking in the first place."
- `2026-05-24-m2.10-strings.md` — M2.10 strings.  ANS's notorious
  PAD-as-shared-temporary and dual c-addr/counted-string
  conventions both disappear in our nf-addr model.  `S"` correctly
  returns (c-addr, u); TYPE/CMOVE/FILL/BL ship.  Two stack-order
  bugs caught in FILL — same lesson as M2.5 and M2.9: write
  multi-step stack flow with `:: locals` not `bi`/`tri` shuffles.
- `2026-05-24-m2.10b-number-formatting.md` — M2.10b pictured
  numeric output.  `<# # #S sign hold #>` reframed as a small
  stack-based string builder; PAD is gone, replaced by a
  per-call SBUF that gets reversed at close time.  Plus
  base-switching (hex/decimal/binary/octal), `n>$` convenience,
  and a lexer paper-cut where `#S` had been misparsed as a
  malformed decimal-prefix number.
- `2026-05-24-m2.9b-templates.md` — M2.9b CREATE/DOES> as
  templates.  `:` definitions containing both CREATE and DOES>
  parse as Item::Template; `<n> name <newname>` triples in
  TopLevel are folded into Item::TemplateInstance by a
  pre-resolve sema pass.  Emit produces SYMBOL + buffer +
  accessor with the captured does_body inlined and `+`
  translated to `nf-addr+`.  Two ordering bugs (expand-after-
  resolve, row-vars-vs-concrete-effect) caught and fixed.
- `2026-05-24-phase-3.1a-session-foundation.md` — Phase 3.1a.
  `Session` abstraction lands: worker thread owns the VM,
  channel-based eval, three IoMode shapes, singleton-per-
  process enforcement, per-eval watchdog.  Extern functions
  defined but not yet wired through Factor (3.1b).
  Architecture sketch + the FFI-via-add-library plan for 3.1b.
