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

## Addendum (2026-10-25): every static regex is compiled at compile time

The 2026-05-24 clarification says a literal pattern "cannot fail at runtime".
Until now that held only where SB5023 looked: a lone top-level
`[[ $x =~ re ]]`, and the first edit of an `x =~ s/…/…/` statement, where the
check also skipped any RE containing `$`. Every other static pattern compiled
clean and died at runtime with `regexp error`. SB5023 now covers every regex
the runtime compiles:

- argument 1 of `sub`, `gsub`, `match` and `matchall`, wherever the call
  stands (statement, condition, `-`, capture);
- every edit of a `${x:s/…}` chain, of the cell sugar `${a[i]:s/…}` and of an
  `x =~ s/…` chain, since all three desugar to `sub`/`gsub`;
- `=~` anywhere in a `[[ ]]` test, under `&&`, `||` and `!` as well as alone,
  attest predicates included;
- each pattern of a regex dispatch (DD#66), every arm.

A `$` in a static pattern is an anchor (DD#63), so an RE containing one is
checked like any other: `x =~ s/(c$/Z/` is SB5023.

One constructor, `validate.rs::check_static_regex`, raises it for all three
callers (the `[[ ]]` walk, the builtin operand, the dispatch arm). The message
is `invalid regex in substitution: …` for `sub`/`gsub` and
`invalid regex pattern: …` otherwise. An operand or a dispatch arm reports its
pattern. A `[[ ]]` test reports its command: the `[[`, or an attest
predicate's `attest`. The `${…:s/…}` sugar reports the word that holds it, and
the statement form its statement. The parser's separate first-edit check is
gone.

This supersedes the 2026-07-08 scope sentence ("Literal-pattern compile
validation (SB5023) runs on the `=~` mutation form and `[[ =~ ]]`").

A dynamic pattern still compiles at runtime. `sub`/`gsub` used to print
`regexp error`, return the subject unchanged and carry on with status 0; they
now exit 2, as `match`, `matchall` and `[[ =~ ]]` already did. That covers the
`${x:s/…}` sugar and the statement form, which run on `sub`/`gsub`.

**Open, not settled here.** DD#55 says `-` suppresses a dynamic regcomp
failure, and DD#70 (A) adds that `if`/`||` catch it. This DD's 2026-05-24
clarification says only that it aborts. DD#71 says it "aborts via the DD#55
(A) channel" and names no lever, though that channel is the one DD#70 gives
levers. No regex builtin honours either lever. `match`, `matchall` and
`[[ =~ ]]` exit 2 regardless, before this change and after it; `sub` and
`gsub` carried on regardless before it, and now exit 2 regardless. Filed as
`notes/tasks/q-dynamic-regcomp-levers.md`; this addendum takes no position.

Specs: 39.36–39.56 (regex.sh). Earlier pins: 39.5 (wordsplit.sh, the
statement form) and e.a70.regcomp (errors.sh, the exit under `if match`).
