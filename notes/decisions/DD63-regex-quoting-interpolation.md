title: DD#63 — Regex as a builtin family (match / sub / gsub) with thin sugar
status: accepted

1. Regex builtins take ordinary string arguments. Matching is unanchored;
   `sub` replaces the first match and `gsub` all matches. Quoting controls
   string evaluation, not regex literalization.
2. Literal patterns are checked at compile time; dynamic patterns compile
   at runtime under the ordinary recoverable-failure contract. Patterns
   support exact two-digit `\xNN` escapes; replacements support only
   `\0`-`\9` backreferences and `\\` as backslash escapes.
3. `match` supplies a boolean, a whole-match value, or a boolean with an
   out-array of whole match and up to nine groups. `matchall` requires an
   out-array and collects each match's groups, or whole matches if ungrouped.
   Both out-array forms introduce their target if needed and clear it before
   filling; no match leaves it empty. Boolean results obey DD70.
4. `[[ SUBJECT =~ PATTERN ]]` is regex-test sugar. Substitution sugar uses
   slash-delimited interpolating words, a mandatory final slash, and optional
   `g`; mutation writes its subject without producing a result.
5. Regex subjects are examined only before the first interior NUL; later
   bytes are preserved. Case folding uses `toupper`/`tolower`.
6. Attest predicates may use regex tests, but not substitution captures or
   their expansion sugar.

---

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
*command*, ~~so `=~` leaves the `TestExpr` entirely. The `[[ ]]` test path
then has no pattern operand at all — every operand is a value — which is
exactly the precondition DD#62 needs to lower test operands to `SsaValue`
and delete `right_is_quoted` + the duplicate `lower_subscript`.~~
The attest-predicate addendum below retains a regex test node.

## The builtins

The out-array fill contracts below describe the shipped replacement/
clear behavior. DD131 provisionally includes `match`/`matchall` in an
accumulation/immutable-scratch branch trial, preserving their own no-match
verdicts and capture layout within each produced batch. It also proposes
the separate basename selector `rglob`; neither that command nor the new
destination model is implemented by this recording.

DD132 extends that **provisional** trial: runtime patterns require
successful `r?` proof for the same immutable value, and `sub`/`gsub`
return a match/no-match boolean alongside their text. No match yields
the original subject and false; an identity replacement can yield true.
Existing text-producing regex forms may use a capture-condition to
consume the boolean; text-only explicit calls discard it with
`$(- sub ...)` / `$(- gsub ...)`. This does not invent a scalar
`matchall` form or unrestricted bare value-intrinsic statements.

Join the existing builtin family (`keys`/`join`/`len`). Reserved names:
`match`, `sub`, `gsub`, `toupper`, `tolower`.

- **`match SUBJECT PATTERN`** — regex match (unanchored).
  - As a command: exit status 0 if it matched, non-zero otherwise (the
    boolean form, used by `if` / `&&` / the `[[ =~ ]]` sugar).
  - As `$(match SUBJECT PATTERN)`: ~~prints~~ the whole match (`resub[0]`)
    ~~to stdout — an ordinary scalar command substitution~~.
    This is a value-intrinsic capture (DD111).
  - **`match SUBJECT PATTERN OUTARRAY`** — out-array form (the array is
    the trailing positional, ~~consistent with `mapfile -m m` / `read -a a`~~):
    fills `OUTARRAY` with `[whole, g1 … g9]` and returns
    the boolean exit status. This is how capture groups are surfaced — no
    array-returning command-subs, no scalar-vs-array context detection.
- **`sub SUBJECT PATTERN REPLACEMENT`** — replace the **first** match,
  ~~print the result to stdout~~. Capture its value (DD111).
