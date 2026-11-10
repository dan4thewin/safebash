title: DD#64 — No shell-level backslash escaping; quoting + concatenation instead
status: accepted

The shell interprets backslash in **exactly two** places — `$'…'`
(C-escapes) and `\` at end-of-line (line continuation). Everywhere else —
unquoted, `'…'`, `"…"` — `\` is a verbatim literal byte. Escaping shell
metacharacters is done by **quoting and string concatenation**, not
backslashes. From the 2026-06-17/18 discussion.

## Ratified (DG, 2026-06-21)

Accepted as written, **including** the breaking change that inside `"…"`
`\\` no longer collapses to one backslash and there is no `\"`/`\$` — a
deliberate divergence from bash, "judged worth the consistency." The core
(`scan_dq_escape`) is already implemented; the in-`${var:OP}` backslash
handlers are removed by DD#63 (Path B) and DD#65 (operator drops). No
open design items.

## Implementation status (2026-06-19)

**Done** — the core change is `lexer.rs::scan_dq_escape`: inside `"…"`,
`\` is now a verbatim byte (only `\<newline>` continuation survives), so
`"a\\b"`→`a\\b`, `"a\$x"`→`a\`+interp, `"a\"…`→`a\`+close. Spec tests
`spec/wordsplit.sh` 64.1–64.3. Verified zero net regression (native specs
417/2 unchanged; rust clean baseline 186, only flaky `oracle_pipeline`).

**Already compliant (no change needed)** — bare words (`scan_word_chars`
pushes `\` verbatim and breaks at spaces, so `a\b`→`a\b`, `a\ b`→two
words) and regex-within-a-word (`a\.b` tokenises as one `Word`, the `\`
survives). The DD over-counted these as removals.

## Amendment (2026-09-16) — the EOL half was missing from bare words

"Already compliant" above was right about `a\b` and wrong about `a\<newline>`.
`scan_word_chars` had no `\` arm at all, so it pushed the backslash verbatim
and then broke on the newline: a word split across a line continuation became
**two statements**, silently.

```sh
/bin/echo start\
/bin/echo second
```

bash prints `start/bin/echo second`. sash compiled clean and printed two
lines. `skip_whitespace` (between tokens) and `scan_dq_escape` (inside `"…"`)
both implemented the continuation; the bare-word path was the one site that
did not, so the rule above — "`\` at EOL — line continuation. Kept
(structural)" — was only enforced in two of its three contexts.

Fixed by adding the arm to `scan_word_chars`. The *non*-EOL divergence is
unchanged and still deliberate: `a\b` is `a\b` in sash and `ab` in bash,
because unquoted `\` is a verbatim byte everywhere except end-of-line.

### Second amendment (same day) — end-of-input is end-of-line too

The first amendment fixed `\<newline>` and left a trailing `\` with *nothing*
after it as a verbatim byte. bash discards it — `echo a\` with no final
newline prints `a`, not `a\` — and "end-of-line" above covers the last line
whether or not it is newline-terminated. Both `scan_word_chars` and
`skip_whitespace` now treat `\`-then-newline and `\`-then-nothing the same.

This was caught by a spec that had *asserted the wrong answer*: 64.15 was
written from the implementation rather than from bash, and passed. See journal
329.

Tests: `spec/wordsplit.sh` 64.9–64.18. No mode guard is needed — the answer is
the same with or without a trailing newline, which is what makes it correct.

**Historical deferral, resolved below** — the brace-scan `\}` (lexer.rs:660,
plus the in-`"…"` `${}` skip at ~824 and `skip_balanced_dquote`) and the
`/`-operator `\/` (`find_unescaped_slash`). These are `${var:OP}`
brace/glob-matching machinery removed when Path B (DD#63) and the
`/`-glob-replace drop (DD#65) land.

> Scope: this covers backslash in *string* and *regex-argument* contexts.
> Backslash *inside* `${var:OP}` expansions — the brace-scan `\}` and the
> `/`-operator `\/` (`find_unescaped_slash`) — is **deferred to the
> var-modifier vet** (`vet-var-modifiers`), since several of those
> operators may be dropped.

## Why

Backslash is currently handled in ~7 inconsistent, context-dependent ways
(line-continuation, brace-scan skip, `scan_dq_escape`, single-quote
verbatim, ANSI-C, word-char, `find_unescaped_slash`), producing warts:
the `"\\"`→`\` collapse and the awful `"\\\\"` for one backslash; `a\ b`
not equal to `'a b'`; `\/` broken in `:s/`; a standalone `\` silently
dropped on `=~`. Collapsing to "verbatim except `$'…'` and EOL" removes
the warts and the brittle code.

## The rule

- **`$'…'`** — C-escapes (`\n`, `\t`, `\xNN`, …). Kept; the one place the
  *shell* reads backslash escapes. **The exact set is DD#115's** — this DD
  decides only *where* escapes are read, not which ones.
- **`\` at EOL** — line continuation. Kept (structural).
- **unquoted / `'…'` / `"…"`** — `\` is a verbatim literal byte. No `\"`,
  no `\$`, no `\\`-collapse, no `\ `, no word-char escaping. `'…'` and
  `"…"` differ *only* by `$`-interpolation.

