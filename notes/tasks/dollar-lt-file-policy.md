---
title: "`$(<file)` failure policy needs a DD"
status: open
priority: P3
kind: decision
depends: []
gates:
  - "a DD states what `$(<f)` does when the file is missing/unreadable"
  - "the `for w in $(<f)` word-split question is answered"
---

## Why now

Until 2026-09-13 `$(<file)` was a parse-time desugar to `$(cat file)`, so its
failure behaviour was not a sash decision at all — it was whatever `cat`
plus the general command-substitution machinery happened to do. It is now a
first-class node (`ExprKind::SlurpFile` → `sash_slurp_file`), so the
behaviour is ours and should be stated deliberately.

The implementation **preserved** the existing observable behaviour rather
than changing it unilaterally. Nothing is broken; this is ratification.

## Question 1 — missing or unreadable file

sash **aborts** with a diagnostic naming the path and the errno:

```
./prog: /no/such/file: cannot read file: No such file or directory
```

bash prints a warning and evaluates to the **empty string**, continuing.

Both are defensible:

- **abort (current)** matches sash's general posture — a program that cannot
  read its input should not proceed with an empty value it never checked. It
  also composes with taint: there is no "silently empty, but tainted" state.
- **bash-compatible empty** matters if oracle parity on this construct is
  wanted, but it hands the program an empty string that looks exactly like a
  successfully-read empty file.

Related precedent: DD#42 forbids a silent empty return for `argv` bounds, and
DD#101 declined to force handling on `getenv` because taint already requires
it. Neither settles this one.

**Recommendation: keep the abort, write it down.** A slurp that fails is not
a value.

## Question 2 — word-splitting in `for-in`

`for w in $(<f)` currently makes `w` tainted and the loop is rejected at the
use site unless attested. Whether the slurped value should *word-split* at
all in that position was never investigated — it inherited whatever the
cmdsub path did. If `$(<f)` is meant to be the `mapfile -t` replacement
(DD#99), splitting on IFS rather than newlines is arguably the wrong shape
and `matchall` is the intended route.

## Question 3 — diagnostic wording change (minor, no action may be needed)

`for w in $(<f)` was rejected with "tainted command substitution"; it is now
rejected with "tainted variable $w". Both reject. The new message names the
variable the user must attest, which is arguably more actionable, but it is
a user-visible wording change and DG may prefer the construct be named.
