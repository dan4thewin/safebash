title: DD#71 — Operand words: `=~`/`==` RHS and assignment values are plain words; value positions aren't connectable commands
status: accepted
implemented: 2026-07-04 (Cluster A — see journal 128; commits 9424d66,60c667b,e9fc520,1c2e74f,000dd5a)

Decided with DG 2026-07-04, out of the DD#66 dispatch follow-up. Started
as "how does a `=~` regex include a space?" and generalised into a single
lexing principle plus a language-identity choice about assignments. Builds
on DD#62 (`==` exact / `=~` regex), DD#63 (regex builtins; `=~` is sugar
for `match()`), DD#66 (dispatch form), and the DD#55/DD#70 error model.

## Principle

**`=~` is sugar for `match(subject, pattern)`, and the pattern is an
ordinary string handed to regcomp.** It follows that the RHS of `=~` gets
**no unique handling** — it is a normal *operand word*, exactly like the
RHS of `==`. The operator difference (strcmp vs regcomp) lives entirely in
lowering, never in parsing. The bespoke regex collector
(`parse_regex_rhs`/`regex_rhs_step`/`regex_punct_char`) is deleted.

## Operand words

An **operand word** is a single word that:

- consumes to the next **unquoted whitespace** (or the enclosing closer),
- treats regex/shell metacharacters (`( ) { } [ ] | ^ $ . * + ?` …) as
  **ordinary characters** — in operand position none of them is a shell
  operator (no pipe/subshell/brace-group there),
