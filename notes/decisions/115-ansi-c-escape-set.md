---
status: accepted
---

# DD#115 — the `$'…'` escape set

Decided 2026-09-19 (DG). `$'…'` had no governing decision: the escape
set was whatever the lexer happened to implement, and it diverged from
bash **silently** in five ways. Found by the coverage grind (journal 371)
— `simple_escape` was a near-entirely cold table.

## The problem

| written | bash | sash (before) |
|---|---|---|
| `\xff`, `\x80`, any `\xNN` ≥ 0x80 | that byte | **`ef bf bd`** (U+FFFD) |
| `\0012` | `\001` `2` | `\n` |
| `\0777` | `?` `7` | mangled |
| `\xzz` | literal `\xzz` | a NUL byte |
| `\101`, `\777` | `A`, `0xff` | literal `\101` |
| `\u00e9`, `\U0001F600`, `\cA` | é, 😀, `\001` | literal text |

Every row is silent. The first is the worst, and it contradicts a
**ratified** claim: DD#64 removed every other backslash handler and
designated `$'…'` the universal escape hatch — *"any byte via `\xNN`"*,
*"complete literal-byte control inside an interpolated string"*. That
promise was false for half the byte range.

Root cause of that row: `scan_ansi_c_bytes` carefully accumulates a
`Vec<u8>` — its comment explained exactly why bytes matter — and then
`scan_ansi_c_quoted` did `String::from_utf8_lossy` at the token boundary.
A Rust `String` cannot hold a lone `0xFF`.

Note the inconsistency this created: a raw `0xff` byte **in the source
file** is rejected *loudly* (`stream did not contain valid UTF-8`), while
the sanctioned way to write that same byte corrupted *silently*.

## The model — DD#72, applied to escapes

`$'…'` escapes are **authored constants**, so DD#72's rule governs:
*strictness forces the writer to say exactly what they mean*, and an
archaic bash spelling is a clean diagnostic, never a silent difference.

The parallel is exact. **`\0NNN` is the `010` of the escape world** —
ambiguous octal that sash implemented and got subtly wrong, just as
`16#ff` silently truncated to `16`.

### Accepted

- **named** — `\n` `\t` `\r` `\a` `\b` `\e` `\E` `\f` `\v` `\\` `\'` `\"`.
  All below 0x80, all universally understood.
- **`\xNN`** — one to two hex digits, naming a **byte**. Capped at `\x7f`.
- **`\uHHHH` / `\UHHHHHHHH`** — up to four / eight hex digits, naming a
  **character**, emitted as UTF-8. *New in this DD.*
- **anything bash does not read as an escape** (`\q`, `\z`) stays a
  literal `\<byte>`, exactly as bash leaves it. No divergence, so no
  reason to reject.

### Rejected — SB4063, never silent

- **octal** — `\101`, `\0101`, `\012`, `\777`, **and `\0`**. Dropped bash
  creep; write the byte as `\xNN` or the character as `\uHHHH`.
- **`\cX`** — archaic control notation. `\cA` is `\x01`.
- **`\xNN` above `\x7f`** — a lone byte above ASCII is not UTF-8 text.
- **a prefix with no digits** — `\x`, `\u`. Same as DD#72 rejecting `0x`.
- **`\uHHHH` that is not a character** — a surrogate (`D800`-`DFFF`) or
  above `10FFFF`. bash encodes these anyway and emits **invalid UTF-8**.

## Why `\xNN` is capped but `\u` is not

They name different things. `\xNN` names a *byte*; a lone byte above
ASCII is not text, and sash strings are UTF-8 text. `\uHHHH` names a
*character*, and its UTF-8 encoding is valid by construction. So the cap
costs nothing that `\u` does not express better — `$'caf\xc3\xa9'`
becomes `$'caf\u00e9'`, which says what it means.

## What this deliberately gives up

**Authoring a lone byte ≥ 0x80 inline.** Ruled YAGNI (DG). The two
affected use cases are binary magic (`\x89PNG`) and deliberately-malformed
UTF-8 test fixtures.

