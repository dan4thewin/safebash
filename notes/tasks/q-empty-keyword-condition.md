---
title: "Question: an empty condition in the `then`/`do` spelling compiles and is false"
status: done
priority: P3
kind: question
depends: []
gates:
  - "DG rules whether an empty condition is legal in the keyword spelling; the ruling is a DD#68 addendum"
  - "if rejected: `if then`, `elif then`, `while do` and `until do` are a compile error with a message about the missing condition, pinned by specs in both backends"
---

## What

Probed 2026-10-24 on base `1309385` and the F13 worktree (identical):

| probe | program | result |
|---|---|---|
| k2 | `if then echo a; fi; echo done` | compiles, prints `done` |
| k4 | `if then echo a; else echo b; fi` | compiles, prints `b` |
| k1 | `if /bin/false; then echo a; elif then echo b; fi; echo done` | compiles, prints `done` |
| k3 | `while do echo a; break; done; echo done` | compiles, prints `done` |
| k6 | `until do echo a; break; done; echo done` | compiles, prints `a done` |
| k5 | `while { echo a; break }; echo done` | SB4027 |
| p1 | `if /bin/true { echo a } elif { echo b }` | SB4027 |

An empty condition evaluates false. The brace spelling rejects the same
shape with SB4027, "a `{ }` group cannot be a condition"
(`brace_group_cond_error`). The keyword spelling has no such check:
`parse_if_keyword`, its elif loop, and the `do` branch of `parse_while`
accept an empty condition list.

## Why it is a question, not a bug

DD#68 calls `then…fi` and `do…done` sugar for the brace form, but its
reason for rejecting an empty brace condition is the `{` ambiguity, which
the keyword spelling does not have. No DD says an empty condition is an
error. (Bash rejects `if then` as a syntax error; that is a reference
point, not a rule.)

## Options

- (a) Reject in both spellings. The keyword spelling then needs a message
  about the missing condition; SB4027's text is about `{ }` groups.
- (b) Keep: an empty condition is false.

Recommendation: (a). A sugar spelling should not admit what the canonical
spelling rejects, and a silently-false empty condition reads as a typo
that compiled.

Probes: `files/port-f13-probes.tar.gz`, directory `el2/` (k1–k6, p1).

## Resolution (2026-10-27)

Answered in F1+F4 (journal 537) with option (a), as decide-and-report
rather than a ruling; DG may veto it in favour of (b), which would now mean
making an empty condition false (the next paragraph shows it never was).
`if then`,
`elif then`, `while do` and `until do` are SB4027 "this header has no
condition", anchored at the `then` or `do`. Specs errors.sh cnd.1-cnd.4,
both backends; DD#68 addendum 2026-10-26.

The table above misreads its own results: an empty condition was not
false. It tested the status the previous command left. On the pre-port
binary, `/bin/true` before `if then echo y; else echo n; fi` printed `y`
and `- /bin/false` printed `n`, in both backends. k2-k4 and k6 ran nothing
before the header, and k1's `elif` followed the `if`'s own `/bin/false`,
so each read as false. At program start the condition read a status that
an explicit `$?` may not read (SB5012 "no preceding command").

The option (a) message is SB4027's code with its own text, not a new code:
the brace spelling of the same mistake was already SB4027.
