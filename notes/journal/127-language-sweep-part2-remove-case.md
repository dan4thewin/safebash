---
title: Language sweep part 2 — remove `case`
date: 2026-07-04
---

# Language sweep part 2 — `case` removed (DD#66 destructive half)

DG: "continue — remove case first." The additive `[[ ]]` dispatch form
landed part 1 (5231dd1); this is the destructive half that deletes the
legacy `case…esac` construct now that dispatch covers it.

## What shipped (f71c04a)

Spec-first: migrated the specs, watched the reject go red→green, then
removed the construct.

- **Parser**: `case` is a reserved-word compile error (**SB4029**)
  directing to the dispatch form. `parse_case`/`parse_case_item` deleted;
  `parse_word` (its only caller) went with them. `Tok::SemiAmp`/
  `Tok::DblSemiAmp` (`;&`/`;;&`) removed from the hand lexer + the logos
  token set + `to_tok` + the lexer equivalence tests. `stmt_is_dispatch`
  simplified (every `Case` is a dispatch now).
- **IR**: `CaseItem.fallthrough`/`resume` dropped; `Case.op` is now only
  `==`/`=~`.
- **Lowering**: `wire_case_exits` collapsed to "each arm body → merge"
  (no fall-through/resume); `glob_test`→`dispatch_test` (dropped the
  `=glob` source-form branch); merge block `case.merge`→`dispatch.merge`,
  with **both** `is_catching_construct_exit` matchers (codegen/entry.rs
  and emit_c.rs) updated — this is the DD#55 `$?`-reset machinery, so
  missing one would silently break dispatch's catching-construct exit.
- **Backends**: `=glob`/`!=glob` removed from native codegen + emit_c;
  the `sash_fnmatch` extern dropped from both (setup.rs + the
  `ExternFuncs.fnmatch` field + emit_c's C prototype). emit_janet needed
  no functional change — it never had a real glob path (it treated glob
  as exact, which *was* `bug-janet-case-glob`); with `=glob` gone that
  misbehavior is now unreachable.
- **Runtime**: `sash_fnmatch` + `fnmatch_impl` deleted from sash-rt.
  **`sash_glob`/`sash_glob_regex` (filename globbing, `array f=(*.sh)`)
  kept** — that is a *separate* readdir-based function, not the string
  matcher. Verified `array f=(*.sh); for x in @f` still lists files.

## Spec migration

control_flow.sh cf.c1-c12 rewritten as dispatch. The interesting ones:
- multi-pattern `a|b|c)` → `=~ ^(a|b|c)$` (== has no alternation, DD#66).
- glob arms flip meaning deliberately: cf.c7 `?b` was *literal* under the
  *-only glob (asserted "star-only" won); as `=~ ^.b$` the `.` **is** a
  wildcard, so it now matches (`wildcard`). cf.c8 `[0-9]` likewise becomes
  a real regex class. Documented in the spec comments — this is the
  point of moving glob-matching to regex.
- Added cf.c13 (case → SB4029) and cf.c14 (case with `;&`/`;;&` still
  rejected — the reject fires before the arms are parsed). misc.sh 20.4
  (old extglob-in-case reject) repointed to the case-removal reject.

## Verification

Authoritative spec.db diff (latest-run-per-test, native/c/janet):
**0 regressions in all three backends.** native 595/595 passing. c
unchanged at 52 failing. janet **75→74** — cf.c6 flipped green because
the migrated regex path sidesteps the old janet glob-as-exact bug. That
closes `bug-janet-case-glob` (dissolved, as it predicted).

## Converter — split, not skipped

sash-convert didn't handle `case` at all (silent pass-through to an
SB4029 reject). It now emits a MANUAL dispatch-form annotation for
`case_statement`, matching the `subshell`/`select`/`coproc` convention.
The *full* mechanical `case`→dispatch auto-rewrite is a genuine design
call — the dispatch form uses **one** op for the whole construct, but
bash `case` mixes exact and glob patterns per-arm, and the patch-based
converter would conflict on nested arm-body conversions. That readability
tradeoff is the same one that shaped the janet backend, so it goes to DG:
filed `converter-case-to-dispatch` with the options laid out.

## Remaining sweep work

- **dd65 var modifiers** — the last sweep item (atomic drop+rebuild:
  toupper/tolower builtins, basename/dirname sugar, full `:-` word
  interpolation across the three backends). Not started.
