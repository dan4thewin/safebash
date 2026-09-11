---
title: "NOT A BUG: literal `-` in a bracket expression is spelled `\\-`"
status: closed
priority: P1
kind: not-a-bug
depends: []
gates:
  - "`[[ $s =~ ^[A-Za-z-]+$ ]]` compiles (trailing `-` is a literal)"
  - "`[[ $s =~ ^[-A-Za-z]+$ ]]` compiles (leading `-` is a literal)"
  - "`[[ $s =~ ^[a-z_-]+$ ]]` compiles"
  - "a genuinely malformed class (`[a-]-z]`-style) still rejects"
  - "runtime match results agree with bash for each of the above"
---

## Context

POSIX ERE says a `-` that is **first or last** inside a bracket expression is a
literal, not a range separator. Bash accepts all three patterns below and
matches with them. sash rejects them at compile time:

```
$ cat x.sh
str s=hi
if [[ $s =~ ^[A-Za-z-]+$ ]]; then echo m; fi
$ sash -o x x.sh
x.sh:2:14: error [SB5023] invalid regex pattern: malformed '[]'
```

Measured (2026-09-11):

| pattern            | sash    | bash  |
|--------------------|---------|-------|
| `^[A-Za-z]+$`      | OK      | OK    |
| `^[A-Za-z0-9./]*$` | OK      | OK    |
| `^[A-Za-z-]+$`     | SB5023  | OK    |
| `^[-A-Za-z]+$`     | SB5023  | OK    |
| `^[a-z_-]+$`       | SB5023  | OK    |

## Where it comes from

**Not sash's validator.** `sash/src/regex_validate.rs` is a thin wrapper that
calls `regcomp9` and reports whatever the engine says; `malformed '[]'` is the
**Plan 9 regex library's** message. So the divergence is in the vendored engine,
and it is not only a compile-time gate — the same engine runs at runtime, so a
pattern built at runtime would fail there too.

This matters beyond ergonomics: `[a-z-]` and `[-/.]` are ordinary idioms in
attest gates, and DD#24's whole posture pushes authors toward writing character
classes for validation. A rejected-but-valid class nudges them toward a looser
pattern.

## Why it is filed rather than fixed

Found mid-DD#100 while writing an attest gate. The fix is a **fork DG should
pick**, not a mechanical repair:

1. **Patch the vendored Plan 9 regex** to accept a first/last literal `-`.
   Correct at both compile and run time, one place. Touches vendored C.
2. **Normalise the pattern** before it reaches the engine (rewrite a first/last
   `-` into an escaped form). No vendored change, but adds a translation layer
   that must stay faithful, and only helps literal patterns.
3. **Document the divergence** and reject with a message that *teaches* the
   workaround (`put the dash in the middle escaped`, or `[\-…]`). Cheapest,
   but leaves sash accepting a strictly smaller regex language than bash while
   claiming byte-identical behaviour.

Option 1 is the only one that keeps the "same language as bash" promise, but it
is the one that touches vendored code.

## Related

- `notes/tasks/bug-regex-rhs-space-quote.md`
- `notes/tasks/limitation-regex-stops-at-interior-nul.md`
- DD#39 (compile-time regex validation), DD#63

## Resolution (DG, 2026-09-11) — not a bug; the premise was wrong

DG ruled on the fork directly, and rejected all three options because the
question was mis-framed:

> the regex library is chosen on faith in the author, Rob Pike. It represents
> his view of a sane pre-feature-creep implementation. POSIX compliance is not
> a goal. dash can be used in a character class with a `\` (I think). I have
> made small changes for convenience, not compliance. It deserves a full
> write-up in the future docs/man page

DG's recollection is correct, verified 2026-09-11: `\-` works in **every**
position and agrees with bash's result on the same subject.

    ^[A-Za-z\-]+$  matches a-b      (bash: same)
    ^[\-A-Za-z]+$  matches -ab      (bash: same)
    ^[a-z_\-]+$    matches a_b-c    (bash: same)

So the claim in "Where it comes from" above — that sash "accepts a strictly
smaller regex language than bash" — is **false**. The same character classes
are expressible; they are spelled with an escape, exactly as `{` and `[` are
under the 2026-07-08 ruling. The attest-gate ergonomics concern is real but
costs one backslash.

No engine patch, no normalisation layer. The posture is recorded as a
clarification in `notes/decisions/DD39-glob-regex.md`, which also corrects two
stale claims found while confirming this (`\d`/`\w`/`\s` **are** supported;
`{n,m}` is capped at 32).

Superseded by `docs-regex-dialect-writeup`.
