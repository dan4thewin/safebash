# 377 — one break byte, two spellings

*2026-09-20*

DG asked two things: whether `$'\0'` was actually retired, and for
`y@{a:-x}` to report what `@{a:-x}y` reports.

## The `$'\0'` question

Retired, landed, done — journal 371. Ten spec sites became `$'\x00'`,
DD#115 was written, DD#64 amended. I had been carrying it in `status.md`
as *"still unanswered: the `$'\0'` retirement flag"* across several
sessions. That was wrong, and the error is worth naming: I flagged it as
an open decision at the moment I *implemented* it, and then never
retired the flag when DG's own framing (DD#72, applied to escapes)
turned out to settle it. **A note that says "awaiting DG" is a claim
about DG, and it has to be retired as carefully as it was raised.**

What was genuinely missing was smaller and duller: `sash.1` had **no
`$'…'` section at all**. An entire escape surface — which escapes are
accepted, which raise SB4063, and why a NUL is now `$'\x00'` — was
governed by a DD and by good diagnostics, and documented in neither the
man page nor the guide. Added: accepted set, rejected set with the
replacement each one names, and the note that the `\x7f` cap is a
*source-literal* constraint only, since the runtime is byte-clean.

The diagnostics themselves were already doing the teaching, which is
presumably why the gap survived:

```
$'\0'   SB4063 … octal escapes are not supported (`\0`, `\101`, `\0101`)
               — write the byte as `\xNN` hex …
$'\xff' SB4063 … `\xff` is above `\x7f` — a lone byte above ASCII is not
               UTF-8 text; write the character …
```

Two further gaps found and reported rather than fixed: `sash-convert`
passes both `$'\0'` and DD#72's `16#ff` / `010` through unchanged, so a
converted script fails to compile on a construct that has an exact,
lossless respelling. DD#57's "Mechanical — convert silently" table
already contains this shape (`` `cmd` ``→`$(cmd)`, `${var-x}`→`${var:-x}`),
so the category is settled; what is not settled is whether DG wants that
scope taken on now. Asked.

## `y@{a:-x}`

`@{a:-x}y` raised SB5083. `y@{a:-x}` said **`SB5011 command 'a:-x' not
found`** — a message about a command, for a program containing no
command by that name.

`scan_word` tries the `@`-word scan only when the word *starts* at `@`:

```rust
if self.source.get(self.pos) == Some(&b'@') {
    if let Some(tok) = self.try_scan_at_word(start) { return tok; }
}
```

Mid-word, `@` was an ordinary byte. `{` already breaks words, so `y@`
became one word and `{a:-x}` opened a brace **group** — hence a command
named `a:-x`.

The fix is one arm in `word_ends_here`, which already had the pattern
(`b'+' => …== Some(&b'=')`):

```rust
b'@' => self.source.get(self.pos + 1) == Some(&b'{'),
```

What makes this the right shape rather than merely a working one: it does
not add a second diagnostic for the prefix case. It makes `y@{a:-x}` lex
as `y` + `@{a:-x}`, which the adjacency pass joins into **the same
`Concat`** the suffix spelling already produced, so both spellings reach
the one check in `check_concat_splat_seq`. "Report the same" is then a
structural property, not two messages kept in sync by hand.

`y@{a}` followed for free — SB5033, missing disposition, exactly what
bare `@{a}` reports.

The empty-word hazard is real and does not arise: a word-*leading* `@{`
never reaches the new arm, because `try_scan_at_word` always claims the
braced form, so `scan_word_chars` can only meet `@{` with a non-empty
word. Single caller, checked.

Controls, all spec'd, all still accepted: `user@example.com`, `a@b`
(`@` breaks only before `{`), `"x@{a:-y}"` (quoted is literal), and the
brace-less `y@a:-x`.

That last one is a deliberate non-change and the one open asymmetry.
`y@a:-x` is *currently accepted* as literal text, so changing it would
break an accepted program rather than improve an error — and unlike the
braced form it is not silently *collapsing* anything, since the array is
simply not splatted and SB5050 catches the unread array. Flagged to DG
rather than decided.

## The plan drift, found by the mechanical check

`glob_splat.sh` came back `plan=53` against 54 `chk` tokens after my
edit. Walking it back per commit:

```
1f8b699  plan=42   chk=42
d9b7598  plan=47   chk=48    <-- MISMATCH
```

**Mine**, from earlier the same day. I added six specs and bumped `plan`
by five.

The interesting part is why nothing caught it. The *sweep total* was
right — I predicted 5783 and got 5783 — because I counted the six specs
correctly for the total and fat-fingered only the per-file line. And
`common.sh` notes a plan mismatch rather than failing on it. So the one
gate that would have fired is the plan-vs-`chk` check, which I ran
before the edit and not after.

Confirmation came out clean: with `plan=54` the plan sum equals the
sweep exactly (5795). At HEAD the sum had been 5781 against 5783 actually
run — a gap of precisely one per mode, which is the same off-by-one seen
from the other side.

**Run the mechanical checks after editing specs, not before.** The
before-run tells you about someone else's mistake; the after-run is the
only one that can tell you about your own.

## Gates

Sweep **5795** (5782 passing + 13 skipped), 0 failing, **0 flips vs
`d64cee0`**. Units 50/27/20. groff clean.