Two backslash *interpreters* remain, both downstream and contextual:
`$'…'` (shell C-escapes) and the regex engine (`\.`, `\\`). The shell
proper interprets none.

## Non-expanding dollars (DG, 2026-11-10)

Keep the compiler's existing literal fallback: a `$` in an unquoted word
or double-quoted string that starts no recognized expansion remains literal.
In particular, `$-` is the two characters `$-`, not Bash's shell-option
flags. `"$%"` and `"cost: $"` retain their literal dollars too; no general
stray-dollar rejection is added to words.

Intentional literal text can use `'$-'` or a C-string. This does not add
a `\$` escape: backslash retains the rules above. DD#65's `:-` defaults
still follow double-quoted-body interpolation.

The fallback does not turn malformed recognized forms into text: existing
`${…}`, `$(…)` and `$'…'` diagnostics remain. In particular, native `${-}`
keeps its existing rejection; it does not become an alternative spelling
of literal `$-`. DD#72's stray-dollar rejection is arithmetic-specific,
and DD#63's `s///` fields retain their separate regex-anchor rule.

The language boundary belongs in `sash-convert`: when Bash source actually
expands its option flags, `$-` or `${-}` gets DD#57's ordinary `# MANUAL:`
annotation and pass-through for unsupported bashisms, not a new refusal.
Literal occurrences are not option-flags expansions. Converter implementation
is queued in `q-word-dollar-dash`; this recording preserves compiler behavior.

## Addendum (DG, 2026-06-21): `$'…'` expands inside `"…"`

A `$'…'` C-string is recognized **inside double quotes** as a
compile-time-constant expansion, alongside `$var`/`${var}`/`$(…)`/`$((…))`.
So inside `"…"` **every** `$`-prefixed form expands — `$'…'` is no longer
the odd one out. `"a$'\n'b"` → `a`⏎`b` (bash leaves it literal; this is a
deliberate, capability-adding divergence).

**Why.** DD#64 made `\` verbatim in `"…"`, so there was no in-place way to
put a control byte (newline/tab) into an interpolated string — only
concatenation (`"a"$'\n'"b"`) or a variable. This restores it via the one
sanctioned escape syntax, reinforcing (not weakening) DD#64: *escapes live
only in `$'…'`, now usable everywhere including inside `"…"`.*

