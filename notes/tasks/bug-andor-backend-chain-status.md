---
title: "BUG — janet/c backends evaluate && / || chains by value-truthiness, not exit status"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "janet & c run nested && / || chains by exit status (matching native)"
  - "spec wordsplit 62.8-62.10 pass under all 3 modes (then drop the in_modes native guard)"
---

# BUG — janet/c `&&`/`||` chains use value-truthiness, not exit status

## Discovery

Found 2026-06-19 while fixing the `&&`/`||` left-associativity bug
(`bug-andor-right-associative`). That parser fix is correct (native
verified); it surfaced this **pre-existing** janet/c backend defect via
the new nested-chain tests. The chains mis-evaluated regardless of
associativity, so this predates the parser fix.

## Repro (janet)

```
$ sash --janet -c '[[ 1 = 1 ]] && echo A || echo B' | janet
A
B          # WRONG — bash/native: just A
```

Emitted janet:

```
(when (not (do (when (do (= "1" "1")) (print "A")))) (print "B"))
```

The `||` branches on `(not <left-block-value>)`. The left `&&` block ran
`(print "A")`, whose **value is nil**, so `(not nil)` → true → `(print
"B")` also runs.

## Root cause

`emit_janet.rs::emit_list_and`/`emit_list_or` (≈309-329) condition on the
Janet **value** of the left block (`(when (do left) right)` /
`(when (not (do left)) right)`), not on the exit status. Two problems:

1. A successful left block's value (e.g. `print` → nil) is falsy, so `||`
   wrongly fires.
2. Janet tracks `__last_exit`, but **not every command sets it** — e.g.
   `echo`/`print` leaves the previous value — so naively switching to
   `__last_exit` isn't enough.

The c backend (`emit_c.rs`) has the analogous issue (spec 62.9 fails
there; 62.8/62.10 happen to pass).

## Fix plan

1. Make **every** command emission set the status var (`__last_exit` in
   janet; the c equivalent) — echo/print/builtins included (0 on success).
2. Rewrite `emit_list_and`/`emit_list_or` to run the left for effect, then
   branch on the status var:
   - `&&`: run right iff status == 0.
   - `||`: run right iff status != 0.
   Mirror in `emit_c.rs`. Match native's DD#55 reset semantics for the
   chain's resulting status.
3. Native already does this via `test_result_var`/`__last_exit` (the SSA
   `lower_list_stmt`), so it is the reference.

## Tests

`spec/wordsplit.sh` 62.8–62.10 are currently guarded `in_modes native`.
Once fixed, drop the guard so they run under janet+c too. Add a
`printf`/`echo`-in-chain case to pin the per-command status setting.

## References

- AGENTS.md "Bug policy"
- `sash/src/emit_janet.rs:309` (emit_list_and), `:320` (emit_list_or)
- `sash/src/emit_c.rs` (&&/|| lowering)
- `bug-andor-right-associative` (the parser fix that surfaced this)

## Investigation (2026-06-21) — concrete map, ready for a focused effort

Confirmed the root cause and mapped the full change surface (janet;
c mirrors it). The fix is well-defined but spans the whole command-
emission path, so it wants a dedicated session with full re-verification.

**janet pieces (emit_janet.rs):**
- `emit_list_and` (~309) / `emit_list_or` (~320): stop wrapping the left in
  `(do …)` and branching on its value. Instead: `emit_stmt(left)` for
  effect, then `(when (= 0 __last_exit) <right>)` for `&&` /
  `(when (not= 0 __last_exit) <right>)` for `||`. With status-setting
  commands these compose correctly (the chain's resulting `__last_exit` is
  the last-run command's, matching bash).
- Every command must leave `__last_exit` set. Today only true/false do
  (`emit_janet_const_bool`, ~680). Needed:
  - `emit_janet_echo`, `emit_janet_printf`, `emit_janet_cat` (stdin) → 0.
  - `emit_janet_test_cmd` (~639): currently emits a bare bool value;
    change to `(set __last_exit (if <test> 0 1))`.
  - `emit_janet_general_command` (~678): `(os/execute […] :p)` returns the
    exit code → `(set __last_exit (os/execute …))`. **Edge case:** the
    user-function call branch `(f args)` returns a value, not a status —
    decide its `__last_exit` (native uses the callee's `$?`).
  - The simple-success builtins can be set centrally in `emit_command`
    (~601, which has `depth`) after the match, EXCEPT the kinds that set
    their own (test, general-command, true/false) or are control flow
    (exit/return/break/continue). Mind the indent (emitters get one
    `indent(depth)` from the dispatch; a second line needs its own).

**c pieces (emit_c.rs):** the analogous `&&`/`||` lowering + `__exit_status`
must be set by every command emitter (echo already sets it to 0;
audit the rest). 62.9 fails in c; 62.8/62.10 pass by luck.

**Tests:** drop the `in_modes native` guard on wordsplit.sh 62.8–62.10;
add a `printf`/`echo`-in-chain case to pin per-command status. Verify the
WHOLE spec + rust suites under all 3 modes (broad blast radius: every
program's command/list emission changes).