- **`gsub SUBJECT PATTERN REPLACEMENT`** — replace **all** matches.
- **`toupper STRING`** / **`tolower STRING`** — case fold (awk names;
  DD#65). Compensates for the engine's missing `i` flag — case-insensitive
  match is `match $(tolower $x) $(tolower $pat)`.

The case-folded-pattern example describes the existing value interface.
In the DD132 trial, construct and validate the **folded result** first;
proof for the original pattern or for a substring does not certify it.

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

**Backslash (per DD#64).** `\` is *not* regex-special at the parser
level. Under DD#64 the shell does **no** backslash processing outside
`$'…'` and `\`-at-EOL, so `\` is **verbatim** in both `'…'` and `"…"` and
reaches the regex engine untouched: `'a\.b'`, `'a\\b'`, `"a\.b"`,
`"a\\b"` all hand the engine exactly those bytes. The recommended form is
a **single-quoted** regex (standard regex escaping, no shell
involvement); use `"…"` only to interpolate. A literal byte that is
awkward to type is `\xNN` (`\x5c` = `\`, `\x7d` = `}`), and a literal `"`
or `$` inside an interpolating pattern is concatenation (`'"'`, `'$'`).
No `"\\\\"`, no `scan_dq_escape` collapse. (This supersedes the earlier
draft's double-quote-escape layer.)

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
- Dynamic patterns (containing interpolation) compile at runtime in the
  shipped model. DD132's trial requires a previously validated immutable
  finished pattern instead: interpolation that assembles new bytes needs
  its own construction and `r?` check. A literal pattern retains
  compile-time validation via `regcomp9` (DD#39). Within that trial:
  Neither `:!` nor disabled
  taint checking supplies regex proof, and pattern operands forbid trusted
  reads even when subject/replacement trust is legal.

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

These are the shipped text-only desugarings. Under the DD132 trial,
substitution value sugar explicitly projects text and discards the new
boolean (equivalent to consuming the value from `$(- sub ...)` or
`$(- gsub ...)`). Mutation sugar still performs a resultless write,
so it cannot target immutable scratch. Chained edits retain their order,
projecting each text result without inventing a chain-wide verdict.
Every pattern, including an interpolated sugar operand, owes the same
proof as an explicit call.

The `s///` sugar is `/`-delimited **only** and **always interpolating**
~~(the fields desugar to double-quoted, i.e. interpolating, args)~~. To make
this work inside the `${var:s/…}` form, the **lexer treats `/` as a
quote-delimiter after `:s`** (Path B, 2026-06-18): exactly as `'…'` and
`"…"` already suppress brace-counting inside `${…}`, the `:s/RE/SUB/[g]`
payload is scanned as three `/`-delimited fields, so a bare `{`/`}` inside
RE or SUB is literal and does **not** close the expansion. The closing
`}` is recognised only after the third `/` (and optional flags); the
trailing `/` is **mandatory** — `s/RE/SUB/` has exactly three delimiters,
so `${var:s/a/b}` with only two is malformed. A
**malformed** payload — fewer than the required delimiters, or
unterminated before the close — is a **compile error**, not a silent
fallback (see Compromises). A literal `/` (the delimiter itself) is
`\x2f`, or drop to the `$(sub …)` / `$(gsub …)` function form, which needs
no delimiter and can quote the args however you like. Desugaring happens
in lower/transform (like the other builtins), keeping the parser thin.

## Compromises (deliberate)

- **Literal containment** of a variable's value is not a regex concern:
  use `fgrep` / `grep -F "$needle"`. `==` is exact (DD#62), `=~`/`match`
  is regex; there is no literal-regex mode.
- **No case-insensitive** flag (engine limitation).
- A bare `{`/`}` inside the `${var:s/RE/SUB/}` payload is **literal**, not
  a brace-count event: under Path B (above) the lexer scans the
  `:s`-payload as `/`-delimited fields, so `${var:s/a}b/z/}` substitutes
  `a}b`→`z` — it does *not* truncate. The closing `}` is the one after the
  third `/`. A **malformed** `:s` payload — e.g. `${var:s/a}` (one
  delimiter, unterminated) — is a **compile error** (**SB4032**; the `:s`
  structure is
  incomplete), whereas a *well-formed* `${…}` followed by ordinary text —
  e.g. `${var:s/x/z/}tail` — simply **concatenates** the literal `tail`.
  The error is localised to the `:s` payload shape; it is never "leftover
  chars after `}`". No `\}` is needed (DD#64 removes it). A literal `/`
  (the delimiter) is `\x2f`; for anything more, use the statement form
  `var =~ s/…` or the function form `$(sub …)`, which have no enclosing
  braces.

## Reality check (why interpolation "just works" here)

Field interpolation is currently **unimplemented** on every surface —
`$var` in a pattern/replacement is literal (`[[ $x =~ $pat ]]` doesn't
interpolate; `${pat}` → SB4014; `${v:s/$x/Y/}` no-ops; `v =~ s/$x/Y/` →
`sash: bug:`). The function-family model fixes this for free (args are
evaluated normally). DD#39's "dynamic patterns compiled at runtime" was
aspirational; DD#39 is also stale on `{n,m}` (counted repetition works).

## Resolved (DG, 2026-06-21)

- **`\` + non-digit in a `sub`/`gsub` replacement → compile error.** Only
  `\0`–`\9` (backrefs) and `\\` (literal backslash) are valid; every other
  `\<char>` is rejected (a new SB code). Holds the explicit line (DD#64
  removed shell backslash; sash rejects ambiguity rather than guessing).
  This deliberately diverges from sed, which is lenient (drops the
  backslash for unknown escapes and recognizes a `\t`/`\n`/… table). A
  literal char is just itself; a literal backslash is `\\`.
- **`Resub` stride = 16 bytes** (2 pointers per entry: start `s`, end `e`;
  `regexp9.h`). Read `resub[1..9]` at 16-byte stride. The `sash-rt/src/
  lib.rs:1307` comment ("4 pointers … 320 bytes") is **wrong** (the
  320-byte buffer is 2× over-allocated but safe); fix that comment when
  surfacing capture groups. The `:1363` comment ("16 bytes") is correct.
- **`case` `=glob` → match/glob builtin** is resolved by **DD#66**, which
  removes `case`/`esac` entirely (so there is no `case` glob operand left
  to desugar). Confirmed under DD#66.

## Default/replacement context unification (DG, 2026-06-21)

~~The `sub`/`gsub` **replacement** and the `${var:-default}` **default** are
the **same interpolating word context**, differing in exactly one way: the
replacement runs an extra **`\0`–`\9` backref template pass** over the
evaluated string; the default has no second pass.~~ DD65 later makes defaults
literal-only; replacement words retain the interpolation rules below.
In that ~~shared~~ context:

- `$var`/`${var}` interpolate; `'…'` literal; `"…"` interpolating;
  **`$'…'` C-string works** (DD#64 addendum — control bytes/escapes in
  place, no concatenation); pieces concatenate. The sigils don't collide:
  `$` = interpolation (word layer), `\` = backref (template layer, replacement
  only).
- **`$(cmd)` is allowed** (not rejected): an external capture is tainted and
  needs ~~bless,~~ now attest, and a user-function capture is tainted as DD#127 says —
  taint handles both, ~~same as a `${var:-…}` default (DD#65)~~. Arithmetic
  `$((…))` allowed.

**Desugar correction.** The `s/RE/SUB/` sugar must desugar its fields to
this **word context**, NOT to literal-double-quoted args. The earlier
"fields desugar to double-quoted `\"RE\"`/`\"SUB\"`" wording would make
`$'…'` (and the intended interpolation) behave differently in the sugar
than in the `$(sub …)` function form; desugaring to a word context makes
**sugar ≡ function form**. (Path B still governs `/`-delimiting of the
`${var:s/…}` payload at the lexer.)

**Lexer note.** The `${…}` brace scanner and the `:s/` Path-B `/`-scanner
must skip a nested `$'…'` by **C-string (escape-aware) rules** so an
escaped delimiter inside (`$'\}'`, `$'\/'`, `$'\''`) does not terminate the
field. (Today the brace scanner skips `$'…'` with literal single-quote
rules — `skip_balanced_squote`, lexer.rs:658 — a latent bug to fix when
this lands.)

See task `regex-interpolation-delimiters` for the spec-first
implementation; `dd62-test-expr-ssavalue` for the overlapping operand
work (the `=~`-as-`match`-command desugaring is the seam between them).
## Addendum (2026-09-09) — `matchall S P ARR`

DG: *"can we use match instead of read?"* — asked while the DD#99 `mapfile`
migration was stuck. It was the right question: the blocker
(`bug-regex-blind-past-interior-nul`) was **self-inflicted** by the
substitute-separator-to-NUL-then-split approach. `matchall` extracts from the
raw text, so no NUL is ever created and the bug is never reached.

`match` is single-shot. `matchall S P ARR` fills ARR from **every** match:

> each match contributes its **capture groups**, or the **whole match** when
> the pattern has no groups.

That one rule serves both shapes the migration needs:

<del>

```sh
matchall $s "[^\x0a]+" lines                      # mapfile -t : the lines
matchall $s "([^=\x0a]*)=([^\x0a]*)" pairs       # mapfile -m : k1 v1 k2 v2 …
map m = @{pairs:~}
```

</del>

These historical calls omit result handling; use a legal consumer or
`- matchall` when the array fill is the intended effect (DD70).

The second lands exactly on the pairwise array `map m = @{a:DISP}` already
consumes (DD#99) — the two features fit without adapting either. It also
expresses what the NUL route could not: only the **first** `=` of a record
separates, so `key=val=ue` keeps its value intact.

### Deliberate differences from `match`

- **The whole match is omitted when there are groups.** `match` yields
  `[whole, g1..gN]`; prepending the whole here would interleave a third
  element per record and destroy the pairing. With groups, you asked for the
  groups.
- **The out-array is mandatory.** There is no boolean/value 2-arg form,
  because "all matches" has no single value to be.
- **The target auto-declares**, following the argv / read / newread bulk-fill
  precedent (DG 2026-07-08). ~~Singular `match` does not; that asymmetry is a
  filed bug (`bug-match-outarray-no-autodeclare`), not a rule.~~

Boolean like `match`: exit 0 when at least one match. The array is cleared on
entry in the shipped model, so the no-match path leaves it empty.
DD131's trial instead appends this batch to an ordinary target, or
constructs a fresh immutable scratch batch. No match appends nothing
or leaves the new scratch empty, and is false regardless of prior
destination contents. An operation error preserves an ordinary target
but cannot resurrect a preceding scratch instance.

### Arity is now enforced

`match` validated neither its upper arity nor the 1-argument form:
`match S P ARR EXTRA` compiled, reported **success**, and filled no array — so
an `if` took the then-branch on a match that never happened. The 3-arg
out-array lowering declined the shape and the 2-arg boolean lowering picked it
up, ignoring the surplus. Both `match` (2 or 3) and `matchall` (exactly 3) are
now checked (SB4045).

### Implementation

One `SsaInst::MatchInto { …, all: bool }` rather than a parallel variant — the
forms differ only in which runtime symbol is called, so splitting them would
mean two of every downstream arm to keep in step. Runtime:
`sash_regex_matchall_into`; the janet helper gains a `captures-all`
subcommand. A zero-width match advances one byte, or the loop would spin.

Specs: `regex.sh` 63.ar1-ar4 (arity), 63.ma1-ma9.



### The out-array form auto-declares its target (2026-09-11)

The out-array form was specified as "consistent with `mapfile -m m` / `read -a
a`", but only `matchall` carried the auto-declare half across; singular `match
S P ARR` left the name unregistered, so a following `len g` drew SB5039
("introduced nowhere") on a name introduced two lines above.

Both forms now register the out-array, in all five places that dispatch on it:
`collect_read_targets` and `command_writes_args` (validate), `check_command`
(type_check), `try_handle_match_into` (transform), and the two janet match
emitters. Every site is guarded on a 3-argument call, so the boolean form still
binds nothing.

janet is gated off the new specs for an unrelated pre-existing bug — it writes
the auto-declaring `var` inside the if-condition expression
(`janet-match-outarray-var-in-condition`), which reproduces with `matchall` on
untouched code.

Specs: `regex.sh` 63.mi1-63.mi5.

## Addendum (2026-09-13) — the subject stops at its first interior NUL

Documenting a boundary DG decided to keep (2026-09-10, YAGNI). Not a change
in behavior; it has always been this way.

Every regex operation in this family — `[[ =~ ]]`, `match`, `matchall`,
`sub`, `gsub`, `${x:s/…/…/}`, `x =~ s/…/…/` — reads its subject as a **C
string** and examines it only **up to the first interior NUL**. Bytes past
that are neither matched nor replaced, and are preserved unchanged. The
operation reports success.

<del>

```sh
nul=$'\0'
s="a${nul}bcd"
echo "$(strlen $s)"        # 5 — the scalar is byte-transparent
[[ $s =~ cd ]]             # false; the bytes are there
u=$(gsub $s "cd" "Z")      # unchanged, strlen 5
```

</del>

The NUL example's `$'\0'` spelling is superseded by DD115's `$'\x00'`;
the bare test also needs a consumer (DD70).

The length is not lost at the runtime boundary — `sash_regex_replace` and the
`sash_regex_match*` family all receive an explicit `str_len` and build a
correct slice. The truncation is one level down, in the Plan 9
`regcomp9`/`regexec9` API, which takes a `char *` and has no length parameter.

**Like the POSIX bracket classes, this boundary does not announce itself.**
Everything else the engine cannot do is rejected (SB5023); this returns a
wrong answer quietly. That is why it is stated here and in
`docs/sash.1` (§REGULAR EXPRESSIONS, "Interior NUL bytes") rather than left
to be discovered.

Coherent with DD#99, where a NUL is the **record separator**: text within one
record does not contain one, and a scalar that holds NULs is mid-split. The
operation to reach for there is `read` or `matchall`, which work on the
fields, not `=~` on the joined subject. `matchall` (above) is what removed
this from the critical path — the DD#99 `mapfile -m` migration routed around
it rather than through it.

If it is ever reconsidered, the candidate fix is **segment on NUL**: split the
subject at NULs, run the regex per segment, rejoin with the NULs intact —
contained to `sash-rt/src/lib.rs`, and it makes a NUL a hard match boundary
rather than merely a stopping point. Care needed so a non-global `s///` still
means "first match across the whole subject". Full discussion:
`notes/tasks/limitation-regex-stops-at-interior-nul.md`.

## Addendum (2026-10-31) — the `s///` expansion sugar in an attest predicate

An attest predicate runs no code (DD#24's 2026-10-31 addendum, SB5098), and
`${var:s/RE/SUB/}` *is* `$(sub $var "RE" "SUB")` (The thin sugar, above), so
the sugar is rejected there as well. The desugaring leaves no mark in the
compiler's tree, so the message names both spellings: "`$(sub …)` runs `sub`
(a `${…:s/…/…/}` expansion is that call)", and the `/g` form for `gsub`.
`[[ $var =~ RE ]]` stays legal in a predicate. It is a regex test, which
DD#24 allows by name, and it reaches the predicate as a test node, not as a
capture. Specs taint.sh atpred.26 (`gsub` in a `[[ ]]` predicate),
atpred.31 (`sub` in a Part D value's `${…}` import) and atpred.28 (the `=~`
control). Journal 552.
