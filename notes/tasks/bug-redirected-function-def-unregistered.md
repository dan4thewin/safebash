title: "BUG: a redirected function definition `f(){…} >file` isn't registered (SB5011 on call)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`f() { echo hi } >/tmp/x; f` defines and runs f (or is a clean compile error), not SB5011 'command f not found'"
  - "the existing `lower_redirected_function` transform path is reachable"

## Resolution (2026-08-28) — reject + delete (DG-chosen, option B)

DG: `f(){…} >file` is an ultra-rare, counter-intuitive bash corner (a
persistent per-call redirect that "falls out of the grammar") — not in the safe
subset. A redirect may only attach to a **command**. Now rejected uniformly with
one clear spanned **SB4046** ("a redirect cannot attach to {a function
definition | a variable declaration | a variable assignment} — redirects attach
to a command"), in `wrap_pipeline_part` (parser). This also fixed two sibling
gaps found alongside: `int x=5 >f` (was the misleading SB4014) and `x=5 >f`
(was **silently accepted and the redirect dropped**). `parse_declaration_
assignments` now stops at a redirect token so a declaration reaches the reject.

The dead `lower_redirected_function` (and its `lower_redirected_stmt` dispatch)
plus the now-unreachable `function_redirects` machinery (ssa.rs field/init,
cmd_handlers push/pop) were deleted. Specs `commands.sh` 14.13–14.16. native+c 0
regressions.

## Symptom (found via the coverage grind)

A function definition with a trailing redirect isn't registered as a function,
so a later call to it fails resolution:

```
$ sash -c 'f() { echo hi } >/tmp/z; f'
-c:1:26: error [SB5011] command 'f' not found — use absolute path or define as function
$ sash -c 'f() { echo hi }; f'          # baseline: prints hi
```

## Cause (likely)

The parser wraps the definition as `Redirected(Function)`. But
`validate`'s function collection (`collect_functions` /
`collect_function_names`) does not descend into a `Redirected` wrapper, so `f`
never enters the known-functions set → the call to `f` is rejected SB5011 before
transform. The transform code that WOULD handle it —
`transform/statements.rs::lower_redirected_function` (dispatched from
`lower_redirected_stmt`) — exists but is unreachable, which is why coverage
flags it as never-run.

## Fix — approach fork (present to DG)

- **A. Support it**: make function collection descend into `Redirected` (and
  any other wrapper that can hold a `Function`) so `f` is registered; the
  redirect applies to the function body per `lower_redirected_function`
  (already implemented). Matches bash `f() { … } >file`.
- **B. Reject it** cleanly: a redirect on a function *definition* is a spanned
  SB error (not the misleading "command f not found"), and remove the now-dead
  `lower_redirected_function`.

DG decides whether `f(){…} >file` is in the subset. Either way the current
misleading SB5011 (and the dead-but-present transform path) should be resolved.

## Found

Coverage grind, probing inputs to reach the never-run
`lower_redirected_function`. The never-run signal pointed straight at a
half-wired feature.
