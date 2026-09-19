# 371 — the escape hatch that could not hold a byte

DD#64 called `$'…'` the universal escape hatch. Its ratified text says
*"any byte via `\xNN`"* and *"complete literal-byte control inside an
interpolated string"*. That was the whole argument for removing every
other backslash handler in the shell: you do not need `\` anywhere else,
because `$'…'` can say anything.

It could not. `$'\xff'` emitted `ef bf bd` — U+FFFD, the replacement
character. Every byte from 0x80 up, silently wrong, for the entire life
of the feature.

## How it surfaced

Coverage, again, and again not where the cold line was.

`simple_escape` in `lexer.rs` is a twelve-arm table mapping `n`→`\n`,
`t`→`\t`, and so on. Almost none of it was covered. That is about as
boring a cold spot as exists — a lookup table with no logic in it — and
the honest expectation was a pure coverage gap.

It was. All twelve named escapes match bash exactly. But sitting next to
the table were the octal and hex scanners, and writing probes for the
table meant writing probes for its neighbours. Seven of those diverged:

| written | bash | sash |
|---|---|---|
| `\xff` | byte `ff` | **`ef bf bd`** |
| `\0012` | `\001` then `2` | `\n` |
| `\0777` | `?` then `7` | mangled |
| `\xzz` | literal `\xzz` | a NUL byte |
| `\101` | `A` | literal `\101` |
| `\u00e9` | `é` | literal `\u00e9` |
| `\cA` | `\001` | literal `\cA` |

Not one of these produced a diagnostic. They all produced *output*.

## The one that mattered

`scan_ansi_c_bytes` accumulates a `Vec<u8>`, and its comment explains
why with some care: `\xC3\xA9` is two escapes that together spell one
character, so pushing each as a `char` would re-encode them separately
and turn two bytes into four. Someone thought about this properly.

Then `scan_ansi_c_quoted`, at the very next call site, did
`String::from_utf8_lossy(&bytes)`.

The bytes were collected exactly right and thrown away one line later.
A Rust `String` is UTF-8 by type, so a lone `0xFF` has nowhere to live
in it; `from_utf8_lossy` does what its name says and substitutes U+FFFD.
The bug was not a mistake in the escape logic. It was a representation
boundary that the escape logic could not see across.

There is a sharper way to state the inconsistency. A raw `0xff` byte
**in a source file** is rejected loudly — `stream did not contain valid
UTF-8`, refuse to compile. The officially sanctioned way to write that
same byte corrupted it in silence. The compiler was strict about the
thing it could not control and permissive about the thing it could.

## The fork, and DG's answer

Fixing the representation properly means bytes through `ExprKind::Literal`
— 127 sites, plus `intern_or_declare`, which does `s.as_bytes()` and
appends a NUL. Large, and in service of what?

I put it to DG as a language-surface question. His answer reframed it:
*"discuss use cases — we already retired `16#ff` in favour of more
modern/sane formulations."*

That is DD#72, and once named it is obviously the same decision. DD#72
kept `0x`/`0o`/`0b` and rejected `010`, `0377`, `base#digits` — not
because they were hard, but because *the constant is authored, so
strictness forces the writer to say exactly what they mean*. An archaic
spelling gets a clean diagnostic, never a silent difference.

Apply that here and the set falls out:

- **`\0NNN` is the `010` of the escape world.** Archaic, ambiguous about
  how many digits it takes, and — exactly like `16#ff` truncating to
  `16` — sash had already got it wrong. bash reads `\nnn` as one to three
  digits *total*, so `\0101` is `\010` then `1`. sash read `\0` plus up
  to three more and produced `A`. Same failure, same shape, found again.
- **`\xNN` capped at `\x7f`.** A lone byte above ASCII is not text.
- **`\uHHHH`/`\UHHHHHHHH` added.** The modern formulation for a character
  above ASCII: it names a character rather than a byte, and its UTF-8
  encoding is valid by construction.
