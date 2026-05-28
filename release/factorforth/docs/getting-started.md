# Getting started with Factor4th

## Install

There is no installer.  Factor4th is a self-contained folder
you can run from anywhere:

1. Unzip the release archive (or use the `release/factor4th/`
   folder directly).
2. Double-click `factorforth-ui.exe`.

That's it.  No registry keys, no startup entries, no admin
rights required.

## Your first session

When the IDE opens you see one pane: the Forth console.  At the
`>` prompt, type a line and press Enter:

```
> 2 3 +
> .
5
```

The first line pushed 2 and 3 onto the stack and added them.
The second line printed the top of the stack.

Define a word:

```
> : square dup * ;
> 7 square .
49
```

The `:` starts a definition, `;` ends it.  Inside, `dup`
duplicates the top of stack and `*` multiplies the top two.
So `square` is "duplicate then multiply" — squaring whatever's
on top.

## State persists across lines

Factor4th's REPL behaves like a real Forth listener: values
you push stick around until you consume them.  This is the
fundamental Forth REPL rhythm.

```
> 5
> dup
> . .
5 5
```

Three separate evals.  Eval 1 pushes 5.  Eval 2 duplicates it
(stack now has two 5s).  Eval 3 prints both.  The data stack
isn't reset between lines.

Words you define stick around too:

```
> : double 2 * ;
> 21 double .
42
```

Variables:

```
> variable counter
> 0 counter !          \ initialise to 0
> 1 counter +!         \ increment
> 1 counter +!         \ increment
> counter @ .
2
```

## Multiple panes

The Tools menu opens:

- **Editor** — paste or load a `.f` file, hit F5 to evaluate
  the whole buffer at once.  Useful when you're building up
  more than a one-liner.
- **Data stack** — live view of the current stack.  Updates
  after each eval.
- **Log** — internal diagnostic messages (mostly empty during
  normal use).

The Demos menu loads bundled programs from `demos\`.  Try:

- `factorial.f` — recursion + iteration
- `fibonacci.f` — three styles of the same algorithm
- `stack-tour.f` — guided tour of the data stack
- `let-algebra.f` — infix math via Factor4th's LET DSL

## Documentation while you work

Help → Documentation opens `doc-crate.exe` against the bundled
`docs\` folder.  It's a self-contained markdown browser — no
internet required.  Use it as a quick reference while you work.

## What's next?

- New to Forth? Read [Forth tutorial](forth-tutorial.md).
- Coming from another Forth? See [language reference](language-reference.md)
  for what's the same and what's new.
- Curious how it works? [Architecture](architecture.md) walks
  through the compiler + VM split.
