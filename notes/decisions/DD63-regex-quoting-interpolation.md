title: DD#63 — Regex as a builtin family (match / sub / gsub) with thin sugar
status: candidate

Regex is a family of **builtins** that take ordinary sash string
arguments; the familiar syntaxes (`[[ =~ ]]`, `s/re/sub/`,
`${var:s/re/sub/}`) are **thin sugar** that desugar to those builtins.
Because the arguments are normal strings, quoting and interpolation are
just normal argument evaluation — there is no bespoke regex-field
parser. From the 2026-06-17 discussion. (SUBSET.md and DD#39's regex
prose are stale; this DD is authoritative.)

## Why a builtin family

The alternative — a special parser that handles delimiters, `$`
interpolation, and `\` inside regex/replacement fields — is complex and
was the source of the open questions in the first draft of this DD. By
making the operations callables whose args are ordinary strings, all of
that collapses into "evaluate the argument": `"$re"` interpolates,
`'$re'` doesn't, `"a/b"` carries a literal slash, no delimiter needed.
The user can always express anything via the function form; the sugar is
a convenience for the common case.

This also **unifies with DD#62**: `[[ $x =~ re ]]` desugars to a `match`
*command*, so `=~` leaves the `TestExpr` entirely. The `[[ ]]` test path
then has no pattern operand at all — every operand is a value — which is
exactly the precondition DD#62 needs to lower test operands to `SsaValue`
and delete `right_is_quoted` + the duplicate `lower_subscript`.

## The builtins

Join the existing builtin family (`keys`/`join`/`len`). Reserved names:
`match`, `sub`, `gsub`.

- **`match SUBJECT PATTERN`** — regex match (unanchored).
  - As a command: exit status 0 if it matched, non-zero otherwise (the
    boolean form, used by `if` / `&&` / the `[[ =~ ]]` sugar).
  - As `$(match SUBJECT PATTERN)`: prints the whole match (`resub[0]`)
    to stdout — an ordinary scalar command substitution.
  - **`match OUTARRAY SUBJECT PATTERN`** — out-array form (mapfile /
    `read -a` style): fills `OUTARRAY` with `[whole, g1 … g9]` and returns
    the boolean exit status. This is how capture groups are surfaced — no
    array-returning command-subs, no scalar-vs-array context detection.
- **`sub SUBJECT PATTERN REPLACEMENT`** — replace the **first** match,
  print the result to stdout.
- **`gsub SUBJECT PATTERN REPLACEMENT`** — replace **all** matches.

`g` is therefore the `sub`/`gsub` choice, not a flag argument. There are
**no other flags** — `regcomp9` takes only a pattern and the vendored
Plan 9 engine has no `REG_ICASE`; case-insensitive is unsupported (would
need vendored-engine work, deliberately punted).

## Captures

The Plan 9 engine already fills a 10-slot `Resub` (`regexec9(…, resub,
10)`): `resub[0]` is the whole match, `resub[1..9]` up to nine groups.
`sash_regex_replace` already uses `resub[0]`; `sash_regex_match`
currently computes and discards the `Resub` — surfacing groups is a
wrapper change, not an engine change. (Caveat: the two `Resub`-stride
comments in `sash-rt/src/lib.rs` disagree — 32 vs 16 bytes/entry; confirm
against `regexec.c` before reading `resub[1..]`.)

**Replacement backreferences** (`sub`/`gsub` only): `\0` = whole match,
`\1`…`\9` = capture groups. This is *replacement-template* substitution
using `resub`, **not** engine pattern-backreferences — Plan 9 regexp
still cannot match `(a)\1` against `aa`; that remains unsupported (and
consistent). So `\` has two meanings by field: in the **pattern** it is
an engine escape (handed to `regcomp9`); in the **replacement** it
introduces a backref. `\\` is a literal backslash in the replacement.

## Interpolation (textual first pass)

A pattern/replacement is a string first, a regex only when it reaches the
engine. In the **function form** this is just argument evaluation. In the
**`s///` sugar** the fields always interpolate:

