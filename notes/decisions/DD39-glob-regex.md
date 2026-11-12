title: DD#39 — Single-star file globs + regex mutation
status: accepted

1. A file glob selects one directory, with at most one `*` in the final
   filename component and a literal directory prefix.
2. Leading dots must be explicit; wildcards never select `.` or `..`.
   Sort each expansion bytewise and preserve enclosing source order.
3. Glob results are clean array elements without word splitting and use
   normalized absolute paths (DD69/DD73); directory failures follow DD130.
4. Regex operations use the chosen Plan 9 dialect, not Bash/POSIX equivalence.
   Construct-opening metacharacters must be escaped to denote literals.
5. Validate every static regex at compile time and identify the offending
   pattern; validated literals are to be compiled once for runtime reuse.
6. Dynamic invalid-pattern failures retain DD55/DD70's ordinary operational
   handling contract, distinct from fatal faults.

---

The selected file-glob policy is deliberately coarse: at most one `*`
in the final filename component, with a non-wildcard directory prefix.
No additional `?`/`[abc]` wildcard operators, multiple stars or wildcard
directory traversal. DD131 packages this in a provisional branch trial
with explicit `glob`/`rglob` commands and scratch collections.
Perl-style regex for string mutation: `var =~ s/re/str/flags` (statement, mutates).
Expression form: `${var:s/re/str/flags}` (returns value, no mutation).
~~`${var/pat/rep}` family accepted via glob→regex coercion.~~ DD65 retires it.

## File glob contract (DG, 2026-11-09)

DG chose a small directory-selection tool, not a second `find`:

> no to anything that needs multiple opendir calls

> no to more than one * in a glob - it should be a coarse tool, regex
> can refine the result

- **One directory per glob.** `*`, `prefix*`, `*suffix` and
  `prefix*suffix` select entries from one directory. A literal directory
  prefix is legal: `sub/*.sh`, `./*`, `../x/*`, `/etc/*`. Open that
  directory once and read its entries; this is not a one-`getdents` limit.
- **Reject broader patterns.** `a*b*c`, `**`, `*/file`
  and `sub*/f*` are not supported patterns, not successful empty matches.
  Validate known patterns statically and computed string patterns at
  runtime (DD131). Do not normalize several stars into one or require a
  star in an explicit `glob` pattern. Use `/usr/bin/find` for traversal.
- **Leading dots are explicit.** The filename pattern must start with
  a literal `.` to select a leading-dot entry. `*` excludes `.config`;
  `.*` includes it, but no wildcard selects `.` or `..`. Literal `.` and
  `..` in the directory prefix remain legal.
- **Sort each expansion bytewise.** No locale or filesystem-enumeration
  order dependence. Composition preserves source order, rather than
  globally sorting or deduplicating the enclosing list (DD69).

The limits are **per glob**, not per array or statement. DD69 allows
several such expansions in array literals, array appends and `for` lists.
This does not change `*` in arithmetic, regexes or literal text outside
glob syntax, introduce inline command-argument globbing, or restore a
retired inline glob/regex-filter form.

DD73 still requires normalized absolute paths; DD69 still supplies clean
array elements without word splitting. Directory-operation failures follow
DD130, not a blanket fatality rule or an invented empty-success value.

**Implementation is a provisional branch experiment (DD131).**
`q-glob-semantics` owns the composed behavior and evaluation;
`bug-glob-multi-star` now owns rejection, not a general matcher.
`bug-glob-directory-portion` retains the already-required literal-prefix
repair. This revision narrows the old unrestricted count of `*` characters;
it does not claim that the current compiler enforces the new contract.
The explicit regex selector uses ordinary regexes rather than the glob
star limit and does not restore DD108's combined shorthand. Journal 603
records the decisions and provisional implementation boundary.

## Regex engine

Plan 9 regexp library. Supports POSIX ERE subset: `.`, `*`, `+`, `?`,
`|`, `()`, `[]`, `[^]`, `^`, `$`. Does **not** support ~~`{n,m}` counted
repetition, `\d`/`\w` shortcuts, or~~ lookahead/lookbehind.