- still honours **`$`/`` ` ``/quotes** as sub-structure starters
  (expansions, command substitution, quoting) — so interpolation and
  quoting are the established behaviours,
- includes a **literal space only when quoted/escaped** (`'a b'`,
  `"a b"`, `a\ b`), because whitespace is the word terminator.

So `[[ $x =~ ^(a|b)$ ]]` works bare, `[[ $x =~ 'a b' ]]` matches a space,
and `[[ $x =~ foo bar ]]` is *not* a silent `foobar` — it is `foo`, then a
dangling `bar` (a clean error / a second dispatch pattern), never a silent
miscompile.

### Why this is the whole fix, not a patch

The two `=~` defects (silent space-drop; quoted-regex rejected — see
`bug-regex-rhs-space-quote`) both **vanish by construction** once the RHS
is an operand word: there is no bespoke path left to drop spaces or ignore
quote tokens. The DD#66 dispatch-arm `{` disambiguation heuristic (the
`regex_rhs_step` whitespace-gap check) also disappears: a counted
repetition `a{2,3}` is one word (no internal space); a dispatch arm `{` is
whitespace-separated, so consume-to-whitespace stops before it naturally.

## Lexer scope — where operand words apply

The absorb-to-whitespace behaviour is a **lexer mode** scoped to *value
positions that are not themselves a connectable command*:

| position | operand word? | terminator |
|---|---|---|
| `[[ ]]` comparison operands (both sides) | **yes** | whitespace / `]]` |
| dispatch arm patterns (`pat { … }`) | **yes** | whitespace(→`{`) / `]]` |
| assignment / declaration RHS (`x=`, `int x=`, `const`, array elements) | **yes** | whitespace / `;` / newline |
| command arguments (`echo a b`) | **no** | `\| & ; < >` stay operators |
| the `[[ ]]` boolean skeleton (`&& \|\| ! ( )`) | **no** | structural operators |

The discriminator is **"can the thing to the left legally take a command
connector?"** A command can (`echo foo | cat` is a real pipe), so command
args keep operator lexing. A `[[ ]]` operand and an assignment value
cannot, so they absorb metacharacters. This also means only the *operand*
slots inside `[[ ]]` are absorb-mode — the boolean connectives that
structure the test stay operators.

## Assignments are value bindings, not commands

The assignment case is licensed by a language-identity choice: **an
assignment (and typed declaration) is a value binding, not a command.** It
has no meaningful exit status, and cannot be the source of a pipe, a
background job, or a `&&`/`||` chain. Empirically these forms already do
not work in sash (`x=hi && echo $x` prints empty; `a=foo | cat` loses the
assignment to a subshell), so nothing useful is lost. Consequently
`a=foo|cat`, `a=foo&`, `a=foo&&bar` are the **literal values**
`foo|cat` / `foo&` / `foo&&bar`; `;`/newline still sequence statements and
whitespace still bounds the value (`a=foo bar` → `a=foo`, then `bar`).

The dual of this — **a boolean context must be fed a boolean/exit-status;
a value binding used there is a compile error** — is the DD#70 extension
filed as `reject-nonstatus-in-boolean-context` (that is where
`x=… && …` / `if x=…; then` become clean rejects instead of silently
empty).

**Suppression adjacency (DG, 2026-11-08).** Reject `- x=$(cmd)` rather
than treating it as shorthand for `x=$(- cmd)`: `-` belongs directly on
the command whose result is discarded, not on its value binding. The same
rule covers append and cell assignments; declarations retain their
existing refusal. DD55 records the full rule and pending
`reject-nonadjacent-suppression` enforcement. DD88/DD101's explicit
capture-condition exceptions remain handling forms, not permission for
an assignment-level `-`.

## Multi-exact dispatch arms (falls out of the operand rule)

Because a `[[ ]]` operand consumes to whitespace, a dispatch arm may carry
**multiple space-separated exact patterns**: `[[ $x == a b c { … } ]]`
runs the group if `$x` is `a`, `b`, or `c` (first-match-wins across the
arm). This gives `==` the OR-of-exacts it structurally lacks (DD#66 keeps
`|` out of `==`), so plain alternation stays in the readable `==` form
instead of forcing `=~ ^(a|b|c)$`. **Backends need no change** —
`CaseItem.patterns` is already a `Vec` and native/c/janet already lower
multi-pattern arms; this is parser-only. `=~` arms stay single-regex
(regex expresses alternation internally as `(a|b)`; a space in a `=~`
pattern is quoted like any operand word).

## Quote semantics (confirmed)

Quotes are **shell-level string construction, not regex-literalisation.**
`[[ $x =~ '^(a|b)$' ]]` builds the string `^(a|b)$` and regcomp still sees
it as an alternation — quotes protect the metacharacters from the *shell*
(word-splitting), they do **not** make regex metacharacters literal. This
deliberately diverges from bash's "a quoted substring matches literally"
rule, in favour of the simpler "the pattern is just a string arg to
`match()`" model.

## Dynamic regex (confirmed supported)

An interpolated pattern (`[[ $y =~ "$x" ]]`, `=~ $pat`) is supported. When
the pattern is **static** (bare or single-quoted literal) it is validated
at compile time (DD#39/#63, `regex_validate.rs`); when it is **dynamic**
(interpolated) it defers to the runtime regcomp, whose failure aborts via
the DD#55 (A) channel. Validate-when-static / abort-when-dynamic.

## Removed / added

- **Removed:** `parse_regex_rhs`, `regex_rhs_step`, `regex_punct_char`
  (sash/src/parse/parser.rs) and the `{` whitespace-gap heuristic.
- **Added:** an operand-word lexer mode (absorb to whitespace / closer,
  quote- and `$`-aware) shared by `[[ ]]` operands/patterns and
  assignment/declaration RHS. `==` and `=~` RHS parse through the same
  routine.
- **Related fixes:** `bug-regex-rhs-space-quote` (superseded — resolved by
  construction), `bug-dispatch-literal-star-wildcard` (`*` is a literal
  operand; `||` is the only default — same "no pattern language" spirit),
  and the multi-exact arm feature.

## Implementation

Spec-first, per `operand-word-absorb-mode` (lexer mode + delete the
collector + multi-exact) and `reject-nonstatus-in-boolean-context` (the
DD#70 dual). SB codes assigned at implementation (the reject joins the
SB401x must-consume family; keep adjacent per DD#70). Migration: existing
unquoted-`=~` specs are unaffected (bare metachars still work); new specs
cover quoted spaces, dynamic patterns, multi-exact arms, and the assignment
literals.

## Addendum (2026-10-26): a declaration stops where an assignment does

"An assignment (and typed declaration) is a value binding", but only the
untyped assignment ended at a list or pipe operator. A typed declaration took
every word to the end of the statement as another assignment, so
`int n=1 && /bin/echo ok` read `&&` as a name and said SB4014 "expected
variable name in declaration" (batch-531 B28). A declaration now stops at an
`|`, `&`, `&&` or `||` token, as an untyped assignment does, and gets the same
verdicts: SB4025 "a declaration is a value binding, not a boolean or exit
status" as an `&&`/`||` operand, SB4022 as a pipeline stage and SB4024 under
`&`. The value-word rule above is unchanged, since only an operator token ends
the declaration: `str s=a|b`, `str s=a&` and `str s=a&&b` are still literals.

In a control-flow header the `{` that opens the body ends a declaration too,
as it ends a command's words there (DD#68). The declaration took it as its
next name and said SB4014, so `if int n=$(cmd) { … }` now gets DD#88's
SB4025, which the keyword spelling `if int n=$(cmd); then` already got.
Outside a header a `{` does not end a declaration (decide-and-report):
`int n=1 { … }` stays SB4014, while the untyped `s=x { echo hi }` splits in
silence into `s=x` and a `{ }` group and prints `hi`. Which of the two is
right is `q-statement-form-residuals`.

Specs: errors.sh 71.1-71.9. Journal 537.
