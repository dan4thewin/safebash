title: DD#39 — Star-only glob + regex mutation
status: implemented

Star-only glob for files (`*` only, no `?` or `[abc]`).
Perl-style regex for string mutation: `var =~ s/re/str/flags` (statement, mutates).
Expression form: `${var:s/re/str/flags}` (returns value, no mutation).
`${var/pat/rep}` family accepted via glob→regex coercion.

## Regex engine

Plan 9 regexp library. Supports POSIX ERE subset: `.`, `*`, `+`, `?`,
`|`, `()`, `[]`, `[^]`, `^`, `$`. Does **not** support `{n,m}` counted
repetition, `\d`/`\w` shortcuts, or lookahead/lookbehind.

Literal patterns (no variable expansions) are validated at compile time
via `regcomp9` — unsupported syntax is a compile error, not a silent
no-match. Validated literal patterns are precompiled once at runtime
init and reused at every match site.

Dynamic patterns (containing `$var`) are compiled at runtime; `regcomp9`
failure prints an error to stderr.

## Clarification (2026-05-24): Runtime regex abort

When a dynamic pattern (containing $variables) fails regcomp9 at
runtime, the program aborts with exit code 2. This is consistent
with DD#55's error model: runtime calls that can fail on data are
checked, and failure = abort.

Literal patterns are validated at compile time (SB5023) and cannot
fail at runtime.

## Clarification (2026-07-08): `{n,m}` supported; literal `{`/`[` need `\{`/`\[`

**Correction:** the engine **does** now support `{n,m}` counted repetition
(the "Regex engine" paragraph above is stale on this point) — e.g.
`^a{2,3}$` matches. `\d`/`\w`/lookaround remain unsupported.

**Ruling (DG, 2026-07-08) — bare `{` / `[` are not silent literals.**
A `{` opens a `{n,m}` interval and a `[` opens a `[…]` class. To match a
**literal** brace/bracket, escape it: `\{` / `\[`. A bare `{` that does not
open a valid interval, and an unterminated/invalid `[`, are a **compile or
runtime regex error — never a silent literal.** One uniform rule for both:
*a construct-opening metacharacter must be escaped to be literal; a bare or
invalid one is an error.*

Rationale (surveyed the field, 2026-07-08):

| engine                         | bare `{` (invalid interval) | unterminated `[` |
|--------------------------------|-----------------------------|------------------|
| glibc `regcomp` = **bash `[[ =~ ]]`** | **error** ("Unmatched \{") | **error**    |
| GNU grep/sed/awk (ERE)         | literal (lenient)           | **error**        |
| Perl                           | literal + deprecation warn  | **error**        |
| Go RE2                         | literal (silent)            | **error**        |
| **sash** (Plan 9 engine)       | **error** (`\{` for literal)| **error** (`\[`) |

- **`[` is unanimous:** every engine errors on an unterminated `[`; nobody
  treats a bare `[` as a silent literal. sash already matches this.
- **`{` splits:** the GNU command-line tools / Perl / RE2 are lenient
  (literal), but **bash's own `=~` engine (glibc `regcomp REG_EXTENDED`)
  errors**, exactly like sash. Since sash subsets bash and targets
  bash-faithful semantics, we keep the strict behavior — a bash user's
  `[[ $x =~ a{b ]]` already errors in bash. Going lenient would match GNU
  grep but *diverge from bash*.
- Keeping strict also makes `{` and `[` behave the same (both need their
  escape), which is cleaner than GNU/Perl's internal inconsistency, and
  avoids relying on POSIX-ERE-undefined behavior.

Closes the `decide-regex-bare-brace` task. No spec change (63.g3 already
uses `\{`). Literal-pattern compile validation (SB5023) runs on the `=~`
mutation form and `[[ =~ ]]`; a bad static brace/bracket there is a compile
error, a dynamic one a runtime `regexp error` (per the DD#55 abort model).
