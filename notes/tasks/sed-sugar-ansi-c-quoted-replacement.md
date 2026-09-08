---
title: "investigate: `$'…'` in the `s///` sugar's replacement slot (sugar is less capable than the function form)"
status: ready
priority: P3
kind: decision
depends: []
gates:
  - "a decision recorded: either the sugar accepts `$'…'` in RE/SUB, or DD#63's compromise is reaffirmed and the limitation is documented rather than left to be rediscovered"
  - "if accepted: `s =~ s/:/$'\\0'/g` behaves as `$(gsub $s \":\" $'\\0')` does today"
  - "if accepted: the meaning of a decoded delimiter (`$'\\x2f'` → `/`) in the replacement is pinned by a spec, not left to fall out of the implementation"
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

## Mechanism

`reconstruct_sed_raw` (`parse/parser.rs`) rebuilds the payload from tokens and
accepts `Tok::Word`, `Tok::Dollar`, `Tok::DollarBrace` and (DD#96)
`Tok::Newline`. `Tok::AnsiCQuoted` hits `_ => break`, so the raw text truncates
to `s/:/` and fails the three-delimiter check → SB4032.

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
