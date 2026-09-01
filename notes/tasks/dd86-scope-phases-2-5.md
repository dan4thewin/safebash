---
title: "DD#86 scope model — phases 2–5 (drop local, structural scope, no-shadow, dead-var, converter)"
status: ready
priority: P1
kind: feature
depends: []
gates:
  - "Phase 2: `local` keyword removed (lexer/parser/DeclKeyword); scope is structural — a top-level declaration is a global, a new name assigned in a function is auto-local; globals are read AND written frictionlessly from functions"
  - "Phase 3: no-shadowing — a typed re-declaration inside a function of a name that is a top-level global is an error (redeclaration); a bare assignment writes the global"
  - "Phase 4: dead-variable rejection — a variable assigned but never read is a hard error (dual of DD#62 SB5031); const included; NO warnings"
  - "Phase 5: `local fn` (DD#18) abandoned; converter maps bash `local` to the keyword-free form; all specs/tests using `local` rewritten; GUIDE/SUBSET updated; DD#35 amended (lifetime axis retired)"
---

# DD#86 phases 2–5

Phase 1 (the `str` keyword) shipped. The design is fully ratified in
`notes/decisions/DD86-variable-scope.md`. This task is the rest.

## Phase 2 — drop `local`, structural scope

- Remove `Tok::Local`, `parse_local`, `DeclKeyword::Local`, and the
  save/restore machinery (`collect_local_vars`, `save_caller_locals`,
  `restore_saved_locals`, `save_locals`/`restore_locals`).
- Scope is **positional**: a variable declared at top level is a **global**; a
  name first assigned inside a function that is not a top-level global is a
  **function-local** (automatic). Globals are freely read AND written from
  functions.
- Implementation hinge: when compiling a function body, resolve each assigned
  name against the set of top-level globals (global-write vs new-local). The
  transform inlines functions and tracks versions (`VersionState`), so the
  global set is known; the current SB5031 typo check already treats
  function-only names as non-global, so this largely formalizes existing static
  behavior — the runtime change is that a bare assignment no longer escapes to a
  global by dynamic scope.

## Phase 3 — no-shadowing / redeclaration error

- A typed declaration (`int`/`str`/`array`/`map`/`const`) inside a function of a
  name that is already a top-level global → **error** (new SB code). A bare
  assignment to that name writes the global. There is no way to create a local
  that shadows a global.

## Phase 4 — dead-variable rejection (dual of DD#62)

- New validate pass: every declared/assigned variable must have a reachable
  read; else a **hard error** (no warnings — DG: warnings signal ambiguity, and
  there is none). `const` is not exempt. This is the safety net that catches a
  misspelled write target (a stray local never read).

## Phase 5 — cleanup / migration

- Abandon DD#18 `local fn` (re-spell as a block-scoped `fn`, or drop).
- Converter: bash `local x` → drop the keyword; a bash local that *shadowed* a
  global must be renamed (flag for review).
- Rewrite every spec/test using `local`.
- Amend DD#35 (lifetime axis retired); update GUIDE/SUBSET.

## Watch-items

- Whole-program dead-variable analysis must handle globals written in one
  function and read in another (the method pattern) — a global read *anywhere*
  counts as used.
- Phases 2/4 will churn many specs (every `local`; any assigned-but-unread var);
  land each phase with its spec updates so the native gate stays green.