Literal patterns (no variable expansions) are validated at compile time
via `regcomp9` — unsupported syntax is a compile error, not a silent
no-match. The original design also called for validated literals to be
precompiled once at runtime init and reused. The earlier claim that this
was delivered is not supported by the current match/substitution runtime
entrypoints, which call `regcomp9` on use (source inspection, 2026-11-10).
`dd39-precompile-literal-regex` is reopened to reconcile that record and
the outstanding work; it is separate from the diagnostic goal below.

In the shipped implementation, dynamic patterns (containing `$var`) are
compiled at runtime; `regcomp9` failure prints an error to stderr.
DD132 provisionally replaces that consumer-side invalid-data path:
runtime regex operands must be immutable and successfully checked with
`r?` before use. Invalid candidates give quiet false at the predicate;
actual literal regex use retains static validation, and allocation
faults remain fatal. The integrated trial is not yet implemented.

## Clarification (2026-05-24): Runtime regex abort (shipped baseline)

~~When a dynamic pattern (containing $variables) fails regcomp9 at
runtime, the program aborts with exit code 2.~~ That supplies DD55's
unchecked-failure behavior, but does not establish that its handling
levers work; the F15 section below records that discrepancy. DD132's
provisional validation-before-use model supersedes this as the proposed
dynamic-pattern interface, not as a claim about the current binary.

Literal patterns are validated at compile time (SB5023) and cannot
fail at runtime for invalid syntax. This does not make allocation
infallible.

## Clarification (2026-07-08): `{n,m}` supported; literal `{`/`[` need `\{`/`\[`

**Correction:** the engine **does** now support `{n,m}` counted repetition
(the "Regex engine" paragraph above is stale on this point) — e.g.
`^a{2,3}$` matches. ~~`\d`/`\w`/lookaround remain unsupported.~~
The September correction below retains only the lookaround restriction.

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
  errors**, exactly like sash. ~~Since sash subsets bash and targets
  bash-faithful semantics, we keep the strict behavior — a bash user's
  `[[ $x =~ a{b ]]` already errors in bash. Going lenient would match GNU
  grep but *diverge from bash*.~~ The later engine-choice ruling is authoritative.
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

**Diagnostic-quality objective (DG, 2026-11-10; implementation pending).**
SB5023 must clearly identify the offending static regex, including when
one test, attest predicate or substitution chain contains several
patterns. The approach is deferred until the task is undertaken:
per-pattern source positions, explicit pattern text and a catalogue of
unique regexes are candidates to evaluate, not selected mechanisms.
Neither exact-character locations nor diagnostic grouping is prescribed.
`q-sb5023-names-the-test-not-the-pattern` owns the work.

The diagnostic code, regex dialect and validation scope remain unchanged.
DD132's provisional quiet-false contract for invalid `r?` candidates stays;
better diagnostics do not make those candidates static-regex errors or
select runtime caching.

**Current implementation baseline, not the new diagnostic design:**
one constructor, `validate.rs::check_static_regex`, raises it for all three
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

**Historical discrepancy, now provisionally addressed by DD132.**
DD#55 says `-` suppresses a dynamic regcomp
failure, and DD#70 (A) adds that `if`/`||` catch it. This DD's 2026-05-24
clarification says only that it aborts. DD#71 says it "aborts via the DD#55
(A) channel" and names no lever, though that channel is the one DD#70 gives
levers. No regex builtin honours either lever. `match`, `matchall` and
`[[ =~ ]]` exit 2 regardless, before this change and after it; `sub` and
`gsub` carried on regardless before it, and now exit 2 regardless. Filed as
`notes/tasks/q-dynamic-regcomp-levers.md`. The 2026-11-09 choice is instead
validation before use, not blanket runtime abort or a failed-substitution
fallback. `q-glob-semantics` owns that integrated branch trial and the
DD revision inventory. These F15 observations have not been remeasured.

Specs: 39.36–39.56 (regex.sh). Earlier pins: 39.5 (wordsplit.sh, the
statement form) and e.a70.regcomp (errors.sh, the exit under `if match`).
