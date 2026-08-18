# 148 — DD#78 disposition-default discipline + DD#60 `arg N DEFAULT`

DG, reviewing sash-native they'd written (`dir=$(arg 1); bless dir …`),
proposed `$(arg N)` take an optional second argument as a default rather than
abort on out-of-range. The design conversation ratified two things: DD#60's
`arg N DEFAULT` addendum, and a cross-cutting DD#78 constraining what a
disposition default may be — which turned out to close a real taint hole.

## Design conversation (decision authority)

1. Confirmed `$(arg N)` abort is *intentional* (DD#60, tied to DD#26 "no silent
   empty") and deliberately *asymmetric* with array/map subscript OOB
   (zero-value, arith-subscript-values "BUG 2 — NOT a bug"): a missing argv
   slot is usually a forgotten argument; a missing collection key is a normal
   query. The proposal is the strict-vs-default split (`:?` vs `:-`) applied to
   argv — keep abort as the default, add an opt-in fallback.
2. Spelling: **positional** `$(arg N D)` — `arg` is a builtin in `$(…)`, so it
   can't wear `${…:-}`; one index means a second token is unambiguous.
3. Eval: **eager** (mirrors `emit_default_value`'s branchless `select`). DG:
   eager is fine but side effects must not sneak in → discuss constraints.
4. Constraint: fork-free wasn't enough — DG: "`:-` is an expansion, a read not a
   store, so we can't allow any tainted value to sneak through." Landed on:
   **the default must be clean + non-`random`**, applied uniformly to `:-`/`:?`
   AND `arg N D`. Taint classification (cmd_subst_taint) makes `$(getcwd)`
   clean (DG's example ✓) but `$(getenv)`/`$(arg)` tainted; `random` is the one
   clean-but-*stateful* intrinsic. "Clean" subsumes fork-free (externals are
   tainted).

Recorded: **DD#78** (new) + **DD#60 addendum**.

## The taint hole DD#78 closes (a real pre-existing bug)

`check_expr_taint`/`expr_uses_tainted` walked a `:-` disposition's *var* but
not its *operand*, so a tainted default slipped through:

    y=$(getenv HOME); echo ${x:-$y}     # COMPILED (should reject)
    ls ${x:-$(getenv HOME)}             # COMPILED
    echo ${x:-$(date)}                  # COMPILED — also ran date eagerly
    z=${x:-$(getenv HOME)}; echo $z     # z left CLEAN — full escape

The store path didn't even propagate (`expr_uses_tainted` returned false for the
operand → `z` clean → tainted data flowed out through `z` unchecked). Both
primitives had the hole.

## Stage 1 — DD#78 for `:-` (DONE)

- `expr_uses_tainted`: recurse into the `:-` operand → correct propagation
  (`z=${x:-$tainted}` now taints `z`).
- `check_expr_taint`: recurse into the `:-` operand → a tainted read in a
  default is rejected as a use-without-bless (SB5020), exactly like
  `echo $tainted`. (`:?` and pattern operators carry a *literal* operand,
  DD#65, so nothing to check.)
- `check_disposition_side_effects` (new structural pass, walk_all_exprs):
  a `:-` default that invokes `$(random)` → **SB5035** (eager eval would advance
  the PRNG whether or not the fallback is taken).

**Semantics: clean-when-read.** A tainted default is rejected at a use site;
in a bare store (`z=${x:-$y}`) the taint *propagates* to the target and is
caught when the target is read — so there is no sneak-through anywhere. (This
is slightly looser than "error exactly at the store"; both are safe. Chosen to
avoid disturbing the delicate direct-source/quoted propagation exemptions
`z=$y` / `z="$y"`. Tightening to error-at-store is a small follow-up if wanted.)

Tests: `spec/taint.sh` dd78.1-7 (tainted var / `$(getenv)` / `$(arg)` default
reject; `$(getcwd)` / literal accept; `$(random)` → SB5035; store no-escape).
plan 68→75. Full 3-mode sweep: **0 passing→failing, +21 passing**.

## Pre-existing Rust-suite rot (noted, not mine)

`cd sash && cargo test` (i.e. `mk test`) fails heavily at HEAD *before* my
changes — brush_compat 24/28, taint_spec 18/24, codegen 179/77, e2e 86/44 —
because those `sash/tests/*.rs` reference removed features (heredocs→SB5002,
`case`→SB4029) and old codes (expect SB4020 where the modern taint code is
SB5020). Verified identical counts with my changes stashed. The maintained gate
is the spec sweep (`mk spec` → spec.db), which is what prior sessions used. My
change adds zero new failures to any suite. Tracked: `audit-rust-test-suites`
(canonical, already filed/deferred; my counts match its findings).

## Stage 2 — DD#60 `arg N DEFAULT` (next)

Runtime `sash_argv_get_or`, 3-backend lowering, arg-count 1|2, negative index
still rejected, result stays tainted, default constrained by DD#78 (reuse the
clean/non-random check on arg's 2nd arg).

## Stage 2 — DD#60 `$(arg N DEFAULT)` (DONE)

The feature: `$(arg N)` still aborts on OOB (unchanged); `$(arg N DEFAULT)`
returns `DEFAULT` when `N` is out of range. Eager (mirrors `:-`), result stays
tainted (arg is a source), blessed downstream.

- **Runtime** (`sash-rt`): `sash_argv_get_or(idx, d_ptr, d_len, &out_len)` —
  argv[idx] in range, else the (already-evaluated) default. Any invalid index
  (negative or `>= argc`) → default.
- **3 backends**: native (`value_handlers.rs`, new `externs.argv_get_or` +
  `argv_get_or` sig in setup/types), c (`emit_c`, `sash_argv_get_or` extern),
  janet (`(if (and (>= _ai 0) (< _ai len)) (get args _ai) DEFAULT)`).
- **Validation**:
  - Arity (`check_disposition_defaults`/`flag_arg_builtin`): `arg` takes 1 or 2
    args → **SB5036** otherwise (was silently ignoring extra args — `arg 1 2 3`
    used `2` as the default and dropped `3`).
  - DD#78 clean default: a tainted default is rejected — **SB5020**. arg lives
    almost only in a store RHS (`dir=$(arg …)`, a source assignment), whose
    cmdsub args aren't otherwise use-checked, so `taint_assignment` now calls
    `check_arg_default_taint` (a read-position check on the default).
  - DD#78 non-random: `$(random)` default → **SB5035** (shared with `:-`).

**Parse limitation (pre-existing, general — noted).** A *bare* nested command
substitution as a command arg fails to parse (`$(arg 1 $(getcwd))` → SB4014;
`p=$(dirname $(getcwd))` fails the same way). The **quoted** form works:
`$(arg 1 "$(getcwd)")`. So an intrinsic default must be quoted. Not arg-specific.

**janet gap (pre-existing).** janet exec's `$(getcwd)` (and the getpid-class) as
fake external commands rather than intrinsics, so `$(arg 9 "$(getcwd)")` fails on
janet — guarded av.d4 native,c; tracked in `janet-getpid-class-intrinsics`.

Tests: `spec/argv.sh` av.d1-d7 (present→argv; absent/high-OOB→default; quoted
intrinsic default native,c; tainted default→SB5020; arity→SB5036;
random→SB5035). plan 37→44. Full 3-mode sweep: **0 passing→failing, +20
passing**, native 0-fail. clippy clean; complexity gate green.

DD#60 addendum + DD#78 scope-for-arg recorded; SUBSET.md documents `$(arg N
DEFAULT)` + SB5035/SB5036. **DD#60 arg-default + DD#78 complete.**

## Deferred (filed)

- Negative-index compile rejection (DD#60 says `$(arg -1)` should be a
  compile error; today it isn't enforced — `x=$(arg -1)` compiles, and only the
  downstream unblessed-read taint check catches the spec case av.t7). Runtime
  still aborts on a bare negative. `arg-negative-index-compile-reject`.
- Bare nested `$(…)` as a command arg (general parser limit) —
  `bare-nested-cmdsub-arg`.
