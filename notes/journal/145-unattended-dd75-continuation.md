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

## Task 3 — intrinsics-lengths-in-arithmetic (PARTIAL: gate 1 done)

DG demo case `$(( (len a) + 1 ))` now works. `funccall_to_ssa` intercepts an
int-returning intrinsic (`is_arith_int_intrinsic` → `len`), reconstructs a
Command from the sexpr args (`reconstruct_intrinsic_command`), and reuses
`try_native_intrinsic` → the arrlen/strlen ParamExpansion, instead of the
user-int-func path (`inline_int_func`) returning 0. `(len a)`→count,
`(len s)`→bytes, `(len m)`→entries; nested + user int-fns still work.

Test-first: arithmetic.sh 58.10–58.13, green in native/c/**janet** (janet
handles `(len a)` in arith natively — no guard needed). native 0 failing;
c 13 / janet 36 == baseline. (2 codegen_tests fail pre-existing declare-rot.)

Deferred (filed `arith-braced-length`, P2): `$(( ${#a} + 1 ))` still 0 — a
PARSE-level gap (arith lexer reads `${#a}` as var `#a`). Fix sketch: lexer
rewrites `${#a}` into the `(len a)` FuncCall shape so the transform side
(just added) needs no new arm. Also deferred: string-intrinsic-in-arith
reject.

## Task 3 gate 2 — arith-braced-length (DONE) + janet correction

`${#a}` in `$(( ))` now evaluates. Added `ATok::LenOf`: arith lexer
`scan_arith_braced_var` emits it for `${#name}`; parser lowers to
`FuncCall{len,[Var(name)]}` — the same shape `(len a)` takes — so the
gate-1 transform path handles it with no new arm. `${#a}`→count,
`${#s}`→bytes, `${#m}`→entries; `${x}`/`$#` unaffected.

**Correction to gate 1:** my a80d94d note "green in janet" for 58.10–13 was
WRONG — janet emits an undefined `(len …)` symbol (its builtin is `length`),
so `len` in arith-operand position fails in janet. I misread a stale spec.db
row. Fixed here: guarded 58.10–16 to native,c and filed the janet gap
(`janet-join-intrinsic-gaps` addendum). Verified: `bash arithmetic.sh` PASSES
in native/c/janet (janet skips the 7); full sweep native 0 failing, c 13 /
janet 36 == baseline. Lesson: trust `bash <spec>.sh` per-mode over the
aggregated spec.db row when a test is freshly (re)guarded.

## Session summary (2026-08-14 unattended)

Landed (test-first, all pushed on hand-parser):
- ab184c0 bug-subscript-assign-bare-key (P2) — `a[i]=` bare key reject.
- dc9f8df dd75-keys-map-only (P3) — keys map-only + For-arm type-check.
- a80d94d + 2dad1ab intrinsics-lengths-in-arithmetic gates 1+2 —
  `$(( (len a)+1 ))` and `$(( ${#a}+1 ))` evaluate (DG demo).

All: native 0 failing; c 13 / janet 36 == baseline. Filed janet gaps
(join/keys/len-in-arith) under janet-join-intrinsic-gaps.

Remaining DD#75 backlog = the 3 invasive language changes
(dd75-special-var-sugar, dd75-args-model, dd75-retire-bracket-subscripts)
+ deferred small bits (string-intrinsic-in-arith reject, undefined-len
hard reject). These are larger/migration-bearing — better as focused
efforts. string-func-capture-in-arg-ice is decision-gated (works vs reject)
— left for DG.
