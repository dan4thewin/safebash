title: DD#63 — Regex quoting, interpolation, and delimiters
status: candidate

The three regex surfaces — `[[ $x =~ re ]]` (match), `${var:s/re/abc/f}`
(expression replace), `$var =~ s/re/abc/f` (mutation) — get one
consistent quoting/interpolation/delimiter model. From the 2026-06-17
discussion. (SUBSET.md and DD#39's regex prose are stale; this DD is
authoritative for the surface syntax.)

## Reality check (discovered while designing this)

**Field interpolation is currently UNIMPLEMENTED on every surface** —
`$var` in a pattern or replacement field is taken literally:

| input | today | should be |
|-------|-------|-----------|
| `[[ $x =~ $pat ]]` (pat=`a.*b`, x=`axxb`) | `N` (literal `$pat`) | match |
| `[[ $x =~ ${pat} ]]` | **SB4014** (doesn't parse) | match |
| `${v:s/$x/Y/}` (x=`b`, v=`abc`) | `abc` (no sub) | `aYc` |
| `${v:s/b/$r/}` (r=`Y`) | `a$rc` (literal `$r`) | `aYc` |
| `v =~ s/$x/Y/` | `sash: bug:` runtime abort | `aYc` |

Only the **subject/LHS** of `=~` interpolates. So DD#39's "Dynamic
patterns (containing `$var`) are compiled at runtime" was aspirational.
(DD#39 is also stale on `{n,m}`: counted repetition now works —
`[[ aaa =~ ^a{2,3}$ ]]` matches.)

## Decisions

### 1. Interpolation is a textual first pass

A pattern/replacement field is **a string first, a regex only when it
lands in the engine**. `$name` and `${name}` interpolate exactly as
everywhere else in sash; the resulting string is then compiled as a
regex (dynamic patterns compile at runtime, so a malformed interpolated
regex is a *runtime* error). `foo='a.*b'; [[ $x =~ $foo ]]` is identical
to `[[ $x =~ a.*b ]]`. `a.*$suf` interpolates `$suf` then compiles.

- **Anchors fall out for free.** `$` *not* followed by a name char (or
  at end) is a literal regex anchor: `foo$`, `^foo$`, `a$|b`. Only `$`
  immediately followed by a name char interpolates — and a literal `$`
  before a name char is a degenerate regex (anchor-then-more-chars never
  matches), so nothing meaningful is lost.
- **`${name}` is unambiguous even with `{n,m}`** — an anchor cannot be
  quantified, so `${…}` can only be interpolation.
- **Metachar collisions are the user's concern.** An interpolated value
  containing `.` `*` `[` is active regex. To match a value literally,
  use `''` (below) or `fgrep`/`grep -F` (decision #2).
- **`'…'` disables interpolation** — a single-quoted field is fully
  literal (`'$pat'` is the literal text `$pat`). `\` still reaches the
  engine in `'…'` (single-quote suppresses `$`, not `\`).

### 2. Literal containment is not a regex concern

With `==` exact (DD#62) and `=~` regex, there is deliberately **no
literal-regex mode**. To test "does `$x` contain the literal value of
`$needle`", use `fgrep` / `grep -F "$needle"`. sash makes this
compromise for a smaller, consistent surface.

## Proposed (pending the #3–#7 walkthrough — not yet locked)

- **Delimiters for `s///`:** the char after `s` is the delimiter — `/`,
  `"`, or `'`. `/` and `"` interpolate; `'` does not. **No delimiter
  escape:** to include the delimiter char in a field, *switch
  delimiters* (`s"a/b"c"`), don't escape it. This works because
  **delimiter parsing is compile-time on the literal source; interpolation
  is runtime** — a delimiter char *inside an interpolated value* is data,
  never a delimiter.
- **`\` passes verbatim to the engine** — no parser-level escape. `\.`,
  `\\`, etc. are the engine's concern.

## Open (to walk)

- #4 Match form: is `/re/` a delimiter or literal slashes? (Lean:
  `/` is a delimiter only in `s///`; in `=~`, `/re/` = literal slashes.)
- #5 A literal `}` inside `${var:s…}` closes the expansion early
  (brace-scan limitation; the statement form is immune).
- #6 Confirm single-quote-still-engine-escapes-`\` is the intended,
  documented surprise.
- Replacement field: require `${…}` for uniformity, or allow bare `$var`
  (no anchor ambiguity there)?
- `\` final rule once #3 is locked.

See task `regex-interpolation-delimiters` for the spec-first
implementation; `dd62-test-expr-ssavalue` for the overlapping `=~`
operand work.
