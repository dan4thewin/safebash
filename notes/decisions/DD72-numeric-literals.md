title: DD#72 — Numeric literals in arithmetic
status: accepted

Numeric literals in `$(( ))` had no governing decision. The arith lexer
accepted C-octal (`0377`), hex (`0x`), and bash arbitrary-base
(`base#digits`), but `base#` and the modern prefixes silently miscompiled
(`16#ff` → 16, `0o10`/`0b1010` → 0). Decided 2026-07-05 (DG).

## Model — modern prefixes only

A numeric value has two origins, handled differently:

### Constants (source literals written inside `$(( ))`)

Accepted:
- **decimal** — `42`, `0`, `-5`
- **`0x` hex** — `0xff`, `0XFF`
- **`0o` octal** — `0o377`, `0O377`
- **`0b` binary** — `0b1010`, `0B1010`

Rejected with a clean diagnostic (never silent):
- **leading-zero integers** — `010`, `08`, `0377`. A leading `0` followed
  by more digits is ambiguous bash-octal creep; the author must write
  `0o…` for octal or drop the zero for decimal. (`0` alone is decimal zero.)
- **bash arbitrary base** `base#digits` — `16#ff`, `2#1010`, `10#42`.
  Dropped bash creep; use `0x`/`0o`/`0b`. This was the direct cause of the
  `#`-as-comment collision.
- **invalid digits for the prefix** — `0o8`, `0b2`, `0x` (no digits).

Rationale: the constant is *authored*, so strictness forces the writer to
say exactly what they mean — a bash script whose `010` meant octal-8 gets a
compile error, never a silent value change. Output is always decimal, so
literal *input* syntax has no effect on byte-identical stdout — sash is free
to pick a clean, minimal set.

### Runtime string→int coercion (a variable's/data's value used in arith)

`atoi_inner` / `sash_atoi`. Lenient, because data is not authored:
- **honors `0x`/`0o`/`0b` prefixes** — `x=0xff; $((x+0))` → 255.
- **otherwise decimal, leading zeros ignored** — `x=010; $((x+0))` → 10,
  `x=08` → 8 (NOT octal). A non-prefixed `base#`-looking value parses its
  leading decimal digits atoi-style (`"16#ff"` → 16).

Rationale: data from `read`/`$(…)`/argv/env can't be required to be clean;
plain-decimal leniency is robust and makes bash's `10#$var` decimal-forcing
idiom unnecessary (sash never octal-traps `08`/`010` from `date`/`read`).

The only constant/string divergence is the leading-zero case: strict-reject
for constants, lenient-decimal for strings. Prefix handling is identical.

## Also fixed (silent-error sites this surfaced)

- The main scanner lexed `#` as a comment even inside `$(( ))`, truncating
  `16#ff` → `16` before the arith number scanner ran. `#` in arith mode is
  no longer a comment. **(Half a fix — see DD#110.)** The same truncation was
  happening *outside* arith, where `/bin/echo 16#ff` printed `16`; that went
  unnoticed for two months because the guard written here was scoped to
  `arith_depth > 0`. DD#110 makes the rule general: `#` opens a comment only
  after a blank.
- `tokenize_arith` silently *skipped* unknown bytes, and the parser
  silently recovered an unknown token as `0`. Both now produce a clean
  reject (zero-tolerance: no silent arith miscompiles).

## Amendment (DD#77, 2026-08-17) — a bare string operand no longer coerces

DD#77 (arith string→int discipline) **supersedes the bare-operand path** of
the runtime coercion above. A *bare* string operand in `$(( ))` — `x=0xff;
$(( x + 0 ))`, `s=010; $(( s ))` — is now a **compile error** (SB4030, DD#77
D1), not a silent lenient coercion. The string must carry a disposition:
`$(( ${x:?msg} + 0 ))` (abort if not a valid integer) or `$(( ${x:-N} + 0 ))`
(default N).

What survives unchanged: the lenient *success grammar* above still defines
what a **valid integer** *is* — it is exactly what `parse_strict_i64` accepts
(0x/0o/0b honored; decimal with leading zeros ignored; out-of-range → invalid).
The disposition's `is_int` gate and its `sash_atoi_or`/`sash_atoi_or_abort`
conversion consume that same grammar. So DD#72 still governs *validity*; DD#77
governs *how a string crosses into int* (never silently). The old
`arith_literals.sh` bare-`$((x))` coercion tests (`nl.s*`) migrated to
dispositions accordingly.

## Addendum (2026-10-24, batch-531 port step F14) — a `$` that starts no variable is a stray byte

In arithmetic a `$` opens a variable: `${…}`, a special (`$#` `$?` `$$` `$!`
`$@` `$*`), or a name. `scan_arith_dollar` scanned a name after every other
`$` too, and when no name followed it scanned an **empty** one: a read of the
variable called ``, which is 0. "Also fixed" above closed this
silent-miscompile shape for unknown bytes; a `$` reached it by another route
(batch-531 B70, the arithmetic half). On the base binary, in both backends:

- `echo $(( $ + 1 ))` printed `1`;
- `echo $(( $- + 1 ))` printed `-1` (sash has no `$-`: this is the empty read,
  then `- +1`);
- `(( x = $ ))` assigned 0;
- `${a[$(( $ ))]}` read `a[0]` natively, and its C failed to compile.

**The rule.** A `$` in arithmetic must start a variable. One that does not is
a stray byte, **SB4035** "stray `$` in arithmetic", raised by
`arith_bad_literal_error`, which already gives DD#123's stray bracket that
code. Only the `$` is consumed, so the token stream stays balanced.

**`$(` gets its own message.** A nested `$( … )` or `$(( … ))` is not an
arithmetic operand. Base said so only as SB4020 "unexpected token after
arithmetic expression", about the `(` after the `$`. It is now **SB4035** "a
nested `$( … )` or `$(( … ))` is not an arithmetic operand", and the message
names the routes that work: capture into a variable first (a string capture
converts with a disposition, `${s:-0}`), call an int function as `(f)`
(DD#58), and group with plain `( … )`.

**Re-coded specs** (same programs, same ids):

- `31.46` `$(( $(getpid) > 0 ))`, `31.85` `(( x = $(echo 2) + 1 ))` and
  `cov.356` `$(( $((1)) + 2 ))`: SB4020 → SB4035, "is not an arithmetic
  operand";
- `31.67` `(( x = 1 $'a' ))`: SB4020 → SB4035, "stray `$`";
- `75.164` `${arr[$(( $(f) ))]}`: SB4041 → SB4035, through DD#80's addendum of
  the same date, which stops an explicit index's fallback from swallowing this
  error.

An empty *braced* name in arithmetic (`$(( ${} + 1 ))`) falls under the word
rule rather than this one: DD#65's addendum of the same date.

Specs: `31.173`–`31.175` (arithmetic.sh) and `80.134` (subscript.sh). Journal
533.

## See also
- DD#26 — integer overflow traps (Policy A); out-of-range literal → SB4026.
- DD#25 — arithmetic overflow hardening.
- DD#77 — arith string→int conversion discipline (supersedes bare coercion).
