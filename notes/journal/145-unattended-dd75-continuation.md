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

## Task 4 — string-func-capture-in-arg-ice (DONE, DG: lift-and-print)

`echo $(greet)` ICE'd (`lower_func_call_value` non-constant panic). DG chose
lift-and-print (code unreleased → no migration risk; convert Qs as follow-ups).

Investigation: the two-step `s=$(f)` works via `try_capture_inline`
(CaptureStart/End inline-capture), which arg-position lacked. First attempt (lift
`FuncCall` → `Assign{tmp,FuncCall}`) only fixed const-foldable fns — runtime fns
(getpid) still ICE'd. Correct fix: generalized `try_capture_inline` into a shared
`capture_func_into(target,fname,args,…)`; the `lift_int_func_calls` FuncCall arm
captures into a `__strcap_<f>` temp. Now runtime capture works too
(getpid→live pid). Args are dropped upstream by `func_call_capture` for string
`$(f)` (pre-existing), so none threaded in arg position.

Test-first: functions.sh 17.3/17.4 (native; c/janet guarded — they don't
implement CaptureStart/End: c → TODO+straight-to-stdout, janet → execs fn name;
filed cjanet-function-stdout-capture). native 0 failing; c 13 / janet 36 ==
baseline.

## Session close (2026-08-14 unattended)

5 tasks landed (test-first, pushed on hand-parser):
- ab184c0 bug-subscript-assign-bare-key (P2)
- dc9f8df dd75-keys-map-only (P3)
- a80d94d + 2dad1ab intrinsics-lengths-in-arithmetic gates 1+2 (P2, demo)
- 84c28ce string-func-capture-in-arg-ice (P2, DG: lift-and-print)

All native 0 failing; c 13 / janet 36 == baseline. Filed janet/c backend gaps
(join/keys/len-in-arith, function-stdout-capture).

DG mid-session unblocked the invasive DD#75 tasks (code unreleased → no
migration risk; convert Qs as follow-ups). Remaining, now scoped:
- dd75-retire-bracket-subscripts — PARSE-level reject (${a[@]} and @a share the
  arrvals representation; distinguish only at parse) + flips passing wordsplit/
  codegen/brush tests. Scoping written into the task file.
- dd75-args-model — implicit `args` array; most invasive.
- dd75-special-var-sugar — mostly converter/docs (compiler already accepts both).

Paused before the parser-level retire change to avoid a rushed cross-cutting
edit at session end; scoped it for a clean pickup.

## dd75-retire-bracket-subscripts DONE (2026-08-14 cont.)

