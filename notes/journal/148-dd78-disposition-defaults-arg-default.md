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
