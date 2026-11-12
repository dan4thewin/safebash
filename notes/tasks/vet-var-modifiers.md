title: "vet the ${var:OP} modifier family — keep / drop / rework each"
status: done
priority: P2
kind: design
depends: [DD64-no-shell-backslash, DD63-regex-quoting-interpolation]
gates:
  - "each ${var:OP} operator has a recorded keep/drop/rework decision (a DD or addendum)"
  - "for each kept operator: backslash handling under DD#64, operand interpolation, and brace-} behaviour are specified"
  - "dropped operators are rejected with a diagnostic pointing at the replacement"
  - "TDD: spec tests below (V1-) written first and red before code; :- is a word (interpolation+concat+quoting), unquoted } closes, quoted } is literal"

# vet the `${var:OP}` modifier family

> **DONE 2026-07-04.** All stages shipped: toupper/tolower builtins
> (896a424), basename/dirname sugar (bdcac8f), the `:-` word on the
> `:- ≡ "…"` model (1788ee0), the drops → SB4034 uniform reject (7b6974f),
> and the dead-backend-dispatch removal (39519e5). Replacement/migration
> moved to sash-convert (task `converter-var-modifier-migration`). See
> DD#65 and journal 129. Gates below all satisfied.

> **Verdict 2026-06-18 → DD#65.** Keep `:-`, `:off:len`, `:s/`. Drop
> `:=` `:+` `:?`, the `#`/`##`/`%`/`%%` strip ops (use `basename`/`dirname`),
> the `/`-family glob-replace (use `sub`/`gsub`), and `^`/`,` first-char
> case. Move `^^`/`,,` to `toupper`/`tolower` builtins. Open:
> `basename`/`dirname` as builtins vs external. The inventory below is
> retained as reference / for the dropping-implementation.

Several of these look like unintentional bash creep. Go through each,
decide keep / drop / rework, and (for kept ones) pin down operand
interpolation, brace-`}`, and DD#64 backslash behaviour. No DD currently
*drops* any of them; DD#61 only *restricts* `#`/`##`/`%`/`%%` for taint
(SB5021). Operand interpolation is currently **unimplemented** for all of
them (`${x:-$y}` → literal `$y`, verified).

## Inventory (current behaviour)

`split_operator` recognises: `:-` `:=` `:+` `:?` `##` `%%` `//` `/#` `/%`
`^^` `,,` `%` `#` `/` `^` `,`; plus `:off:len` (substring), `:s/re/sub/`
(sed), `@X` (transform).

| op | meaning | today | first take |
|----|---------|-------|-----------|
| `:-` | default if unset/empty | works | keep? (param defaults) |
| `:=` | assign-default | misses.md: fails to compile | fix or drop |
| `:+` | alt if set | misses.md: does nothing | fix or drop |
| `:?` | error if unset | misses.md: fails to compile | fix or drop |
| `#` `##` | strip prefix (glob) | works (`${x#abc}`) | **creep?** glob-based; regex/`sub` could cover |
| `%` `%%` | strip suffix (glob) | works | **creep?** as above |
| `^` `^^` | upper first/all | bash | **creep?** could be a builtin |
| `,` `,,` | lower first/all | bash | **creep?** could be a builtin |
| `/` `//` `/#` `/%` | glob replace | works; `${v/b/X}` == `${v:s/b/X/}` | **drop** — redundant with `sub`/`gsub`, DD#62 glob holdover, only var-op using `\` (`find_unescaped_slash`) |
| `:off:len` | substring | works | keep? (useful, no glob/regex) |
| `:s/re/sub/[g]` | regex replace | DD#63 | rework → sugar for `$(sub|gsub …)` |
| `@X` | transform (`${v@U}`) | SB4015 reject | already rejected — confirm |

Also confirm the status of `${#var}` (length), `${!var}` (indirect —
rejected), and array forms (`${a[@]}`, `${!a[@]}`) — out of this vet's
core but note interactions.

## Per-operator questions

For each KEEP:
- DD#64 backslash: operands are verbatim `\` (no `find_unescaped_slash`);
  escaping via quoting + concat; `\xNN` for literal bytes.
