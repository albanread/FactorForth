# 2026-05-27 — the rename: FactorForth → Factor4th

Late entry, third today.  After the day's compiler work shipped
and the object-system design was filed, the question of what to
call the thing came up.  Tried "Factor4th" out loud and it stuck.

## Why the rename

The old name "FactorForth" had a persistent reading hiccup —
it looks like a compound noun ("Factor Forth") and people
(including me, typing) inconsistently capitalised or spaced it.
Searches were inconsistent.  The product name kept fighting the
prose around it.

"Factor4th" solves three things at once:

  - **Single-token clarity**: reads unambiguously as one word,
    types consistently, searches consistently
  - **Forth-tradition placement**: the "4" pun on "Forth" is
    classic in the community (`4tH`, et al.) and signals "this
    is a Forth implementation" without using the word Forth at
    all
  - **Distinct from Factor**: when people say "Factor" they mean
    Slava's language; "Factor4th" can't be confused with it

Honest tradeoffs noted: numbers-in-names can hurt some search
engines, and "4th" might be pronounced "fourth" once by a
newcomer before someone corrects them to "Forth".  Neither
matters much for a niche dev tool.

## What this sprint changes

**Forward-facing surface only.**  The internal codebase keeps the
old names where they're stable identifiers:

| Layer | Changes? | Why |
|-------|----------|-----|
| `release/factorforth/README.txt` | Rewritten | User-facing |
| `release/factorforth/docs/*.md` | Rewritten | User-facing |
| `docs/design/object-system.md` | Header updated | Current design |
| Icon (`tools/factor4th.ico`) | Regenerated as "f4" mark | New brand |
| Historical journal entries | **Unchanged** | Period record |
| Source code (`src/**`) | **Unchanged** | Sprint 2 |
| Binary names (`factorforth-ui.exe`) | **Unchanged** | Sprint 2 |
| Image filenames | **Unchanged** | Sprint 2 |
| Release directory path | **Unchanged** | Sprint 2 |
| Crate name `newfactor` | **Unchanged ever** | Internal namespace |
| FFI prefix `nf_*` | **Unchanged ever** | Internal namespace |
| Factor vocab `forth.runtime` | **Unchanged** | Internal namespace |

The `newfactor` crate name reads naturally as "new Factor
implementation" — orthogonal to the user-facing brand, so it
stays.  Same logic for `nf_*` FFI exports.

## The new icon

Replaced the cursive "ff" with a cursive "f4" on the same warm
amber + terracotta palette.  The 4 has more visual character
than the second f did — open angular form against the curved f
stem creates better contrast at small sizes where the original
"ff" tended to read as a single doubled glyph.  Italic shear and
palette are unchanged from the original mark so the new and old
icons share family resemblance during the transition.

Generated via `E:\factorforth-scratch\make_icon_f4.py` (copy of
the original generator with `glyph_text = "f4"`).  Output:
multi-resolution ICO (16/24/32/48/64/128/256) at
`tools/factor4th.ico`.  Also staged at
`tools/factorforth.ico.new` so the next binary rebuild can
either swap content into the existing path or update the .rc to
point at the new path — that's a sprint-2 decision.

## Canonical spellings

  - In prose: **Factor4th** (PascalCase, matching how Factor
    itself is capitalised)
  - In filenames / paths / Rust identifiers: `factor4th`
    (lowercase, Unix convention)
  - Spoken: "factor-forth" (the 4 is a Forth pun, pronounce it as
    Forth)

## Sprint 2 outline (deferred)

The full rename completes when these land:

  - `Cargo.toml`: `[[bin]] name = "factorforth-ui"` →
    `"factor4th-ui"`
  - File renames:
    - `factorforth-ui.exe` → `factor4th-ui.exe`
    - `factorforth.image` → `factor4th.image`
    - `factorforth.image.zst` → `factor4th.image.zst`
    - `release/factorforth/` → `release/factor4th/`
  - Source-string updates:
    - Title bars ("∴ FactorForth — Forth IDE" → "∴ Factor4th — Forth IDE")
    - Window class names
    - Menu strings
    - `tools/factorforth-ui.rc` → `tools/factor4th-ui.rc`
    - The `factorforth.image` / `factor.dll` path lookups in
      `session.rs::resolve_default_paths`
  - Icon-resource path update in `.rc`
  - Build verification: `cargo test`, side-by-side Mandelbrot
    smoke

Estimated effort: ~2 hours, mostly find-replace plus the build
verification.  Tracked as the back half of task #61.

## What's true today

Open the IDE → still launches as `factorforth-ui.exe` from
`release/factorforth/` because we didn't touch the binary
surface.  Title bar still reads FactorForth.  But the README
inside that folder, every documentation page, the design doc
for the upcoming object system, and the icon staged for the next
rebuild all say Factor4th.

The product name has changed.  The binary and path names will
catch up.

— end of rename entry
