title: "fail-open `/* TODO */` arms remain in emit_c.rs (the P1 fix covered only command dispatch)"
status: done
completed: 2026-09-13
priority: P2
kind: bug
depends: []
gates:
  - "every `_ =>` arm in emit_c.rs that emits a `/* TODO … */` comment and returns an empty/zero value is either implemented or converted to an ICE"
  - "a reachability probe for each arm is recorded in the task or its journal entry, so an 'unreachable' claim is measured rather than asserted"
  - "no spec regressions"

# The fail-open class was only half-closed

`fail-open-todo-arms-in-emit-c` (P1, closed in `bf96d25`) made
`emit_command_body`'s dispatch exhaustive with ICE terminals, and fixed
`unset m[k]`. It did **not** sweep the rest of the file. As of 2026-09-13 six
`/* TODO */` fall-through arms are still live, each emitting a comment and
returning `("\"\"", "0")` or `"0"` — the C compiles and produces a wrong value.

| site | arm | reachable? |
|------|-----|-----------|
| 2634 | `emit_test_binary_op` — test binary op | **YES** — `<` and `>`; see `bug-c-string-ordering-always-false` (P1) |
| 3365 | redirect op | **YES** — `<&`, `<>`; see `bug-fd-dup-input-redirects-broken` |
| 1341 | `lower_value_as_str` | not yet reached by probe |
| 2711 | `ir_expr_as_str` | not yet reached by probe |
| 2791 | param expansion op | not yet reached by probe (`:s/` is the documented candidate; `${x/l/L}` rejects at SB4034 and `${x:s/l/L/}` emitted 0 TODO arms) |
| 3152 | cmdsub: non-literal cmd | not reached — `x=$($c hi)` rejects earlier (SB5050) |

The two marked YES are filed separately because they are user-visible wrong
answers today. The remaining four are the actual content of *this* task: each
needs a reachability probe, and then either an implementation or an ICE.

## The principle (DG, this session)

> Unsupported is not a legitimate state.

No DD sanctions a c-backend subset. A fall-through arm means either a real gap
(implement it) or an impossible state (ICE). Emitting a comment and a zero is
neither — it is the compiler asserting a value it has not computed.

"Not yet reached by probe" is **not** the same as unreachable. The probes above
were a single afternoon's worth of guessing at inputs; the c-string-ordering
bug sat undetected behind exactly that kind of reasoning. An arm should only be
called unreachable when the type system makes it so, which is what
`eliminate-ice-arms-via-types` is about.

## Stale in the old filings

`cmdsub-followups-taint-intrinsics-c` (P3) still lists as a gate: *"the C
backend implements cmdsub exec (currently `/* TODO cmdsub exec … (Phase 4) */`
→ empty capture)"*. That is **done** — `emit_cmdsub_exec_c` exists and now
carries the DD#55 diagnostic. Its other gate about `$(argc)` / `$(signum)` also
measures clean (0 TODO arms). That task needs re-measuring before any work.

## Completion (2026-09-13)

Closed by **enumeration, not probing**. `SsaValue` has 28 variants;
`lower_value_as_str` + `lower_value_as_str_aggregate` handled 21. Of the 8
missing, 3 (`CaseConvert`, `RegexReplace`, `RegexMatch`) are intercepted
upstream by `lower_string_transform_c` and never reach the dispatcher, and
the other 5 (`FuncCall`, `IntFuncCall`, `CmdSubstBody`, `ArraySplat`,
`ElideEmpty`) are exactly native's ICE set in `codegen/values.rs`. Mirrored
those 5 as explicit ICE arms plus an ICE terminal. Same treatment for
`ir_expr_as_str`, the parameter-expansion operator dispatch, `emit_cmdsub`'s
non-literal head, and (with the `<&`/`<>` work) the redirect fall-through —
the last one.

**One arm turned out to be a validator gap, not a codegen gap.** The
non-literal cmdsub head: `$c hi` in statement position is SB5011, but
`$($c hi)` was **accepted**, then aborted at runtime in native
(`sash: bug: bare command passed to sash_exec`) and silently captured the
empty string in c. `check_cmd_in_stmt` only ever visited
`StmtKind::Command`, and `child_bodies()` returns empty for
Assignment/Declaration/Command, so command-substitution bodies — which live
inside *expressions* — were never walked.

The first fix over-rejected: running the full `check_commands` on cmdsub
bodies turned `$(getenv HOME)` and `$(argc)` into SB5073 ("value intrinsic
in statement position"), which is exactly right in statement position and
exactly wrong inside `$( )`. **Only the head-must-be-literal rule
transfers.** Narrowed to a dedicated `check_cmdsub_heads`; spec `23.24`
pins that value intrinsics inside `$( )` are not dragged through statement
rules.

Specs `23.20`–`23.24` (`spec/commands.sh`, plan 92 → 97), including `23.22`
(nested `$( $( $x ) )` is reported exactly once — the walker is
deliberately shallow because `check_cmdsub_heads` already recurses).

Nothing reached an ICE across the 4270-test sweep.
