---
title: "bug: an attest `(( … ))` near-miss co-fires SB4020 with `$(( … ))` text"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "`s=$(arg 1); attest s (( n = (strlen s) ))` reports SB5054 only, or a second diagnostic that names the attest's `(( … ))` correctly"
  - "a plain `x=$(( n = 1 ))` still gets SB4020's `$(( … ))` text"
---

Found 2026-10-31 while porting F10 (journal 552). It reproduces on the pre-F10
binary too.

```sh
s=$(arg 1)
attest s (( n = (strlen s) ))
echo "$n"
```

Both binaries report two errors at 2:1:

- SB5054 "attest's `(( … ))` must be `(( TARGET = EXPR ))` where EXPR reads
  $s". This one is right: `(strlen s)` takes the literal word `s`, and a plain
  `(( n = (strlen s) ))` leaves `s` unread (SB5050).
- SB4020 "an assignment or `++`/`--` inside `$(( … ))` can't take effect". The
  program has no `$(( … ))`.

**Cause.** `attest_arith_predicate` (validate.rs, about line 3023) exempts
only a predicate that `classify_attest` decodes, so `check_stmt_value_rules`
(about line 3074) runs `check_arith_value_mutation` on a near-miss. The comment
there explains why the exemption is narrow: exempting a near-miss once let it
reach the value path and panic, before SB5054 existed. Now that SB5054 rejects
every near-miss, the second diagnostic only misdescribes the program.

**Fix options.**
- (a) Exempt every attest arithmetic predicate once SB5054 is certain to fire
  on the near-misses.
- (b) Keep the co-fire and word the message for an attest predicate.

Check the SB5054 specs first: any that assert a bare `-n $err` may lean on the
SB4020.
