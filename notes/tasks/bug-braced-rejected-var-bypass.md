---
title: "bug: braced ${RANDOM}/${BASH_VERSION}/env-var forms bypass SB5017/SB5018 (silent empty)"
status: done
priority: P2
kind: bug
depends: []
resolved: "2026-08-19 — validate_expr now validates ${name…} (any operator/index) like $name via validate_var_name (which only flags rejected/env/positional names, so normal ${x}/${x:-9}/${a[i]:-N} stay clean). Covers bare + operator + length forms. spec var_modifiers.sh bvb.1-5."
gates:
  - "`echo ${RANDOM}` rejects (SB5017) exactly like `echo $RANDOM` — never silently compiles to empty"
  - "`echo ${HOME}` rejects (SB5018) exactly like `echo $HOME`"
  - "no regression on legitimate braced reads (`${x}` for a declared x) or disposition forms"
---

# Braced rejected/env vars silently compile to empty

## Symptom

```
$ sash -c 'echo $RANDOM'    # → SB5017 (correct)
$ sash -c 'echo ${RANDOM}'  # → compiles, prints empty (WRONG — silent)
$ sash -c 'echo $HOME'      # → SB5018 (correct)
$ sash -c 'echo ${HOME}'    # → compiles, prints empty (WRONG — silent)
```

The **braced** form of a `REJECTED_VARS` / `ENV_VARS` name is not
validated, so it silently produces an empty string instead of the
SB5017/SB5018 diagnostic the unbraced form gets.

## Root cause

`validate.rs::validate_expr` calls `validate_var_name` only for
`ExprKind::SimpleExpansion(var)` (the `$name` form). The braced `${name}`
parses to a bare `ExprKind::ParameterExpansion { name, operator: None,
index: None }`, which hits the `_ => {}` arm — no validation.

`$LINENO` is NOT affected (it was de-magicked 2026-08-19 and is caught by
the DD#62 typo check, which *does* recurse into bare ParameterExpansion —
see `handled_elsewhere`). Only names in `REJECTED_VARS`/`ENV_VARS` slip
through, because `handled_elsewhere` exempts them from the typo check
(expecting `validate_var_name` to catch them) while `validate_var_name` is
never reached for the braced form.

## Fix direction

In `validate_expr`, also call `validate_var_name(&pe.name, …)` for a bare
`ParameterExpansion` (`operator.is_none() && index.is_none()`) so the
braced read is validated identically to `SimpleExpansion`. Decide whether
operator forms (`${RANDOM:-5}`, `${HOME#x}`) should also reject the name —
a rejected/ambient var referenced under any operator is still a reference
(likely yes, but check for false positives on legitimate `${x:-…}` where x
is a normal declared var — those must stay clean).

## Provenance

Found 2026-08-19 while implementing the `$LINENO` rejection
(`decide-lineno-endorsement`). Decoupled from LINENO once LINENO stopped
using `REJECTED_VARS`. Pre-existing (reproduces at HEAD).
