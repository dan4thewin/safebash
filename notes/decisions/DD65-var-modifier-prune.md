title: DD#65 — Prune the ${var:OP} modifier family
status: accepted

The `${var:OP}` family had accumulated bash creep. Vetted 2026-06-18
(task `vet-var-modifiers`). Kept only what earns its place; the rest is
dropped or moved into the awk-style string-builtin family
(`match`/`sub`/`gsub`/…, DD#63).

## Kept

- **`${var:-default}`** — default if unset/empty. Pure, useful, no glob.
  The default is a **normal word/value** (filling the current
  literal-only gap): `$name`/`${name}` interpolation, concatenation
  (`$foo$bar`, `${foo}bar`), and quoting (`'$bar'` = literal). It is *not*
  a restricted grammar — it behaves like any command argument or
  assignment RHS. Taint flows: `${n:-$clean}` is clean, `${n:-$tainted}`
  is tainted (bless before use). An **unquoted `}` ends the expansion**
  (DD#64: single-quote to include one — `${n:-'a}b'}`; no `\}`).
  **Command substitution is rejected** in the default — an external
  capture is always tainted, so `${n:-$(cmd)}` would always need bless,
  making it useless as a convenience; use a variable/literal. (Arithmetic
  `$((…))` is clean and allowed.)
- **`${var:off:len}`** — substring. Useful, no glob/regex.
- **`${var:s/re/sub/[g]}`** — regex replace; per DD#63 this is sugar for
  `$(sub|gsub var "re" "sub")`.

(Operand interpolation is currently unimplemented — `${x:-$y}` yields
literal `$y` today; the `:-` rule above (default = word) fixes it. The
`:s/` operand interpolation is DD#63's textual-first-pass; `:off:len`
takes integer operands. Implement when these are touched.)

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
- **The default is an interpolating word context.** `${var:-default}`
  behaves like any command argument / assignment RHS: `$var`/`${var}`
  interpolate, `'…'` literal, `"…"` interpolating, **`$'…'` C-string
  works** (DD#64 addendum), `$(…)` per above — all concatenating. It is
  *not* a literal-double-quoted wrapper. (Still unimplemented today; this
  is the target when operand interpolation lands.) This is the **same**
  context as the `sub`/`gsub` replacement (DD#63), which differs only by
  its extra `\0`–`\9` backref pass.

## See also
- DD#63 — regex/string builtin family (`match`/`sub`/`gsub`, +`toupper`/`tolower`).
- DD#64 — no shell backslash (the `/`-op `\/` escape goes away with it).
- DD#62 — drop glob (the `/` glob-replace is a holdover).
- DD#61 — only *restricts* `#`/`%` for taint; does not drop them (this DD does).
