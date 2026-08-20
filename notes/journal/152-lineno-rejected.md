---
title: "$LINENO rejected — no magic vars"
date: 2026-08-19
---

# $LINENO: reject, no special handling

DG resolved the long-open `decide-lineno-endorsement`: **"reject — no magic
vars; document how to do it with a preprocessor. $LINENO shouldn't need
special handling."** (I had leaned endorse; DG chose purity.)

## What changed

`$LINENO` was a bash carryover: `parse_dollar_arm` lowered it to a literal
source line number (`ExprKind::Literal("N")`), and `handled_elsewhere`
exempted it from the DD#62 typo check as "compiler-managed." Both special
cases are gone. `$LINENO` now flows through as an ordinary variable
reference, so an unassigned `$LINENO` — and the braced `${LINENO}` — is
rejected as an **undefined variable** (SB5031), exactly like any other
unknown name. No `REJECTED_VARS` entry, no bespoke message, no magic.

The first cut added a dedicated `REJECTED_VARS` entry (SB5017) with a
custom message; DG corrected it mid-implementation ("shouldn't need special
handling"). The generic undefined-variable path is both simpler and more
complete — it catches `${LINENO}` for free (the typo check recurses into
bare `${…}` reads, unlike the `SimpleExpansion`-only `validate_var_name`).

Line numbers, if genuinely needed, come from a source preprocessor
(`awk`/`sed`/`m4`) that substitutes them before compilation — documented in
`docs/SUBSET.md` (Rejected Constructs).

## Decoupled bug found

Chasing the braced form surfaced a **pre-existing** silent bug: `${RANDOM}`
/ `${BASH_VERSION}` / `${HOME}` (braced `REJECTED_VARS`/`ENV_VARS` names)
bypass `validate_var_name` (only `SimpleExpansion` is checked) and silently
compile to empty. Not LINENO-specific (LINENO is caught by the typo check
now). Filed `bug-braced-rejected-var-bypass` (P2); not fixed here to keep
this change to the LINENO decision.

## Verification

`docs/SUBSET.md` + `docs/REVIEW.md` + DD#24 updated. spec `undeclared.sh`
ud.17 (`$LINENO`) + ud.18 (`${LINENO}`) → SB5031, plan 14→18 (also
corrected a pre-existing off-by-2 in the plan). 3-mode sweep 0 regressions.

## Decision-tree note

This was a decided fork (DG picked reject), so implementation was mine to
carry — but DG's "shouldn't need special handling" was a mid-course
correction that the tree's step 1 covers: apply the *principle* ("no magic
vars"), not a literal reading that would re-magic it via a bespoke reject.
The generic path is the faithful application.

## Update 2026-08-19: braced-rejected-var bypass fixed

`bug-braced-rejected-var-bypass` (filed above) is now fixed:
`validate_expr` validates `${name…}` (any operator/index) via
`validate_var_name`, identical to `$name`. Since `validate_var_name` only
flags names in the rejected/env/positional lists, a normal `${x}` / `${x:-9}`
/ `${a[i]:-N}` stays clean, while `${RANDOM}` → SB5017 and `${HOME}` /
`${HOME:-x}` / `${#HOME}` → SB5018. spec var_modifiers.sh bvb.1-5; 3-mode
sweep 0 regressions, +15.
