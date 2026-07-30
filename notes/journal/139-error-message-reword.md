# 139 — error-message reword: intent over "not supported"

DG error-message review. A pass over every diagnostic to make the wording
carry *intent*, not a vague limitation.

## Principles (DG)

1. Drop the "safe subset" phrasing — unhelpful in a user-facing error.
2. Distinguish *why* a construct is rejected:
   - **unsafe** (conflicts with sash's safety tenets) → say so, with the
     reason (defeats static analysis / runtime code).
   - **omitted by design** (dropped for legibility / ambiguity) → "sash has
     no X" / "isn't part of sash", with the native form.
3. "not supported" is too weak for intentional omissions — reserve it for
   things sash could plausibly add later. (None of the reworded ones
   qualified; heredocs are a permanent choice — "we have something just as
   good": a herestring with a multi-line quoted string.)
4. No `DD#n` references in user-facing messages (internal only).

## Changed

- **Unsafe → reason:** `eval` ("runs code assembled at runtime, which can't
  be checked statically"), `source`/`.` ("loads code at runtime" — kept
  minimal, no prescriptive fix per DG), `${!var}` indirect ("a runtime-
  chosen variable name can't be checked statically"), the `check_dynexec`
  net ("sash forbids dynamic command execution").
- **Omitted → "sash has no X" / native form:** `coproc`, `${var@…}`
  (→ toupper/tolower builtins), `${…@…}` operator (dropped "kept";
  → string builtins / `:-` / `:off:len` / `:s/`), compound-command postfix
  ×4 (SB4021-24, "isn't allowed — wrap the compound in a function"),
  `&&`/`||` on a pipeline, `select`, `declare`/`typeset`/`readonly`
  (→ `int`/`const`/`array`/`map`/`const`), `export` (→ `/usr/bin/env
  NAME=value …`), `declare -x` (→ `/usr/bin/env`), non-`*` globs
  (SB5006, "sash has only `*` globs — `?`/`[…]`/`**` omitted"),
  `[[ -v ]]` (→ `-n`), nested backticks ("ambiguous — use `$(…)`").
- **Heredocs (SB5002):** "sash has no heredocs — use a herestring (`<<<`)
  with a multi-line \"…\" string" (permanent, not a future gap).
- **DD# stripped** from SB4029, SB5025, SB5026, SB5027, SB5030, and the two
  SB5020 bless-all forms (comments keep their DD# refs).

Codes unchanged; specs assert codes, not text → native+c+janet 0
regressions. Tidied one stale spec comment (var_modifiers.sh). Left the
specific/parse/model messages alone (SB4014, SB5007 arith rules, SB5009
"eliminated by design", taint/splat, numeric-literal, etc.).
