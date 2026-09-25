---
title: "SSA taint backstop: make it a complete gate (capture path + arrays + positions)"
status: deferred
priority: P2
kind: hardening
depends: []
gates:
  - "under `--no-taint-cst` the SSA backstop rejects the FULL laundering class the CST pass catches: `$(join @a)`/`$(join a)`, `echo $(getenv)` inline, `${a[0]}` of a mapfile/argv/read array, `[[ ]]` operands, redirect targets"
  - "0 spec regressions native/c/janet with the CST pass ON (blessed/clean arrays must not false-positive at SSA)"
  - "spec taint_switch.sh gains sw.* cases proving each newly-caught position under `--no-taint-cst`"
---

## Done so far (DD#82, journal 187)

`cmd_subst_taint` now PROPAGATES operand taint for the CmdSubst-form transform
builtins (`echo`/`dtoi`/`itod` scalar, `join`/`keys` via `collection_reader_taint`)
instead of hardcoding them Clean — matching every other transform value. Under
`--no-taint-cst` this makes the SSA backstop catch e.g. `x=$(dtoi $tainted); echo $x`
(spec sw.8). SSA reject messages neutralised ("SSA taint backstop", no longer
"CST taint pass missed this") since `--no-taint-cst` makes an SSA reject expected.

## What still does NOT reach the SSA backstop (the hard part)

Discovered while wiring DD#82 (empirically, with `--no-taint-cst`):

1. **Capture-form builtins bypass `cmd_subst_taint` entirely.** `$(join @a)` /
   `$(echo …)` lower via `CaptureStart`/`CaptureEnd{target, tainted}`, NOT a
   `SsaValue::CmdSubst`. `CaptureEnd.tainted` is a boolean set at *transform*
   time (external=exec → true, builtin → false), before taint analysis runs —
   so a builtin capture is Clean regardless of what it reads. `cmd_subst_taint`
   is never called for these. Fixing needs the capture's inner command args to
   be taint-checked, or `CaptureEnd` taint to be derived post-analysis.

2. **Array taint is not classified at SSA.** `classify_inst_def` leaves
   `Mapfile`/`ReadArray`/`ArgvFill` in the no-def group ("CST pass governs").
   A naive `taint.insert(format!("{array}_0"), Tainted)` did NOT work — array
   SSA versioning + `taint_of_name`'s max-version lookup meant the read still
   resolved Clean, and `${a[0]}`/`@a` uses were not flagged. Needs the correct
   versioned key (whatever a subscript/splat read resolves to) and a
   `BlessRetag`-clears check to avoid false-positives on blessed arrays.

3. **Inline `$( )` args and Test/Redirect positions** are not covered by
   `check_inst_uses` (Command/Pipeline/SubscriptAssign/ArrayAppend only).
   Redirect targets are IR `Expr`s in SSA (not `SsaValue`), so they need Expr
   walking, not `check_value_use`.

This is a bigger architectural change than the original
`ssa-taint-cmdsub-operand-propagation` task assumed (which is now folded in /
superseded by this one). The CST pass remains the authoritative, complete,
position-uniform gate (DD#24 amendment, taint.sh 60.37–60.55); this task is
defense-in-depth so `--no-taint-cst` is a genuine second gate.

## DG's ruling (2026-09-12) — deferred, conditionally

> "So long as all scenarios are caught by the earlier taint pass, all
> defense-in-depth should be deferred."

The condition is testable, so it is recorded here rather than assumed.

**Measured 2026-09-12.** Three laundering shapes on a `read` array —
`$(join a ,)`, `${a[0]}`, and `[[ "${a[0]}" = … ]]` — each compile to a
**working binary** under `--no-taint-cst` and print the tainted data. Scalar
taint is caught in both the direct (`$(getenv HOME)` → `echo`) and laundered
(`$(dtoi $s)`) forms. So the backstop today covers scalars and not
collections, exactly as the three gaps above describe.

**The condition holds.** Every one of those shapes is rejected **SB5020** by
the CST pass in a normal compile, and the position-uniformity specs that pin
CST completeness (`taint.sh` 60.37-60.55) pass in native and c, with a single
janet skip.

**What would reopen this.** The deferral rests entirely on the CST pass being
complete. Any CST taint hole found — of the kind SB5021 and the two-arg `read`
taint bug both were — invalidates the premise and makes this urgent again,
because nothing else catches array-shaped laundering.

Worth noting for whoever picks it up: `run_ssa_taint_gate` skips only on
`--no-taint`, so the SSA gate **does** run during ordinary compilation. This is
a live second gate with a collection-shaped hole, not merely a `--no-taint-cst`
debugging aid.

## Re-measured 2026-09-25 (journal 408) — the boundary is sharper than "arrays"

Gap 2 above says "array taint is not classified at SSA". That is true of the
*bulk-read* origins and **not** of the literal, which matters when picking up
this task:

| origin | SSA inst | backstop under `--no-taint-cst` |
|---|---|---|
| `array a=($(getenv HOME))` | `Assign` | **rejects** — value carries the cmdsub's taint |
| `read a < f` | `ReadArray` | compiles, leaks |
| `mapfile a < f` | `ReadArray` | compiles, leaks |
| `argv 1 a` | `ArgvFill` | compiles, leaks |

So `check_tainted_array` is reachable today, by the literal only; it is now
pinned by spec `sw.10` (with `sw.11` as the clean-array control). The three
leaking rows are all SB5020 in a normal compile — re-verified 2026-09-25, so
**DG's deferral condition still holds**.

The shared cause of the three is `classify_inst_def`: `ReadArray` / `ArgvFill`
/ `SplitValue` / `MapFromArray` define no `VarRef`, so they sit in the no-def
group and never enter the taint map. Gap 2's note about versioned keys is the
real obstacle — `taint_of_name`'s max-version lookup is what a naive insert
fails against.
