title: DD#65 — Prune the ${var:OP} modifier family
status: accepted

The `${var:OP}` family had accumulated bash creep. Vetted 2026-06-18
(task `vet-var-modifiers`). Kept only what earns its place; the rest is
dropped or moved into the awk-style string-builtin family
(`match`/`sub`/`gsub`/…, DD#63).

## Kept

- **`${var:-default}`** — default if unset/empty. Pure, useful, no glob.
  The default is **exactly a double-quoted-string body** (`:- ≡ "…"`, DG
  2026-07-04): `$name`/`${name}`/`$(…)`/`$((…))`/`$'…'`/`` `…` `` interpolate
  and concatenate; **every other byte is literal**, including whitespace
  (kept verbatim) and quotes (`'` and `"` are ordinary literal bytes — they
  do **not** strip or group). The terminator is the closing `}`: the first
  unnested literal `}` closes the expansion (the brace scanner is pure
  depth-matching — quotes no longer protect a `}`), so `${n:-'a}b'}` is
  `${n:-'a}` (default `'a`) followed by literal `b'}`. To embed a literal
  `}` (or any byte) use a `$'…'` C-string, e.g. `${n:-$'a\x7db'}` → `a}b`
  (the `\x7d` carries no `}` byte, so it never trips the matcher). Taint
  flows normally: `${n:-$clean}` clean, `${n:-$tainted}` tainted (bless
  first); a bare env var is rejected (SB5018 — it is a real interpolation
  context, so use `$(getenv HOME)`); `$(external)` is tainted, `$(user_fn)`
  and `$((…))` are clean.
- **`${var:off:len}`** — substring. Useful, no glob/regex.
- **`${var:s/re/sub/[g]}`** — regex replace; per DD#63 this is sugar for
  `$(sub|gsub var "re" "sub")`.

(Implemented 2026-07-04: `${x:-$y}` now interpolates via the `:- ≡ "…"`
model above — a `DoubleQuoted` operand lowered to `SsaValue::DefaultValue`
across native/c/janet. The `:s/` operand interpolation is DD#63's
textual-first-pass; `:off:len` takes integer operands.)

## Dropped (reject with a diagnostic pointing at the replacement)

- **`:=` `:+` `:?`** — broken today and each redundant: `:=` is
  `var=${var:-default}` (and mutates inside a read — a side-effect wart);
  `:+` is `[[ -n $var ]] && …`; `:?` is murky under sash's no-unset model
  (undeclared already reads empty), collapsing to a plain emptiness test.
- **`#` `##` `%` `%%`** (prefix/suffix strip) — real ergonomic value, but
  the community reaches for **`basename`/`dirname`**, not the strip ops,
  so they don't earn the muscle-memory exception. Use `basename`/`dirname`
  (see open question) or `sub`/`gsub`.
- **`/` `//` `/#` `/%`** (glob replace) — redundant with `sub`/`gsub`,
  glob holdover (DD#62), and the only var-op that used `\`
  (`find_unescaped_slash`). Dropping it makes DD#64 fully clean for the
  family.
- **`^` `,`** (first-char case) — niche bash curio.

## Implemented — the drops reject SB4034 (2026-07-04)

All dropped operators now reject at parse with **SB4034**
(`is_dropped_param_operator` + `check_rejected_param_expansion`,
`sash/src/parse/parser.rs`). One uniform code, one message pointing at the
string-builtin families generically (DG 2026-07-04): *"…left the safe
subset — use the string builtins (toupper/tolower, sub/gsub,
basename/dirname) or the kept forms :- / :off:len / :s/"*. Per-op
migration is **sash-convert's** job, not the compiler's (task
`converter-var-modifier-migration`).

Dropped set = every `split_operator` op except `:-`: `:=` `:+` `:?` `#`
`##` `%` `%%` `^` `^^` `,` `,,`, plus the `/`-family glob-replace. The
`/`-family is remapped to an internal `glob_replace` operator at parse
(`parse_standard_operator`) so it is **distinguishable from the kept `:s/`
sed op**, which shares the raw `/`//` operator strings — SB4034 targets
`glob_replace`, leaving `:s/` (`/`//`) untouched. `split_operator` still
*recognises* the dropped ops (so they reject explicitly rather than
silently degrading `${x#a}` → `$x`). Spec: `spec/var_modifiers.sh`
vm.dr1-15 (one per operator string), plus migrated `misc.sh` 52.6a/b and
`wordsplit.sh` 39.4.

## Moved to builtins

- **`^^` `,,`** (full upper/lower) → **`$(toupper s)` / `$(tolower s)`**
  builtins (awk names, consistent with `match`/`sub`/`gsub`). These also
  give case-insensitive matching, which the regex engine can't (no `i`
  flag): `match $(tolower $x) $(tolower $pat)`. `/usr/bin/tr` remains for
  arbitrary character translation.

## Resolved — basename/dirname (2026-06-18 → **revised 2026-06-21/07-04**)

- **basename/dirname are SUGAR** (DG 2026-06-21, reconfirmed 2026-07-04):
  the compiler rewrites `basename $p` → `$(sub $p '.*/')` and `dirname $p`
  → `$(sub $p '/[^/]*$')` (or equivalent) at compile time — **no fork, no
  taint, no DD#23 dependency**. This supersedes the earlier 2026-06-18 note
  below (no builtins / use external), which is retained only for history.
  ~~No `basename`/`dirname` builtins. Use `${var:s/…/…/}` / `sub`, or the
  external `basename`/`dirname` (DG: fork cost is fine).~~ **Caveat (moot
  under sugar):** DD#23 path-resolution is `partial`; the external route hit
  the `bare command` runtime bug + taint — the sugar rewrite sidesteps both.

## Resolved (DG, 2026-06-21)

- **`${#var}` length: KEPT.** Orthogonal to the pruned `:OP` family, O(1),
  essential. **`@X` transforms** (`${var@U}`/`@Q`/…): **stay rejected**
  (SB4015), unchanged.
- **`${var:-default}` allows `$(cmd)`.** The earlier "command substitution
  is rejected in the default" was an over-restriction. Only an **external**
  capture is tainted (`$(getenv|argv|arg …)` or `$(/abs …)` —
  `cmdsub_is_taint_source`, validate.rs:1925); a **user-function** capture
  `$(my_fn)` is **clean** and useful. So `$(cmd)` is allowed; taint does
  its normal job (clean user-fn works; external needs bless, as
  everywhere). Arithmetic `$((…))` was already allowed.
- **`:?` is rejected for SCALAR `${var:?…}` only.** The `:?` *empty-
  disposition* is reintroduced for the **array-splat** form `@{a:?WORD}`
  (DD#69) — a different construct. DD#65 drops only the scalar `${var:?}`.
- **The default is an interpolating context** — see the 2026-07-04
  resolution below, which settled its exact shape.

## Resolved (DG, 2026-07-04) — the `:- ≡ "…"` model

The 2026-06-21 note framed the default as "an interpolating **word**
context … like any command argument / assignment RHS, `'…'` literal" —
i.e. the bash **word** model where single-quotes *strip*. DG replaced this
with a simpler, fully-consistent rule: **the default is exactly a
double-quoted-string body** (`${var:-X}` ≡ `"X"`).

- **Quotes are ordinary literal bytes.** `'` and `"` do not strip or group
  (`${x:-'a'}` → `'a'`). This collapses the rule to one sentence and
  preserves whitespace verbatim (`${x:-a  b}` → `a  b`).
- **The C-string is the any-byte hatch.** Because `$'…'` decodes
  `\xNN`/octal/`\n` and is recognised inside `"…"` (DD#64 addendum), any
  byte is expressible: `${x:-$'a\x7db'}` → `a}b`. This is *why* stripping
  single-quotes is unnecessary — its only unique job (embedding a `}`) is
  already covered, without a literal `}` byte to confuse the parser.
- **Consequence — quotes are plain in the brace matcher too.** For `'` to
  be genuinely plain, the lexer's `${…}` brace matcher (both the bare
  `scan_brace_expansion` and the in-`"…"` `consume_dq_brace_expansion`)
  drops its quote/`\`-skip: the extent of `${…}` is pure brace-depth, so
  the first unnested literal `}` closes. `${n:-'a}b'}` = `${n:-'a}` +
  literal `b'}`. (Only affects an expansion with a literal `}` *inside*
  quotes; `${m["key"]}` and friends are unchanged, having no interior `}`.)
- **Divergence from bash** (accepted): bash strips quotes in `:-` and
  quote-protects a `}`; sash does neither. The C-string replaces both.

Lowering: the operand parses to an `ExprKind::DoubleQuoted` and lowers to
`SsaValue::DefaultValue { var, default }` (native + c select var-or-default
over the interpolated default; janet interpolates it at the IR level).

## See also
- DD#63 — regex/string builtin family (`match`/`sub`/`gsub`, +`toupper`/`tolower`).
- DD#64 — no shell backslash (the `/`-op `\/` escape goes away with it).
- DD#62 — drop glob (the `/` glob-replace is a holdover).
- DD#61 — only *restricts* `#`/`%` for taint; does not drop them (this DD does).
