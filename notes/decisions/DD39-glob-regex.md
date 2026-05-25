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
