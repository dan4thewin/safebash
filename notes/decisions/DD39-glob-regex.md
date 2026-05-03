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