This is narrow because **the runtime is byte-clean** — measured: `read`,
`arg`, `getenv`, `$(…)` capture and external passthrough all carry
`61 ff 62` intact. The UTF-8 constraint binds *only* the source-literal
boundary, so data flowing through a sash program is unaffected. Nothing
that is genuinely *text* is lost either: type it in the UTF-8 source, or
write `\uHHHH`.

If it is ever wanted, the route is a byte-literal IR node used only when a
`$'…'` contains a high byte (~10-15 sites), not `String` → bytes across
all 127 `ExprKind::Literal` sites.

## Amends DD#64

DD#64's *"any byte via `\xNN`"* and *"complete literal-byte control"* are
narrowed to **any ASCII byte, and any character via `\uHHHH`**. The
surrounding argument is untouched: escapes still live only in `$'…'`, and
`$'…'` still works everywhere a `$`-form does, including inside `"…"`.
The escape-hatch table in DD#64 still holds — every structural character
it lists (`"` `$` `'` `\`) is ASCII.

## `\0` goes too

`$'\0'` is the octal family's most-used member by far — ten spec sites, all
of them NUL-handling: word-split separators, and the DD#55 argv-NUL abort.
It was not named when this was decided, and it is the one casualty worth
stating plainly.

It still goes. **`\0` *is* the octal escape**, so keeping it would be a
carve-out that contradicts the rule rather than an application of it, and the
carve-out is not even clean: `\0` would be accepted while `\01` — which bash
reads as octal 1 — stayed an error. `\x00` is an exact replacement, verified
to still build the interior NUL and still fire the DD#55 abort.

The general principle, worth keeping: *a popular spelling is not a different
spelling.* Frequency of use is not an argument about meaning.

## Breaking

Ten `$'\0'` sites were migrated to `$'\x00'` (`collparam.sh`, `commands.sh`,
`var_modifiers.sh`, `wordsplit.sh`) — see above. Two further spec tests
asserted the retired surface and were migrated:
`coverage.sh` `cov.63` (`$'\061\062\063'` → `$'\x31\x32\x33'`) and
`misc.sh` `utf8.8` (`$'caf\xc3\xa9'` → `$'caf\u00e9'`). `utf8.8`'s
underlying guard — a non-ASCII character must emit as UTF-8, two bytes
not four — is unchanged and still enforced.

## Implementation

`lexer.rs`: `consume_ansi_c_escape` dispatches; `take_hex_digits`,
`consume_ansi_c_hex`, `consume_ansi_c_unicode`, `push_utf8`.
`scan_ansi_c_octal` is gone. A rejected escape is recorded in
`Scanner::bad_escape` and reported by `parse` as **SB4063**, mirroring
`unterminated`/`attached_hash` — the escape is resolved in the lexer, so
that is the only channel through which a rejection can surface.

**Addendum (2026-10-24, batch-531 port step F13) — the channel was dropped at every re-lex.** Three
places re-lex a captured substring with a second `Scanner`: a double-quoted
`$(…)`, a subscript index and a sed RE/SUB. All three discarded the
sub-scanner, and its fatals went with it, so a rejection that can only
surface through `parse` did not surface at all. `${x:s/b/$'\101'/}` on
`abc` compiled and printed `a101c`, the escape read as its literal
characters; an unterminated `$(` as the sed pattern printed `babc`; and a
glued `#` inside `"$(echo a;# c)"` (DD#110, SB4058) became a command named
`#`, reported as SB5011. Each re-lex site now ranks its sub-scanner's fatal
above whatever the sub-parse returned, exactly as `parse` does for the whole
program. One limit remains: a sub-parse that fails early stops scanning, so
a fatal later in the same substring is not seen — but an error is still
reported, so nothing compiles that should not.

The body buffer is now valid UTF-8 **by construction**: `\xNN` is ASCII
and `\u` emits a whole character, so no two escapes can combine into one
character any more.

Specs: `misc.sh` `esc.1`-`esc.20`.

## See also
- DD#72 — numeric literals; the model this follows.
- DD#64 — no shell-level backslash; amended above.
