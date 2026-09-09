---
title: "investigate: the `s///` sugar silently truncates at any unhandled token — `$'…'`, and the metachars `= # & < > ;`"
status: ready
priority: P2
kind: decision
depends: []
gates:
  - "a decision recorded: either the sugar accepts `$'…'` in RE/SUB, or DD#63's compromise is reaffirmed and the limitation is documented rather than left to be rediscovered"
  - "if accepted: `s =~ s/:/$'\\0'/g` behaves as `$(gsub $s \":\" $'\\0')` does today"
  - "if accepted: the meaning of a decoded delimiter (`$'\\x2f'` → `/`) in the replacement is pinned by a spec, not left to fall out of the implementation"
  - "`s =~ s/=/-/g` (and `#`, `&`, `<`, `>`, `;` in the RE) either works or is rejected for a *stated* reason — today all give a misleading SB4032 \"malformed … expected three `/` delimiters\" when the payload is well-formed"
---

## The asymmetry

The function form takes `$'…'` directly; the `s///` sugar does not:

```sh
t=$(gsub $s ":" $'\0')     # → a NUL b NUL c        ✓
s =~ s/:/$'\0'/g           # → SB4032 (malformed)   ✗
sep=$'\0'; s =~ s/:/$sep/g # → a NUL b NUL c        ✓  (the workaround)
```

DD#96 defines the statement form as desugaring to `$(sub|gsub …)`, so the
sugar being *less capable* than what it desugars to is at least surprising.

## This was a deliberate trade-off

DG (2026-09-08): *"I think this was a known trade-off when we made the s///
form."* Confirmed — **DD#63** says so explicitly:

> "The thin sugar. Kept deliberately minimal — only the common case; anything
> else uses the function form."

> "The `s///` sugar is `/`-delimited **only** and **always interpolating** (the
> fields desugar to double-quoted, i.e. interpolating, args)."

> "A literal `/` (the delimiter itself) is `\x2f`, or drop to the `$(sub …)` /
> `$(gsub …)` function form, which needs no delimiter and can quote the args
> however you like."

`$'…'` is not a double-quoted construct, so it falls outside the sugar's field
model **by design**, not by oversight. This task is therefore an
*investigation*, not a bug report: is there a good solution that does not
thicken the sugar?

## Addendum 2026-09-08: it is a whole class, not one construct

The same truncation hits ordinary shell metacharacters in the **RE** slot:

| RE | result |
|---|---|
| `s =~ s/=/-/g` | **SB4032** |
| `s =~ s/#/-/g` | **SB4032** |
| `s =~ s/&/-/g`, `s/<//`, `s/>//`, `s/;//` | **SB4032** |
| `s =~ s/:/-/g`, `s/a/-/g` | ok |
| `$(gsub $s "=" "-")` | **ok** — `NAME-sash` |

The diagnostic is actively misleading: *"malformed `=~ s/…` substitution —
expected s/RE/SUB/[flags] with three `/` delimiters"* when the payload has
exactly three delimiters and is perfectly well-formed. The real cause is that
the token stream broke.

**This matters for `switch-free-read-family`.** `k=v` line formats
(`/etc/os-release`, env files, `passwd`-alikes) are the canonical map source,
and converting them to a NUL-separated stream is exactly
`s =~ s/=/$sep/g` — which is one of the broken cases. The function form
`$(gsub $s "=" $sep)` works, so the redesign is not blocked, but the sugar is
unusable for the most common map-building step.

## Mechanism

`reconstruct_sed_raw` (`parse/parser.rs`) rebuilds the payload from tokens and
accepts `Tok::Word`, `Tok::Dollar`, `Tok::DollarBrace` and (DD#96)
`Tok::Newline`. **Every other token hits `_ => break`**, truncating the raw
text so it fails the three-delimiter check → SB4032. `Tok::AnsiCQuoted` is one
instance; the shell metacharacters that lex as operators (`=` assignment, `#`
comment, `&` background, `<`/`>` redirect, `;` separator) are the rest.

So the defect is structural, not per-construct: a token-driven reconstruction
of what is semantically *raw text* will keep losing whichever tokens nobody
enumerated. Any fix should consider capturing the payload span from the source
directly rather than re-accumulating it token by token.

## Why it is not a one-line fix

`raw` holds **source text** — `$sep` is pushed verbatim and expanded
downstream — whereas `$'…'` is **already decoded at lex time**. Pushing decoded
bytes mixes the two representations, and the decoded bytes may include the
delimiter itself: `$'\x2f'` is `/`. Splitting the raw text on `/` afterwards
would then read a *value* byte as *structure* and silently mis-split the edit —
the failure mode DD#63 was avoiding when it made `/` the sole delimiter.

So the real question is where decoding happens relative to delimiter-splitting:

- **decode after splitting** — `$'\x2f'` is a literal `/` in the replacement
  (probably what a user wants), but the raw slot must carry a marker rather
  than text, which is exactly the "special parser that handles delimiters"
  DD#63 rejected in its opening;
- **decode before splitting** — one match arm, but `$'\x2f'` silently becomes a
  delimiter;
- **reaffirm DD#63** — leave the sugar thin, document that `$'…'` needs the
  function form or a variable binding, and add a spec pinning the SB4032.

## Note

Surfaced 2026-09-08 while establishing that DG's NUL-separator design works
with no language changes — `$(gsub $s ":" $'\0')` is the in-process,
fork-free way to convert any separator to NUL. The design does **not** depend
on resolving this; the function form already does the job. That is why this is
P3.
