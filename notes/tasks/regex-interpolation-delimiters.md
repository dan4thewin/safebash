---
title: "regex field interpolation + delimiter quoting (DD#63)"
status: ready
priority: P2
kind: feature
depends: [DD63-regex-quoting-interpolation]
gates:
  - "TDD: spec tests below written first and red before code"
  - "$name and ${name} interpolate in pattern AND replacement fields of all three surfaces (=~, ${var:s///}, $v =~ s///)"
  - "'…' field disables interpolation (fully literal)"
  - "delimiter switching works (s\"a/b\"c\" puts a literal / in a field); no \\<delim> escape"
  - "$ before a non-name char stays a literal regex anchor"
  - "native spec clean (only cf.c7/cf.c8); rust suite no net regression; complexity gate green"
---

# regex field interpolation + delimiter quoting (DD#63)

Implements DD#63. **Field interpolation is currently unimplemented on
every regex surface** (verified 2026-06-17) — `$var` in a pattern or
replacement is taken literally. This task wires up interpolation as a
textual first pass and the quote/`/` delimiter model.

## Misses (spec tests to write first)

Place in `spec/types.sh` / a regex spec area; bump `plan=`. Use `if/else`
(not `&& ||`) for boolean assertions — the `&& X || Y` chain after a
false test is itself broken (see `dd62-test-expr-ssavalue` F1).

### A. Pattern interpolation (today: literal `$pat`)

- **A1** `pat='a.*b'; x=axxb; if [[ $x =~ $pat ]]; then echo m; fi` → `m`.
- **A2** braced: `pat='a.*b'; x=axxb; if [[ $x =~ ${pat} ]]; then echo m; fi` → `m`
  (today **SB4014** — `${…}` in `=~` doesn't parse).
- **A3** mixed: `suf=end; x=aXXend; if [[ $x =~ a.*$suf ]]; then echo m; fi` → `m`.
- **A4** `:s/` pattern: `x=b; v=abc; echo ${v:s/$x/Y/}` → `aYc` (today `abc`).
- **A5** mutation pattern: `x=b; v=abc; v =~ s/$x/Y/; echo $v` → `aYc`
  (today `sash: bug:` runtime abort).

### B. Replacement interpolation (today: literal `$r`)

- **B1** `r=Y; v=abc; echo ${v:s/b/$r/}` → `aYc` (today `a$rc`).
- **B2** braced replacement: `r=Y; v=abc; echo ${v:s/b/${r}/}` → `aYc`.

### C. `'…'` disables interpolation (literal field)

- **C1** `pat=zzz; x='$pat'; if [[ $x =~ '$pat' ]]; then echo m; fi` → `m`
  (matches the literal text `$pat`, not the value of `pat`).
- **C2** `v='a$xb'; echo ${v:s'$x'Y'}` → replaces literal `$x` → `aYb`.

### D. Anchors survive interpolation

- **D1** `x=foo; if [[ $x =~ o$ ]]; then echo m; fi` → `m` (trailing `$` anchor).
- **D2** `x=foo; if [[ $x =~ ^foo$ ]]; then echo m; fi` → `m`.
- **D3** degenerate not-a-footgun: a bare `$` before a name char interpolates;
  there is no literal-`$`-before-name form (use `'…'` for a literal field).

### E. Delimiter switching (no `\<delim>` escape)

- **E1** literal `/` in pattern via `"` delimiter:
  `v="a/b"; echo ${v:s"a/b"X"}` → `X` (today `${v:s/a\/b/X/}` → garbage `b/X/b`).
- **E2** literal `/` in replacement via `"` delimiter:
  `v=abc; echo ${v:s"b"a/c"}` → `aa/cc`.
- **E3** all three delimiter forms parse: `s/…/…/`, `s"…"…"`, `s'…'…'`.

### F. Regressions / engine facts to lock

- **F1** counted repetition (DD#39 was stale): `x=aaa; if [[ $x =~ ^a{2,3}$ ]]; then echo m; fi` → `m`.
- **F2** empty replacement (delete): `v=abc; echo ${v:s/b//}` → `ac`.
- **F3** global flag: `v=aba; echo ${v:s/a/X/g}` → `XbX`.

## Implementation outline (after specs are red)

1. **Parser** (`parse_regex_rhs`, `try_sed_op`, and the `$v =~ s///`
   statement path): delimiter-aware field scanning. The char after `s`
   is the delimiter (`/`/`"`/`'`); split the literal source into
   `re`/`abc`/`flags` on *unescaped raw* delimiters (no `\` handling —
   `\` is data). `'…'` marks a field non-interpolating. Produce
   structured fields (not a flat `Literal`) so interpolation can happen.
2. **Interpolation**: lower each interpolating field as a value
   expression (`$name`/`${name}` → expansion; `$` before a non-name char
   → literal `$`). Reuse normal expansion lowering. Result is the
   runtime string handed to `regcomp9` / `sash_regex_*`.
3. **`=~` parse**: stop rejecting quoted RHS; accept bare / `"…"` / `'…'`
   and feed the interpolated string to `sash_regex_match`.
4. **Engine checks**: empty pattern/replacement, `g` flag, and what
   Plan 9 `regexp` does with stray `\` — document the answer.

## Open (blocks parts of E / the `\` rule)

DD#63 #4–#6 are unresolved: match-form `/re/` (delimiter vs literal),
`}` inside `${var:s…}`, and confirming `'…'` still engine-escapes `\`.
Resolve in DD#63 before locking E and the `\` spec tests.

## Cross-refs

- Overlaps `dd62-test-expr-ssavalue` at `=~` (that task lowers `[[ ]]`
  value operands to SsaValue; the `=~` *pattern* RHS handled here stays
  raw text either way). Sequence: land DD#62's operand refactor, then
  this; or coordinate the `=~` parse changes.
- Mirror in `emit_c.rs` for c-backend parity.
