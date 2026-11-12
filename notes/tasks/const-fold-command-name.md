title: "Const-fold a command name: allow `const x=/path; $x arg` via compile-time resolution"
status: done
priority: P3
kind: feature
depends: []
gates:
  - "`const x=/usr/bin/touch; $x foo` compiles — the const's literal value is folded into the command head and resolved (DD#23) at compile time"
  - "a NON-const `x=/usr/bin/touch; $x foo` stays rejected (SB5011) — a mutable var is a runtime-chosen command"
  - "the folded head still goes through DD#23 resolution (unresolved → SB5011) and manifest recording"
  - "0 native spec regressions; add specs for the const (accept) and non-const (reject) forms"

# Const-fold a command name

## Why

Today **any** variable/expansion used as a command name is a compile error
(SB5011, `check_cmd_in_stmt` in `sash/src/validate.rs`) — a command head
must be a literal path, because a runtime-chosen command can't be resolved
or manifested. That reject was added 2026-07-30 (it previously reached
`sash_exec` and crashed with `bug: bare command passed to sash_exec`).

But a `const` binding to a **literal path** *is* known at compile time:

```sash
const tool=/usr/bin/touch
$tool foo          # could be exactly `/usr/bin/touch foo`
```

DG (2026-07-30): allow this via compile-time const-folding, as a **future**
task — the immediate reject ships first (const is no exception yet).

## How (sketch)

When the command head is a bare `$x` expansion and `x` is a `const` bound
to a string literal, substitute the literal into the head before DD#23
resolution (so it resolves + records in the manifest exactly as a literal
path would). A non-const or non-literal-valued binding stays SB5011.

Care: only fold a **const** whose initializer is a pure string literal
(no interpolation / concat that could depend on runtime state). Keep the
folded value subject to the same resolution + taint rules as a written
literal path.

## Discarded (2026-08-21, DG)

DG: "discard the const-fold-command-name, that's gpp territory." A command name
computed from a `const` binding is a source-preprocessing / general-purpose-
preprocessor concern, not something sash's compiler should special-case. A
command head stays a literal path (SB5011 for any variable/expansion). Closed
wontfix.

## Closed 2026-09-10 (backlog prune)

Closed 2026-09-10 (backlog prune) — `wontfix` normalised to `done` so it stops
appearing in open-work listings. The decision stands.
