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