- `$name` / `${name}` interpolate exactly as elsewhere in sash; `$` not
  followed by a name char (or at end) is a literal regex anchor
  (`foo$`, `^foo$`). A literal `$` immediately before a name char is a
  degenerate regex (anchor-then-chars never matches), so nothing useful
  is lost.
- Metachar collisions in an interpolated value are the user's concern
  (use `'…'` in the function form, or `fgrep`/`grep -F` for literal
  containment — see "Compromises").
- Dynamic patterns (containing interpolation) compile at runtime; a
  literal pattern can still be pre-validated at compile time via
  `regcomp9` (as DD#39 intended).

## `\xnn` byte escape

Add `\xnn` (exactly two hex digits) to `regcomp` — matches that byte.
`\x` followed by a non-hex digit is an **error**. This lets a pattern
contain any literal byte (a delimiter, space, control char) without
delimiter gymnastics. It is a **pattern/engine** feature; the replacement
uses `\0…\9` backrefs, not `\xnn`.

## The thin sugar

Kept deliberately minimal — only the common case; anything else uses the
function form.

- **`[[ $var =~ RE ]]`** → `match $var RE`. `RE` is **just an argument**
  (a single word): `=~ "a b"` is a quoted arg (space, interpolating),
  `=~ 'a b'` is single-quoted (literal), `=~ $pat` interpolates,
  `=~ /re/` is the literal regex `/re/`. No special parsing — `/` is not
  a delimiter here.
- **`var =~ s/RE/SUB/`** → `var = $(sub $var "RE" "SUB")`;
  **`var =~ s/RE/SUB/g`** → `var = $(gsub $var "RE" "SUB")`.
- **`${var:s/RE/SUB/}`** → `$(sub $var "RE" "SUB")`;
  **`${var:s/RE/SUB/g}`** → `$(gsub $var "RE" "SUB")`.

The `s///` sugar is `/`-delimited **only** and **always interpolating**
(the fields desugar to double-quoted, i.e. interpolating, args). A `/`
inside RE/SUB, a non-interpolating field, or any other delimiter → drop
to `$(sub …)` / `$(gsub …)` and quote the args however you like (or use
`\x2f` for a literal `/`). Desugaring happens in lower/transform (like
the other builtins), keeping the parser thin.

## Compromises (deliberate)

- **Literal containment** of a variable's value is not a regex concern:
  use `fgrep` / `grep -F "$needle"`. `==` is exact (DD#62), `=~`/`match`
  is regex; there is no literal-regex mode.
- **No case-insensitive** flag (engine limitation).
- A literal `}` cannot appear inside the `${var:s/…}` **expression**
  sugar (the brace scanner closes at it); the statement form
  `var =~ s/…` and the function form are immune.

## Reality check (why interpolation "just works" here)

Field interpolation is currently **unimplemented** on every surface —
`$var` in a pattern/replacement is literal (`[[ $x =~ $pat ]]` doesn't
interpolate; `${pat}` → SB4014; `${v:s/$x/Y/}` no-ops; `v =~ s/$x/Y/` →
`sash: bug:`). The function-family model fixes this for free (args are
evaluated normally). DD#39's "dynamic patterns compiled at runtime" was
aspirational; DD#39 is also stale on `{n,m}` (counted repetition works).

## Open / to confirm

- `case` glob: desugar `=glob` to a `match`/glob builtin too (removes the
  last `[[ ]]`/`case` pattern operand)? Proposed, not locked.
- `\` followed by a non-digit in the **replacement**: literal, or error?
- The `Resub` stride (32 vs 16) before reading `resub[1..]`.

See task `regex-interpolation-delimiters` for the spec-first
implementation; `dd62-test-expr-ssavalue` for the overlapping operand
work (the `=~`-as-`match`-command desugaring is the seam between them).
