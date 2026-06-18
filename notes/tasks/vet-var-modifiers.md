---
title: "vet the ${var:OP} modifier family — keep / drop / rework each"
status: ready
priority: P2
kind: design
depends: [DD64-no-shell-backslash, DD63-regex-quoting-interpolation]
gates:
  - "each ${var:OP} operator has a recorded keep/drop/rework decision (a DD or addendum)"
  - "for each kept operator: backslash handling under DD#64, operand interpolation, and brace-} behaviour are specified"
  - "dropped operators are rejected with a diagnostic pointing at the replacement"
---

# vet the `${var:OP}` modifier family

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
- Operand interpolation: implement textual-first-pass (`$name`/`${name}`)
  or leave literal? (Currently literal — a gap.)
- Brace `}`: unbalanced `}` in the operand closes early; `\x7d` / variable
  / function form as the escape (no `\}` under DD#64).

For each DROP: reject with a diagnostic naming the replacement
(`/`-replace → `${v:s/…}` or `sub`/`gsub`; strip/case → builtin or regex).

## Notes

- The `/` glob-replace is the strongest drop candidate: it duplicates the
  regex replace, is glob-based (DD#62 drops glob), and carries the only
  var-op backslash escape (`find_unescaped_slash`). Dropping it makes
  DD#64 fully clean for the family.
- Prefix/suffix/case ops: decide whether they earn their keep vs. a small
  builtin set or the regex family.
