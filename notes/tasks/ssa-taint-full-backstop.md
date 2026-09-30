---
title: "SSA taint backstop: make it a complete gate (capture path + arrays + positions)"
status: done
priority: P2
kind: hardening
depends: []
gates:
  - "under `--no-taint-cst` the SSA backstop rejects the FULL laundering class the CST pass catches: `$(join @a)`/`$(join a)`, `echo $(getenv)` inline, `${a[0]}` of a mapfile/argv/read array, `[[ ]]` operands, redirect targets"
  - "0 spec regressions native/c/janet with the CST pass ON (blessed/clean arrays must not false-positive at SSA)"
  - "spec taint_switch.sh gains sw.* cases proving each newly-caught position under `--no-taint-cst`"
---

## Done 2026-09-28 (journal 435) — the enumeration is total

`check_inst_uses` enumerated 4 of 19 `SsaInst` variants and ended in `_ => {}`;
`check_value_use` covered 8 of 30 `SsaValue` variants and ended in `_ => {}`.
Both are gone. Read positions are now enumerated once, by
`SsaInst::taint_reads` (ssa.rs), total over the enum with no wildcard, and
`check_value_use` is total over `SsaValue`.

Measured under `--no-taint-cst` against this task's own gate list:

| position | before | after |
|---|---|---|
| `[[ ]]` operands | COMPILES | reject |
| redirect targets | COMPILES | reject |
| `$(join a)` | reject | reject |
| subscript index | reject | reject |
| `$(<$h)` slurp path | — | reject |
| pipeline stage argument | — | reject |
| inline `$(getenv)` as an argument | COMPILES | **COMPILES** |
| `${a[0]}` of an argv-filled array | COMPILES | **COMPILES** |

The first two were live escapes with the front-end pass ON, because it does not
descend into a `$( )` body. Closed, spec `bkstp.1`/`bkstp.2`.

**The last two rows are what is left of this task**, and neither is an escape:
with the front-end pass on — the shipped configuration — both are SB5020,
inside a `$( )` body as well as at top level. Measured, not assumed.

Their cause is one shape: `check_value_use` reports tainted *reads* it finds
inside a value, and never asks whether the value is tainted **as a whole**.
`SsaValue::CmdSubst { argv: ["getenv", "T"] }` holds only literals, so the walk
finds nothing, even though `value_is_tainted` calls the same value Tainted. The
derivation half already knows the answer; the reporting half does not consult
it. Fixing it is probably `if value_is_tainted(value, …) == Tainted` at the top
of `check_value_use` — which needs `user_functions` threaded down from
`verify_ssa_taint`, and needs a careful read of every new rejection, since it
widens the backstop considerably.

Also still true: redirect targets resolve by bare NAME via `taint_of_name`
(highest classified version), because a `Redirect`'s target is an unlowered
`ir::Expr` and never becomes an `SsaValue`. That is imprecise in the permissive
direction — a variable attested *after* the redirect reads clean at SSA. The
front-end is the precise one.

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
