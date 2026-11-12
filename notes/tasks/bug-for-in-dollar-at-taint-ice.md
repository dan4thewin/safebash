title: BUG — for-loop variable from $@ is taint-mishandled, causes ICE
status: done
priority: P0
kind: bug
resolved: 2026-06-16
resolution: "Closed by the argv/arg/argc trio task chain (argv-trio-*, DD#60).\nRoot cause was three latent gaps:\n  - DD#46 not enforced for top-level $@ codegen — bypassed the\n    validator into a working-but-DD-violating program-argv splat path\n    (ArgvIndex), and the SSA-taint backstop ICE'd on the tainted loop\n    var the CST pass never saw.\n  - DD#42 $(argv N) was scalar-only — no clean way to express\n    \"iterate user args\" at top level.\n  - DD#22 tolerated decorative quotes around array splats, masking the\n    real surface form.\n\nFix:\n  - DD#60 (new): C-semantics argv/arg/argc trio. $(argv)/$(argv N) are\n    array splats (for-in / array-snapshot only); $(arg N) is the scalar.\n  - check_positionals (SB5024) rejects top-level $@/$*/$1..$N/$# with a\n    directed message, BEFORE transform — so the ICE path is unreachable.\n  - Inside a function, $@/$* expand to the caller's args ($1..$N) at\n    inline time (was incorrectly splicing program argv — a second bug).\n  - $(argv N) for-in lowers to an argv-slice loop (0-based, $(argc)\n    bound); array a=$(argv N) snapshots into a tainted indexed array.\n  - sash-convert rewrites top-level positionals to the builtins,\n    context-aware (inside-function forms preserved per DD#46).\nresolution-spec-tests: cf.f4, cf.dd46.1-6, cf.f11-14, e.dd46.t1-5, av.*\n"
gates:
  - "`for x in \"$@\"; do echo \"$x\"; done` (top level) → SB5024 (was ICE) ✓"
  - inside-function `for x in $@` / `echo "$@"` iterate the caller's args ✓
  - replacement `for x in $(argv 1); do bless x …; done` works ✓
  - cf.f4 passes (native + c) ✓

# BUG — for-loop over $@ ICEs when loop var is used (RESOLVED)

## Discovery

Found while writing `spec/control_flow.sh` cf.f4. The natural sash
pattern `for x in "$@"; do echo "$x"; done` ICEs at compile time
instead of either compiling cleanly or producing a clean SB-error.

## Repro

```bash
$ target/release/sash -c 'for x in "$@"; do echo "$x"; done' -o /tmp/t
-c: ICE: tainted variable x_3 used as argument to command 'echo' — CST taint pass missed this
```

Variants tried — all ICE:

| script | result |
|---|---|
| `for x in "$@"; do echo "$x"; done` | ICE |
| `for x in "$@"; do echo $x; done` | ICE |
| `for x in "$@"; do /bin/echo "$x"; done` | ICE |
| `for x in "$@"; do bless x echo "$x"; done` | ICE |
| `for x in "$@"; do bless x; echo "$x"; done` | ICE |
| `for x in "$@"; do x=$(printf "%s" "$x"); echo "$x"; done` | ICE |

Variants that work (no use of $x):

| script | result |
|---|---|
| `echo "$@"` | works |
| `for x in "$@"; do :; done` | works |
| `for x in "$@"; do echo "loop"; done` | works |

So the bug is specifically: `for x in "$@"` correctly marks $x as
tainted in the SSA, but the CST taint pass (`validate.rs`
`check_taint`) doesn't catch the subsequent use, so the SSA-taint
verification gate (`ssa_taint.rs:395`) ICEs.

## Likely location

- `sash/src/validate.rs::check_taint` (dataflow at line 1108) —
  doesn't propagate taint through for-loop variable binding from
  `"$@"` (or `$1`, `$2`, etc.)
- `sash/src/ssa_taint.rs:395` — catches what CST missed and ICEs

## What user-visible behavior should be

Two valid options (decision required):

- **(a) Compile cleanly.** Iterating $@ is a natural pattern; the
  argv contents are already tainted but iterating them shouldn't
  require ceremony beyond what `echo "$@"` requires (which works).
  Would need taint relaxation in the for-loop body OR an implicit
  bless on the for-loop variable.

- **(b) Clean SB-error directing to bless.** Match the existing
  taint model — direct use of tainted argv element requires
  explicit acknowledgment. But the bless syntax for a for-loop
  variable isn't currently working (every `bless` form tried
  still ICEs), so this option requires fixing bless-for-loop-var
  first.

Either way, **no ICE**. The ICE is the bug; the user-visible policy
is a decision.

## Test coverage to add once fixed

`spec/control_flow.sh` cf.f4. If option (b) is chosen, also add a
test asserting clean SB-error AND a test showing the bless path
works.

## References

- AGENTS.md "Bug policy" — ICE is always a bug
- AGENTS.md "Taint system" — definition-based taint, argv is tainted
- `sash/src/validate.rs:1108` `check_taint` (CST taint pass)
- `sash/src/ssa_taint.rs:395` (SSA-taint gate that ICEs)
- spec.db `native:cf.f4`