- **`\cX` rejected.** `\x01` says it better.
- **unknown escapes pass through**, byte-identical to bash. There is no
  divergence there to fix, so there is no reason to reject.

Then the measurement that made the cap cheap: **the runtime is byte-clean.**
`read`, `$(arg 1)`, `$(getenv HI)`, `$(/bin/cat f)` and direct external
passthrough all carry `61 ff 62` intact. The UTF-8 constraint binds *only*
the source-literal boundary. The question was never "can sash handle a
high byte" — only "can you *author* one inline". DG: **"authoring raw
binary is YAGNI."**

## What it cost, which was more than I thought

The sweep said so. Eighteen failures, nine tests across four files, and
every one of them was `$'\0'`.

Of course it was. `\0` is the octal escape — the single most-used member
of the family I had just deleted, and it had not come up once in the
discussion. It is how every NUL in the spec suite is written: word-split
separators, and the DD#55 argv-NUL abort.

The temptation to carve it out was immediate and worth recording, because
the argument for it is bad in an instructive way. `\0` is not archaic; it
is universal; everyone knows it; nothing about it is ambiguous. All true,
and none of it is an argument about *meaning*. `\0` **is** the octal
escape. Keeping it because it is popular would be a carve-out that
contradicts the rule rather than an application of it.

And the carve-out would not even be clean. `\0` accepted while `\01` —
which bash reads as octal 1 — stays an error is a worse rule to explain
than "no octal".

So it went, and ten sites became `$'\x00'`, which is an exact replacement:
verified to still build the interior NUL and still fire the DD#55 abort.
The principle: *a popular spelling is not a different spelling.*

`AGENTS.md` has the instruction for this, and it is the one I nearly
missed — apply a stated principle to **all** its consequences, do not
narrow it into a smaller decision so you can defer the rest. The nine
failures were the principle's consequences arriving.

Two more specs asserted the retired surface directly: `cov.63` tested
octal decoding, and `utf8.8` tested `$'caf\xc3\xa9'` — the very idiom
the byte-accumulation comment was written to protect. Both migrated;
`utf8.8`'s real guard (a non-ASCII character emits two bytes, not four)
is unchanged and now rides on `\u00e9`.

## The reporting channel

An escape is resolved in the lexer, which has no diagnostic path — so
`Scanner` gained `bad_escape`, mirroring the `unterminated` and
`attached_hash` channels already there, and `parse` reports it as SB4063.

One detail cost real time and is worth writing down: **`reject_escape`
must not advance `self.pos`.** The digit scanners have already consumed
what they read. An extra increment ate the closing `'`, so a rejected
`$'\xff'` reported *SB4057, unterminated string* — a plausible, confident,
entirely wrong diagnostic. Found by running it. Not by reading it.

## The gate earned its keep

Adding the SB4063 block pushed `parse` to cog 7 against a 5.85 bar, and the
pre-commit gate rejected the commit. It was right to. Three consecutive
`if let … return Err(vec![parse_error(…)])` blocks had accumulated there,
one per scanner-level fatal, and the third was mine.

They are now `scanner_fatal`, which returns the first of them. `parse` drops
to cog 5, the helper is cog 3, and the thing actually worth having is a
single place that states *why* the order is the order: it is the order of
consequence, not of severity. An unterminated quote swallows the source
after it, so everything else is downstream. That rationale was previously
spread across three comments that each explained only their own case.

## Ledger

- 20 specs, `misc.sh` `esc.1`–`esc.20`; 12 sites migrated.
- `parse` split: `scanner_fatal` extracted for the complexity gate.
- Sweep **5729, 0 failing, 0 flips** (5689 + 40, predicted exactly —
  twice, once before the `\0` discovery and once after).
- DD#115 written; DD#64 amended, since its escape-hatch claim was
  ratified and is now deliberately narrower.

The pattern from 370 held again: the cold line was not the bug. The cold
line was a reason to ask a question, and the answer was somewhere else.
Three bugs this stretch, and none of them lived where the coverage was
missing.