**Universal escape hatch.** Because `$'…'` is escape-aware and can encode
every structural char (`$'\x22'`→`"`, `$'\x24'`→`$`, `$'\''`→`'`,
`$'\\'`→`\`, any byte via `\xNN`), a C-string nested in `"…"` gives
**complete literal-byte control inside an interpolated string with no
concatenation** — even a literal `"` (`"a$'\x22'b"` → `a"b`), since the
C-string region is opaque to the double-quote scanner. So `"…"` + `$'…'`
together express any byte string *with* interpolation.

**`$'` claims its sequence.** Inside `"…"`, the adjacency `$'` always
opens a C-string (exactly as `${`/`$(` claim theirs). A literal `$'` in a
double-quoted string is therefore encoded via the C-string itself
(`"$'$\''"` → `$'`). A lone `'` (not preceded by `$`) stays a literal
apostrophe (`"it's"` → `it's`). Literal `$'` text is rare; the hatch is
self-contained.

**Implementation.** `scan_double_quoted`, on `$'`, hands off to the
existing **escape-aware** `scan_ansi_c_quoted` (verified: `$'a\'b'` →
`a'b`), which finds the close `'` past any `\'`; the bytes become a literal
part of the double-quoted concat. A C-string is a compile-time constant →
always **clean** (no taint).

**Resolves the default/replacement context (DD#63/DD#65).** Because a
`$'…'` is recognized wherever a `$`-form is, it works uniformly in `"…"`,
in `${var:-default}`, and in the `sub`/`gsub` replacement. There is no
"word context (C-string lives) vs double-quoted (C-string inert)"
split — they are the same interpolating context (see DD#65/DD#63).

## Escaping by quoting + concatenation

Adjacent single-quoted literals and interpolations concatenate (verified:
`'"'$foo'"'` → `"bar"`, `'$'X` → `$X`, `'<'$x'>'` → `<hi>`), which
replaces every backslash escape:

| want | write |
|------|-------|
| literal `"` | `'"'` |
| literal `$` | `'$'` |
| literal `\` | `'\'` (or verbatim in any quote) |
| a space | `'a b'` / `' '` |
| literal byte in a regex (space, `/`, `}`, `\`) | `\xNN` (`\x7d` = `}`, `\x5c` = `\`) |
| newline etc. | `$'\n'` |

## Consequences

- `"a\\b"` → `a\\b` (two literal backslashes; **no collapse**) — a
  breaking change from current double-quote semantics. sash is not
  bash-compatible; the change is judged worth the consistency.
- A regex in single quotes is *standard regex escaping*, untouched by the
  shell: `'a\.b'`, `'a\\b'`, `'\(x\)'`. The recommended form.
- In double quotes (for interpolation) `\.`/`\(` still reach the engine
  verbatim; a literal backslash is `\x5c`, never `"\\\\"`.
- Removes `scan_dq_escape`, the `parse_regex_rhs` standalone-`\` drop, and
  the word-char escape. (`$'…'` and EOL handlers stay.)

## Resolved by the var-modifier vet (DD#65) + Path B (DD#63)

How `\` behaves *inside* `${var:OP}` operands is now settled, and **both**
in-operand backslash handlers are removed:

- The `/`-operator `\/` (`find_unescaped_slash`, expr.rs:741) goes with the
  `/`-glob-replace op itself — DD#65 **drops** `${v/a/b}` (redundant with
  `sub`/`gsub`), which deletes `find_unescaped_slash`.
- The brace-scan `\}` (lexer.rs:660) is **removed outright** — no kept
  operator needs it. `:s/…}` protects a bare `{`/`}` via **Path B**
  (DD#63: the lexer treats `/` as a quote-delimiter after `:s`); `:-`
  includes a literal `}` by **quoting** (`${foo:-'a}b'}` → `a}b`, since the
  lexer already skips `'…'`); `:off:len` takes integer operands. A literal
  `}` that still needs a byte escape is `\x7d` (DD#63 `\xnn`), but it is no
  longer load-bearing for either kept operator.

So the `${var:OP}` family is fully clean under "no shell backslash": the
only backslash interpreters left anywhere are `$'…'`, `\`-at-EOL, and the
regex engine.

## Addendum (DG, 2026-09-19): the escape set is narrowed — see DD#115

The "any byte via `\xNN`" and "complete literal-byte control" claims above
are **narrowed** to *any ASCII byte, and any character via `\uHHHH`*.

They were never true. `\xNN` for `NN` ≥ 0x80 emitted `ef bf bd` (U+FFFD),
silently — the lexer accumulated bytes and then decoded them lossily at the
token boundary, and a Rust `String` cannot hold a lone `0xFF`. Rather than
plumb bytes through 127 `ExprKind::Literal` sites for a capability ruled
YAGNI, `\xNN` is capped at `\x7f` and `\uHHHH`/`\UHHHHHHHH` were added.

**The surrounding argument is untouched.** Every structural character this
section lists — `"` `$` `'` `\` — is ASCII, so the escape-hatch table still
holds exactly as written, and `"…"` + `$'…'` still expresses any *text*
string with interpolation. What it no longer expresses is a byte that is not
text; for that the runtime is byte-clean and the data paths (`read`, `arg`,
`getenv`, capture) carry arbitrary bytes intact.

Also retired from the set: octal (`\101`, `\0101`, **`\0`**) and `\cX`.

## Enforcement correction (2026-10-16): quoted capture boundaries

The private `"…$(…)…"` boundary skipper survived the earlier removals and
still treated `\"` as an escaped quote. It also did not recognize `$'…'`
escapes or C-string interpolation inside a nested `"…"`. Valid captured
programs therefore drew spurious SB4057 errors.

Capture boundaries now use the actual double-quote and C-string scanners;
there is no separate backslash grammar. Comments use the ordinary comment
boundary rule and scanner too, so quotes and parentheses in a comment cannot
change the capture boundary, while `a#b` remains a word. This enforces the
existing rules, not a new escape set. `spec/wordsplit.sh` `dqbound.1`-`dqbound.9`
pin the outputs; `spec/errors.sh` `lexcold.4`-`lexcold.5` pin first-error
preservation through nested captures.

## See also
- DD#115 — the `$'…'` escape set; narrows the escape-hatch claim above.
- DD#63 — regex builtins; `\` in regex args follows this DD.
- DD#62 — `[[ ]]` simplification.
