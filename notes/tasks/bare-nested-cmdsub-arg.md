---
title: "bare nested $(…) as a command argument fails to parse (SB4014)"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`p=$(dirname $(getcwd))` parses (bare nested command substitution as an arg)"
  - "`$(arg 1 $(getcwd))` parses (currently only the quoted `\"$(getcwd)\"` form works)"
  - "no regression to the quoted-nested form"
---

## Symptom

A **bare** (unquoted) nested command substitution used as a command argument is
a parse error:

    p=$(dirname $(getcwd))        # SB4014 expected expression
    x=$(arg 1 $(getcwd))          # SB4014

The **quoted** form works:

    x=$(arg 1 "$(getcwd)")        # OK
    p="$(dirname "$(getcwd)")"    # OK (quoted)

bash accepts the bare form. sash's command-arg parser doesn't recurse into a
nested `$(…)` when unquoted.

## Impact

Minor / ergonomic — the quoted form is a clean workaround and is arguably better
style. Surfaced via the DD#60 `$(arg N DEFAULT)` feature (journal 148): an
intrinsic default like `$(getcwd)` must be quoted (`$(arg 1 "$(getcwd)")`).

## Provenance

Found during DD#60 arg-default work. General limitation (not arg-specific).
