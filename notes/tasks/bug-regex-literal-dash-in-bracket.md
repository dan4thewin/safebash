---
title: "Literal `-` first or last in a bracket expression is rejected (SB5023)"
status: open
priority: P1
kind: bug
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
