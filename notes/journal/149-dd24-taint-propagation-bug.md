# 149 — DD#24 taint bug: scalar assignment propagated instead of rejecting

DG, reviewing the DD#78 checkpoint summary, challenged my characterization of
`z=$y` as a "direct source→target assignment" that "stays legal (DD#24)":

> z=$y exemption?
> z=$y is a read of $y

DG was right, and it exposed a real pre-existing bug.

## The finding

DD#24 §Quarantine (DD#44) is explicit: `b=$a` (RHS read of a tainted `a`) is a
**rejected read**, "No propagation — taint is a property of a definition, not a
flow." Journal 036 (the DD#24 implementation journal) gives the exact case as a
worked example: `x=$1  # compile error`, "No assignment, no echo… Safebash has
no taint inside the program. This is simpler than Perl's taint mode [which]
tracked taint through every operation."

But the implementation *propagated*: `taint_assignment`'s scalar path did
`if is_taint_source(rhs) || expr_uses_tainted(rhs) { tainted.insert(target) }`
— i.e. a tainted RHS read marked the *target* tainted (Perl-style flow) rather
than rejecting. `z=$y` compiled; the taint was caught only at a later sink
(`echo $z`). Every OTHER read site rejected correctly (`arr=($y)` SB5021,
`(( y ))` SB4030, `f $y` SB5020, `m=([k]=$y)` SB5021) — scalar assignment was
the lone offender. Pre-existing since the original taint pass (65c1c26); no
spec test or converter path relied on it.

**This is the root cause of the DD#78 hole.** Propagation is only sound if
*every* sink re-checks taint; the DD#78 `:-` default was a sink that didn't, so
tainted data leaked. Under the DD#24 "reject at the read" model the leak is
structurally impossible — `${x:-$y}` rejects at the read regardless of sink
completeness. My DD#78 patch hardened one symptom; this fixes the disease.

## The fix (validate.rs)

`taint_assignment` scalar path, replacing the propagation branch:

- `is_taint_source(&a.value)` → taint the target (unchanged — assignment
  *from a source* is the sanctioned way in; `z=$(getenv X)`, `z="a$(getenv X)b"`).
- `reject_scalar_rhs_read(&a.value, …)` (new) → reject every read of a stored
  tainted variable in the RHS: `$y`, `${y}`, `${x:-$y}` (the `:-` default is a
  read position, DD#78 — rejects a tainted var *or* a tainted source there),
  `$a[i]`, and any of these inside `"…"`/concat. A command substitution in
  value position is assignment-from-source (allowed) — its `$(arg N DEFAULT)`
  default is still checked by `check_arg_default_taint` (DD#60).

Behavior change (the DD#24-intended shape): **bless at the source, not on a
copy.** `bless y […]; z=$y` (y clean → copy is a clean read) instead of
`z=$y; bless z …`. Diagnostic SB5020 says so.

Also closed as a bonus: `z=${x:-$(getenv)}` (a *source* in a `:-` default at a
store site) — a residual DD#78 hole the use-site check didn't reach.

## Verification

- Matrix (native): rejects `z=$y`, `z="$y"`, `z=${y}`, `z=${x:-$y}`,
  `z="pre$y"`, `z=${x:-$(getenv)}`, `s+=$y`; compiles `z=$(getenv)`,
  `z="a$(getenv)b"`, `bless y; z=$y`, `z=$(arg 1 def)`, `a=5;b=$a`,
  `z=${x:-plain}`, function positionals (`local n=$1`).
- Full 3-mode sweep: **0 passing→failing, 0 failing→passing** — nothing relied
  on propagation. spec/taint.sh np.1-7 (+ dd78.7 comment corrected to
  reject-at-assignment). taint.sh plan corrected 75→85 (pre-existing drift:
  actual distinct-id count). np.5/np.6 run getenv at runtime → guarded native,c
  (janet-getpid-class-intrinsics).
- clippy clean; `reject_scalar_rhs_read` cc=7/cog=3 (reuses
  `check_param_expansion_taint`); gate green.

DD#24 gained an enforcement note; the rule itself was already correct — the
implementation had drifted. My DD#78 work stays correct and sits above this.
