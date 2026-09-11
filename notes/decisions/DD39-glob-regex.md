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

## Clarification (DG, 2026-09-11): the engine is a choice, not an approximation

**Ruling.** The Plan 9 regexp library was chosen **on faith in its author**
(Rob Pike). It represents his view of a sane, pre-feature-creep regular
expression implementation. **POSIX compliance is not a goal.** The small
changes made to the vendored copy were made for *convenience*, not for
compliance.

This settles a class of report that had been arriving as bug filings: "bash
accepts pattern P and sash does not, therefore sash is broken." The premise is
wrong. Where the engine's language differs from POSIX ERE or from bash's
glibc `regcomp`, the difference is **deliberate** unless it produces a wrong
*answer* (a match that should not match, or data mis-handled — those remain
bugs, e.g. `bug-regex-blind-past-interior-nul`).

Note this is a *different* justification from the 2026-07-08 `{`/`[` ruling
above. There, strictness was defended because **bash errors too**. Here, bash
accepts `[A-Za-z-]` and sash does not. The divergence is real and is accepted
anyway, on the reasoning in this ruling.

### A literal `-` in a bracket expression is spelled `\-`

POSIX says a `-` first or last inside `[…]` is a literal. This engine does not;
it reports `malformed '[]'` (surfaced as SB5023 for a literal pattern). The
escape is the spelling, and it works in every position. Measured 2026-09-11,
each agreeing with bash's result on the same subject:

| pattern          | sash              | bash |
|------------------|-------------------|------|
| `^[A-Za-z\-]+$`  | matches `a-b`     | same |
| `^[\-A-Za-z]+$`  | matches `-ab`     | same |
| `^[a-z_\-]+$`    | matches `a_b-c`   | same |
| `^[A-Za-z-]+$`   | **SB5023**        | accepted by bash |
| `^[-A-Za-z]+$`   | **SB5023**        | accepted by bash |

So sash does not match a *smaller set of strings* than bash — the same classes
are expressible, they are spelled with an escape. This is the same shape as the
`{` / `[` ruling: the character that opens a construct needs its escape to be a
literal.

### Correction: `\d` `\w` `\s` (and `\D` `\W` `\S`) ARE supported

Both the "Regex engine" paragraph at the top of this DD and the 2026-07-08
clarification say `\d`/`\w` are unsupported. **Stale.** The vendored engine has
a `bldshortclass` addition providing all six; verified 2026-09-11:

- `\d` = `[0-9]`, `\w` = `[0-9A-Z_a-z]`, `\s` = `[\t-\r ]`
- the uppercase spellings are the negated classes

Lookahead/lookbehind remain unsupported, and that part is not stale.

### `{n,m}` is capped at 32

`parsecounted` rejects a count above 32 with `repetition count too large`
(so `^a{33}$` is SB5023). The cap is a vendored addition, not upstream.

### What is actually vendored

`sash-rt/csrc/regexec.c` and `regaux.c` are **byte-identical** to
`contrib/plan9port-regexp/`. Every local change lives in `regcomp.c` (~223
lines): `parsecounted` (`{n,m}` + the cap) and `bldshortclass` (the six
shorthand classes). Both are pure additions to the parser — the matcher is
untouched upstream code.

### Follow-up

The dialect deserves a full write-up in the man page / docs — what the engine
supports, what it deliberately does not, and why. Filed as
`docs-regex-dialect-writeup`.