Retired the `[@]`/`[*]` bracket-subscript collection forms (DD#75 §4). `@a`
is the sole splat; `${#a}`/`$(len a)` the length; `$(keys m)` the keys.

**Parse-level reject (SB4038).** `${a[@]}` and canonical `@a` both lower to
`ParameterExpansion{arrvals}` — identical downstream (that is *why* they were
aliases), so the only place to distinguish them is the parser. Tagged the 5
bracket parse sites in `parse/expr.rs` with `*_retired` operators
(`arrvals_retired` / `arrlen_retired` / `arrkeys_retired`); the canonical
`@a` arm (expr.rs `@`-word) is untouched and still yields plain `arrvals`.
`parse/parser.rs` `rejected_param_operator_diag` now calls a new
`retired_bracket_operator_diag` that emits SB4038 with the canonical
replacement spelled out per form. `compile_to_object` already fails on
parse-error diagnostics, so the reject fires on the validation-bypass path too
(no ICE).

Sites tagged: non-brace `$a[@]` (try_subscript_arm), `subscript_expr`, braced
`${a[@]}` (try_subscript) → `arrvals_retired`; `${#a[@]}` (try_strlen_or_arrlen)
→ `arrlen_retired`; `${!a[@]}` (try_indirection_or_arrkeys) → `arrkeys_retired`.
`[*]` is covered at every site (the `@`/`*` and `[@]`/`[*]` checks were already
symmetric).

**Verified gates** (all behavioral): `${a[@]}`/`$a[@]`/`${a[*]}`/`$a[*]` →
SB4038 → `@a`/`$(join a)`; `${#a[@]}` → `${#a}`/`$(len a)`; `${!a[@]}` →
`$(keys m)`. Unaffected: `@a` splat, `${#a}` length, `${a[$i]}`/`${a[1]}`
element read, `a[$i]=` element assign, `@{a:-W}`/`@{a:?W}` disposition, `*` as
glob + arithmetic multiply.

**Tests.** spec types.sh 75.10–75.16 (plan 86→93): each retired form rejects,
`@a` splat + `${a[$i]}` element access still work — all 3 modes (parse-level →
backend-independent). wordsplit.sh ws.q4/q5 flipped (were "input-convenience
accepted" → now SB4038, asserting the code). types.sh 30.11 uses canonical
`${#a}`. Rust migrated to canonical forms: brush `brush_rejected_nonexistent_array_loop`
(SB4020→SB4038), c_tests `c_array_length`/`c_array_join_all`/`c_array_for_iter`,
codegen `for_array_iteration` (→`@arr`, dropped bash-parity — `@arr` isn't
bash), e2e `pos_array_length`. All pass.

native 0 failing; c 13 / janet 36 == baseline (0 regressions, latest-run diff).
clippy clean.

Filed the converter half as a follow-up: `converter-retire-bracket-splat`
(rewrite the retired bash spellings to canonical, DD#75 §6 — split per DG's
convert-as-follow-up rule).

## BUG FIX: computed special vars ($$, $?) silent-0 as arithmetic operands (2026-08-14)

While assessing dd75-special-var-sugar (§1) I found a stop-everything bug:
`$$` and `$?` evaluate to **0** when used as an operand in a *compound*
arithmetic expression, while working standalone and as words.

```
echo $(( $$ ))          → pid   (ok, standalone)
echo $$                 → pid   (ok, word)
echo $(( $$ + 0 ))      → 0     (BUG — should be pid)
echo $(( $$ > 0 ))      → 0     (BUG — should be 1)
(( 0 )); echo $(( $? + 7 ))  → 7 (BUG — should be 8; $? = 1)
```

`$#` was unaffected (it is a materialized var slot). Pre-existing (repro at
HEAD bef23d9). Violates DD#24 (lists `$#`,`$?`,`$$`,`$!` as clean operands) and
DD#31 (`$(( ))` accepts arithmetic on clean operands).

**Root cause.** The string codegen path (`lower_value`/`lower_var_value`,
values.rs) special-cases the computed specials with no slot — `$` → `sash_getpid`,
`?` → the exit-status var. The **int** path (`lower_value_as_int`) had no such
handling: it looked up a variable literally named `$` / `?`, found none, and
returned the undefined-var 0. So any special read as an int OPERAND (compound
arith) silently became 0; only a standalone `$(( $$ ))` survived because its
*result* is lowered through the string path.

**Fix (native).** `lower_value_as_int` now calls a new `special_var_as_int`
helper (`$` → getpid, `?` → exit-status var) before the slot lookup, mirroring
the string path; `strip_ssa_version` extracted for the base-name match. Threaded
`exit_status_var: Option<Variable>` through `lower_value_as_int` and its 13 call
sites (all callers already had it in scope). c already emitted `sash_getpid()`
/ the exit-status temp correctly; janet errors on `$$` in arith (filed).

Test-first: arithmetic.sh 31.20–24 (`in_modes native,c`; janet skips —
bug-janet-special-var-arith). 31.20/21/22 failed pre-fix (0/0/7), pass post-fix
(1/1/8); 31.23 ($?=0 case) and 31.24 ($# regression pin) already passed. plan
62→67. Full sweep: native 0 failing (872→877), c 13 / janet 36 == baseline, 0
regressions (latest-run diff). codegen_tests 179/77 and e2e 89/41 == HEAD
baseline (stash-compared: brush_arith_quoted/pos_exit_status fail identically on
HEAD — SB5006 `**` rot / true-false rot, not mine).

**Filed** (distinct, silent-wrong but separable): bug-arith-keyword-pid-intrinsic-zero
(`(getpid)`/`(getcpid)`/`(random)` sexpr in arith → 0; only `len` is wired as an
arith int-intrinsic; also `$(getpid)` command-sub in arith → SB4020) and
bug-janet-special-var-arith. So DD#75 §1 arith-equivalence (`$$` ≡ `$(getpid)`)
holds for the sugar now; the keyword/sexpr side is the filed follow-up.

## dd75-special-var-sugar (§1) DONE — compiler core (2026-08-14)

Both spellings already lowered to one node and were accepted; this reframes the
canonical spelling in the compiler surface + pins the equivalence:
- `$$`≡`$(getpid)`, `$?`≡`$(status)`, `$!`≡`$(getcpid)` verified equal; specs
  misc.sh spv.1–4. spv.1–3 guarded `native,c` — janet execs the getpid intrinsic
  and mis-tracks arith-false status (extended bug-janet-special-var-arith).
- SB5012 diagnostic leads with the canonical form: "$(status) is not available
  … (the punctuation sugar $? reads the same value)". No spec asserted the old
  text.
- DD#75 §1 doc: "Implemented (compiler + specs)" note. DD#24 clean-sources: a
  note that `$?`/`$!`/`$$` are sugar for the canonical clean intrinsics.

Deferred per DG convert-as-follow-up rule → converter-special-var-sugar
(sash-convert emit keyword forms + GUIDE/SUBSET public prose). The compiler
accepts both spellings so nothing is blocked.

native 881 / 0 fail; c 13 / janet 36 == baseline; 0 regressions.

## Remaining DD#75: only §5 (args-model) left

The invasive one — function args → implicit 1-indexed `args` array; `$1`→
`${args[1]}`, `$@`→`@args`, `$#`→`${#args}`, `$*`→`$(join args)`, `$*` removed.
Most invasive; a focused effort (parser + transform positional handling +
type_check). Left for a dedicated pass.
