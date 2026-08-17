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
  no longer a comment.
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

## See also
- DD#26 — integer overflow traps (Policy A); out-of-range literal → SB4026.
- DD#25 — arithmetic overflow hardening.
- DD#77 — arith string→int conversion discipline (supersedes bare coercion).