- Operand interpolation: `:-` is a **word** — interpolation + concat +
  quoting (DD#65), implemented as part of the `:-` rework. `:s/` fields
  interpolate (DD#63). `:off:len` operands are integers.
- Brace `}`: handled per-operator, **not** by `\}` (DD#64 removes it).
  `:-` includes a literal `}` by **quoting** (`${foo:-'a}b'}` → `a}b`);
  `:s/` protects `{`/`}` via **Path B** (`/`-delimited payload, DD#63);
  `:off:len` takes integers. `\x7d` remains a byte-escape but is required
  by neither.

For each DROP: reject with a diagnostic naming the replacement
(`/`-replace → `${v:s/…}` or `sub`/`gsub`; strip/case → builtin or regex).

## Spec tests to write first

Spec-first (TDD). Kept-op behaviour from DD#65; `:s/` brace handling lives
in `regex-interpolation-delimiters` section G (don't duplicate it here).

### `:-` default is a word (DD#65)

- **V1** plain literal: `echo "${foo:-abc}"` → `abc`.
- **V2** single-quote strips the quotes and protects `}`:
  `echo "${foo:-'a}b'}"` → `a}b` (today: literal `'a}b'`). Canonical
  regression (the original report):
  `a=${foo:-'a}b'}; b='a}b'; [[ $a == $b ]] && echo yes` → `yes`.
- **V3** interpolation: `bar=hi; echo "${foo:-$bar}"` → `hi`
  (today: literal `$bar`).
- **V4** concatenation: `x=a; y=b; echo "${foo:-$x$y}"` → `ab`.
- **V5** unquoted `}` closes; trailing text concatenates:
  `echo "${foo:-a}b"` → `ab`.
- **V6** command-sub rejected in a default: `echo "${foo:-$(echo x)}"` →
  compile error (external capture is always tainted → would always need
  bless → useless; DD#65).
- **V7** set var ignores the default: `foo=hi; echo "${foo:-x}"` → `hi`.

### Drops reject with a diagnostic (DD#65, exact codes TBD)

- **V8** `:=` rejected: `echo "${foo:=x}"` → `rc != 0`, err names `:-`.
- **V9** prefix/suffix strip rejected: `x=abc; echo "${x#a}"` → `rc != 0`,
  err names `basename`/`dirname` or `${x:s/…}`.
- **V10** `/`-glob-replace rejected: `x=abc; echo "${x/a/Z}"` → `rc != 0`,
  err names `${x:s/…}` / `sub`.
- **V11** first-char case rejected: `x=abc; echo "${x^}"` → `rc != 0`, err
  names `toupper`/`tolower`.

## Notes

- The `/` glob-replace is the strongest drop candidate: it duplicates the
  regex replace, is glob-based (DD#62 drops glob), and carries the only
  var-op backslash escape (`find_unescaped_slash`). Dropping it makes
  DD#64 fully clean for the family.
- Prefix/suffix/case ops: decide whether they earn their keep vs. a small
  builtin set or the regex family.

## Resolved (DG, 2026-06-21)

- **`basename`/`dirname` are SUGAR**, not builtins: `basename $p` ≡
  `$(sub $p '.*/')`, `dirname $p` ≡ `$(sub $p '/[^/]*$')` (or equivalent).
  Avoids builtin creep — they reduce to the existing `sub` primitive.
- **`toupper`/`tolower` ARE builtins** — the one exception: case conversion
  is not expressible as a regex `sub` replacement (DD#63 forbids `\U`/`\L`),
  so they're irreducible primitives.
- **Sequencing:** drop the modifiers AND build the replacements in the
  **same commit**, with **no transitional diagnostics** (no "use X
  instead" phase). Kept: `:-`, `:off:len`, `:s/`; the `:-` is now a *full
  word* (Decision 1) so its word-interpolation is built here too (shared
  with `@{a:-WORD}`). Dropped: `:=`/`:+`/`:?`, `#`/`##`/`%`/`%%` (→
  basename/dirname sugar), `/`-family (→ `sub`/`gsub`), `^`/`,` (first-char
  case, no replacement), `^^`/`,,` (→ toupper/tolower builtins).
- **DEPENDS ON `regex-builtins`** — `sub`/`gsub` must exist for the
  basename/dirname sugar and the `/`-family replacement. regex-builtins
  lands first or in the same commit.
