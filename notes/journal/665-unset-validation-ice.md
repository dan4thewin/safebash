title: Unset validation ICE
date: 2026-11-22

# Unset validation ICE

DG reported an ICE from `_tmp/t2.sb` on the ERR-attempt trial. The AI
reproduced compiler exit 101 with the checkout binary at `d518b2f1`.
The triggering statement was `unset choices[$pre]` inside an uncalled
function. This was a semantic-validation failure, not ERR recovery or a
backend fault.

## A pending diagnostic is not an established invariant

`check_fill_target` already rejects a nonliteral `unset` operand with
SB5063. The semantic driver collects that diagnostic alongside other
checks, however; it does not stop immediately. The subsequent collection
alias census visits every function definition, including unused ones.
Its `collection_identity_uses` called the shared `unset_targets`, which
called `literal_name_of` and panicked before the queued diagnostic could
be printed.

The earlier taint caller already protected the same decoder with its own
literal-only check. Copying that guard into the alias census would leave
two callers responsible for the decoder's premature invariant.

`unset_targets` now returns an optional complete target list. A nonliteral
operand returns `None`; early taint and alias analyses leave rejection to
SB5063. Type checking and lowering require successful decoding explicitly,
because their semantic gate has actually completed. The duplicate taint
precheck is removed. Literal target separation, flag dropping and split
bracket reattachment retain their behavior.

The first implementation exceeded the touched-function cognitive limit.
Flattening flag skipping and bracket reattachment into one match brought
the decoder from cyclomatic/cognitive 5/5 to 5/3, without adding a helper
or changing the normalization contract. The ordinary gate is unchanged.

This repair does not add computed `unset` indices or relax the existing
binding-target rules. The unchanged reported file now exits 1 in native
and C modes with SB5063 at 14:4 and SB5031 at 13:26 for the unassigned
`$sysver`. It produces no artifact and no ICE.

## Regression evidence

Three checks in `spec/collparam.sh` cover the dynamic cell target inside an
uncalled function/loop, a quoted target inside a called function, and a
literal target followed by an indirect target with `--no-taint`.
Each requires compiler exit 1, SB5063 and its unset-specific message.
The new IDs are `noalias.18` through `.20`: history already contains `.17`,
although the preceding live source block ends at `.16`.

The frozen original compiler fails exactly these three checks in each
mode; all 158 retained checks pass. The repaired compiler passes all 161
per mode. A focused reversion makes the alias caller demand a literal
target list again: exactly the same three checks fail per mode, with the
158 retained checks still passing. After restoring that reversion, source
and compiler hashes matched the pre-flattening positive. The subsequent
flattening passes the complete final gate.

`NEW=6 FLIPS=0 RETIRED=0 TOTAL=12859 mk -p 1 gate` passes, including
`mk test`, both static censuses and the native/C sweep. Rows
7347475-7360333 contain 12,859 distinct checks: 6,544 native and 6,315 C,
12,846 passing and the 13 existing C skips. The six first-ever IDs are
exactly the new checks in both modes; all 12,853 previous statuses are
retained unchanged. No flips, retirements, ghosts or plan mismatches.
The liveness control reports 246 historical changes. Formatter, strict
Clippy, totality and touched-function complexity checks pass.

The reported input, runtime archive and both local lockfiles remain
byte-identical. Session evidence is in `files/t2-ice-20261121.y20IzO/`:
the frozen compiler/input, original backtrace, focused TSVs and logs,
reversion results, final diagnostics, gate log and artifact identities.

The fix stays on `err-attempt-trial`. DD136 remains provisional; this
does not adopt the trial or advance main.
