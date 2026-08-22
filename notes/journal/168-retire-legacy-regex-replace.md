# 168 — retire the legacy sed/`regex_replace` machinery (+ two masked bugs)

Task `retire-legacy-sed-regex-replace-machinery` (P3 cleanup). DD#63 had
re-routed both substitution sugars to the `$(sub)`/`$(gsub)` builtin path at
parse time, leaving the old consumers compiled-but-unreachable. This retires
them — and turned up two real bugs the machinery had been masking.

## What was removed

- `transform/stmt_handlers.rs::try_regex_substitution` (+ its call site). It
  consumed the old `Command{name:var, args:[Literal("s/…")]}` shape that the
  parser no longer emits.
- `codegen/value_handlers.rs`: the `"regex_replace"` match arm, the
  `"/" | "//" => emit_pattern_replace` arm (the string-op match is now just
  `"substring"` + an ICE fallthrough), and the now-orphaned
  `emit_regex_replace_op` / `emit_pattern_replace` / `intern_literal_value`.
- `codegen/setup.rs::intern_param_expansion`: the dead `regex_replace` and
  `/`|`//`|`/#`|`/%` interning guards. The scalar `/`-family remaps to
  `glob_replace` (→ SB4034) at parse, and `regex_replace` had no producer, so
  neither operator reaches codegen.

**Kept** (the live `$(sub)`/`$(gsub)` path, and the `${s:s/…}` / `s =~ s/…`
sugars that desugar onto it): `SsaValue::RegexReplace`,
`codegen/values.rs::lower_regex_replace_value`, the `regex_replace` extern, and
`sash-rt::sash_regex_replace`.

## Bug 1 — `try_regex_substitution` hijacked look-alike command args

`try_regex_substitution` fired for **any** command whose first arg matched
`s/…/…/[g]`, a holdover from when `var =~ s/…` lowered to a `Command`. So
`/bin/echo s/a/b/` was silently treated as a substitution and printed nothing.
With the function gone the command runs literally (`s/a/b/`). Pinned:
regex.sh 63.h1 (`s/a/b/`), 63.h2 (`s/x/y/g`).

## Bug 2 — `is_regex_mutation` SB5011 resolution bypass

The deeper find. `parse_regex_mutation` desugars `var =~ s/RE/SUB/[g]` to a
`StmtKind::Assignment` (`var = $(sub|gsub "$var" RE SUB)`) — it never produces
a `Command`. But two guards still looked for the old Command shape:

- `resolve.rs::resolve_command` — `if is_regex_mutation(c) { return; }` before
  DD#23 path resolution.
- `validate.rs::check_cmd_in_stmt` — the same early-return before the SB5011
  checks.

Because the shape they guard against can't occur anymore, the guards only ever
matched a **real** command that happened to take a lone `s/…`-shaped arg, and
wrongly exempted it from resolution. Consequence: `nosuchcmd_xyz s/a/b/`
compiled cleanly (no SB5011) and then aborted at runtime with
`sash: bug: bare command passed to sash_exec (expected absolute path)` — a
DD#23 resolution bypass surfacing the exact runtime `bug:` state DD#23 exists
to prevent at compile time.

Removed both guards and both `is_regex_mutation` fns. Now:
- `nosuchcmd_xyz s/a/b/` → SB5011 at compile time (regex.sh 63.h3),
- `head s/a/b/` still resolves to `/usr/bin/head`,
- `s =~ s/l/L/g` / `${s:s/l/L/g}` still work (`heLLo`).

## Verification

- `cargo build -p sash --release` clean (no orphaned refs/imports).
- Behaviors spot-checked: `${s:s/l/L/}`→heLlo, `${s:s/l/L/g}`→heLLo,
  `s =~ s/l/L/g`→heLLo, `$(sub "$s" l L)`→heLlo, `/bin/echo s/a/b/`→s/a/b/,
  `nosuchcmd_xyz s/a/b/`→SB5011.
- Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs HEAD
  spec.db: 0 passing→failing, 0 gone, 9 new passing (63.h1–h3 × 3 modes).

## Note

The `arrvals_/` per-element intern guard in `setup.rs` is also dead (the
`arrvals_<op>` family is SB4034 per DD#65/DD#33), but it belongs to the array
modifier drop, not the sed machinery — left in place to keep this cleanup
scoped. Flag for the DD#65 array-modifier cleanup if one is opened.
