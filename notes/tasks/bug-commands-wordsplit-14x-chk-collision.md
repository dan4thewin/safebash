---
title: "chk-id collision: commands.sh and wordsplit.sh both use 14.4-14.9"
status: pending
priority: P2
kind: chore
depends: []
gates:
  - "no test_id is emitted by two spec files"
  - "a failure in either file's 14.x test is visible in spec.db (not masked)"
---

## Problem

`chk` ids are a flat GLOBAL namespace (AGENTS.md). `commands.sh` and
`wordsplit.sh` both use the `14.x` block for *different* tests:

- `commands.sh`: 14.4/14.5/14.6 (compound-redirect rejects), previously
  14.7/14.8 (bare nested cmdsub — now moved to 14.17/14.18).
- `wordsplit.sh`: 14.4-14.9 (postfix `&&`/`|`/`&`/redirect on compound rejects).

Because `wordsplit.sh` sorts after `commands.sh`, its rows overwrite
`commands.sh`'s for every shared id. This actively **masked a real failure**:
`commands.sh` 14.7 (a tainted nested cmdsub, correctly SB5020-rejected) was
hidden behind `wordsplit.sh` 14.7 (`&&`-on-compound, passing), so `spec.db`
showed `native:14.7 = passing` while the commands.sh assertion was failing.
This produced ~420 both-status rows and a phantom "flip-flop" (see the retired
`bug-nested-bare-cmdsub-bless-taint`).

## Fix

Give each file a disjoint block. `commands.sh` owns DD#14 (process primitives /
redirects); `wordsplit.sh`'s 14.x are really *postfix-composition* rejections
(SB4021-4024) and could move to a free range (e.g. a `post.x` block or a
DD-numbered range it owns). Then `grep -rhoE "chk [0-9.]+" spec/*.sh | sort |
uniq -d` should be empty for these ids. Audit the whole suite for other
cross-file collisions while here.

## Found

Investigating a mischaracterized "taint nondeterminism" P1 (2026-08-29) — the
per-run flip was this collision, not a taint bypass.
