# 187 — SSA taint backstop: operand propagation (partial); the hard part scoped

Follow-on to the DD#82 switches (journal 186). With `--no-taint-cst` now able
to exercise the SSA taint pass in isolation, I implemented the deferred
`cmd_subst_taint` operand propagation and discovered the SSA backstop is much
further from a complete gate than the filed task assumed.

## Done

`cmd_subst_taint` (ssa_taint.rs) PROPAGATES operand taint for the CmdSubst-form
transform builtins instead of hardcoding them Clean:

- `echo` / `dtoi` / `itod` → `any_tainted(args)`
- `join` / `keys` → `collection_reader_taint` (splat value **or** bare
  array-name literal → `taint_of_name`)
- `len` + generators (`argc`/`epochseconds`/`random`/`getpid`/`getcwd`/…) stay
  Clean; sources (`getenv`/`argv`/`arg`) tainted; externals tainted.

This matches every other transform value (`Concat`, `RegexReplace`, `Ternary`,
…) which already propagate. Under `--no-taint-cst` it makes the backstop catch
`x=$(dtoi $tainted); echo $x` (spec sw.8). SSA reject messages neutralised
("… without bless (SSA taint backstop)") — under `--no-taint-cst` an SSA reject
is *expected*, not an ICE, so "CST taint pass missed this" was wrong.

0 regressions with the CST pass on (propagation only changes `--no-taint-cst`
behaviour; a tainted transform operand is CST-rejected upstream by default).

## What I found does NOT reach the SSA backstop (empirically, --no-taint-cst)

1. **Capture-form builtins bypass `cmd_subst_taint`.** `$(join @a)` /
   `$(echo …)` lower via `CaptureStart`/`CaptureEnd{tainted}`, not a
   `SsaValue::CmdSubst`. `CaptureEnd.tainted` is a boolean fixed at transform
   time (exec → true, builtin → false), before taint analysis — so a builtin
   capture is Clean whatever it reads. `cmd_subst_taint` is never called.
2. **Array taint isn't classified at SSA.** `Mapfile`/`ReadArray`/`ArgvFill`
   sit in the no-def group ("CST governs"). A naive `insert("{array}_0",
   Tainted)` did not work — array versioning + `taint_of_name` max-version
   meant reads still resolved Clean, and `${a[0]}`/`@a` uses weren't flagged.
   Reverted; needs the correct versioned key + `BlessRetag`-clears handling.
3. **Inline `$( )` args, `[[ ]]` operands, redirect targets** aren't in
   `check_inst_uses`; redirect targets are IR `Expr`s in SSA, needing Expr
   walking rather than `check_value_use`.

## Decision

Ship the propagation (principled, correct, 0-risk, exercised by sw.8) and
**scope the rest accurately** as `notes/tasks/ssa-taint-full-backstop.md` (P2)
— the capture path + array classification + extra positions are a bigger
architectural change than a one-function tweak. The CST pass stays the
authoritative, complete, position-uniform gate (DD#24 amendment, taint.sh
60.37–60.55); the SSA layer is defense-in-depth, and `--no-taint-cst` remains a
partial second gate today (scalar sources + CmdSubst transforms), a complete
one once the task lands.

## Verification

Full `SPEC_MODES="native c janet"` sweep: 0 regressions, native gate 0.
taint_switch.sh sw.8 (dtoi propagation, native). DD#82 unchanged in intent;
task `ssa-taint-cmdsub-operand-propagation` superseded by `ssa-taint-full-backstop`.
