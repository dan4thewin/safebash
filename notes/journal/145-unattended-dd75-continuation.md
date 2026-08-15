---
title: 145 — unattended session: DD#75 continuation (running log)
date: 2026-08-14
---

# 145 — unattended DD#75 continuation (running commentary)

Unattended SOP (journal 115 + 132): work the ready backlog in dependency
order, don't stop for status, skip decision-gated (⚠) tasks, file
non-blocking bugs/decisions as tasks, fix blocking bugs inline. Test-first
per part (DG standing instruction). Brief running log here.

Ready backlog (all DD#75-decided or earlier, no unmet deps), risk order:
1. `bug-subscript-assign-bare-key` (P2 bug) — enforcement symmetry.
2. `dd75-keys-map-only` (P3) — reject keys on non-map.
3. `intrinsics-lengths-in-arithmetic` (P2) — arith len/length eval.
4. `dd75-retire-bracket-subscripts` / `dd75-special-var-sugar` /
   `dd75-args-model` (P3, larger language changes).

Baseline (last session, unchanged): native 0 failing; c 13 / janet 36.

## Task 1 — bug-subscript-assign-bare-key (DONE)

`check_assignment` didn't validate `Assignment.index`, so `a[i]=Q` (bare
string key) compiled silently on an indexed array while the read `${a[i]}`
was SB4030. Added `check_subscript_key_kind(&a.name, idx, …)` to
`check_assignment` — reuses the read-path helper, so the reject is
symmetric and map bare-key assignment `m[k]=…` stays valid.

Test-first: types.sh 33.11 (indexed bare-key assign → reject, red→green),
33.12 (sigil/literal still work). native 0 failing; c 13 / janet 36 ==
baseline; 33.11/33.12 green in all three modes. (4 codegen_tests fail
pre-existing SB5001 declare-rot — confirmed identical at HEAD.)

## Task 2 — dd75-keys-map-only (DONE)

`check_keys_operand` (type_check.rs, sibling of check_join_operand): `keys`
operand must be a declared map (assoc_arrays), else SB4030 — indexed array
(points at `for (( i=0; i<$(len a); i++ ))` / @a) or scalar/undefined.
Hooked into check_command + the cmdsub descent. Also added a **For arm** to
check_stmt (was missing entirely) so for-in header words are type-checked —
`for k in $(keys a)` now reaches the reject.

Test-first: types.sh 75.7 (map keys work, guarded native,c — janet execs
keys as external, pre-existing cf. janet:33.10), 75.8 (indexed reject), 75.9
(scalar reject). native 0 failing; c 13 / janet 36 == baseline. janet keys
gap appended to janet-join-intrinsic-gaps.

Note: the For-arm addition means for-in headers are now type-checked in
general (previously only the body was) — a latent coverage gap closed.
