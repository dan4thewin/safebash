---
title: "ICE: a capture as the subject or pattern of the out-array `match`/`matchall`"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "every program in the table below compiles and runs, or is rejected with a diagnostic, in both backends — none panics"
  - "`if match $(f) \"a(b)\" m { echo \"${m[1]}\" }` prints `b`"
  - "the forms that work today (through a variable, the 2-argument `match`, `$(match …)`, `[[ =~ ]]`) are unchanged"
  - "specs pin the fixed forms in both backends, including one under `--no-taint` for an external capture"
---

## Found

2026-10-26, while porting F7 (batch-531 finding B85, journal 536). It was
probing `match` in condition position. The bug is not part of B85 and the
port does not touch it. It is **pre-existing**: the same panics occur at
`fec83f7` (before F7), with F7, and on the batch-531 reference branch.

The out-array form `match S P ARR` / `matchall S P ARR` panics the compiler
when `S` or `P` is a command substitution. Measured at `fec83f7` and with
F7, both backends; in the native backend the compiler exits 1, in C 101:

| Program | Default | `--no-taint` |
|---|---|---|
| `fn f { echo ab }` then `if match $(f) "a(b)" m { echo "${m[1]}" }` | ICE: `FuncCall (f) should have been lifted during transform` | same |
| the same with `- match …`, `match … \|\| echo no`, `matchall …` | ICE, same | same |
| `fn p { echo "a(b)" }` then `if match $x $(p) m { … }` (pattern) | ICE, `FuncCall (p)` | same |
| `if match $(/bin/echo ab) "a(b)" m { … }`, also with `"$( … )"`, `&&` or `while` | SB5020 (tainted) | ICE: `non-intrinsic command substitution must be lifted to a pipeline` (codegen/value_handlers.rs:259, emit_c.rs:3430) |
| `if match $(- /bin/echo ab) "a(b)" m { … }` | SB5020 | ICE: `CmdSubstBody should have been lifted during transform` |

The two `should have been lifted` panics are raised at `ssa.rs:2130`
(`ssa.rs:2083` at `fec83f7`).

These work, in both backends:

| Program | Result |
|---|---|
| `fn f { echo ab }` then `if match $(f) "a(b)" { echo y }` (2 arguments) | prints `y` |
| `y=$(match $(f) "a(b)")` | prints `[ab]` |
| `s=$(/bin/echo ab)` then `if match $s "a(b)" m { … }`, `--no-taint` | prints `b` |
| `if [[ $(/bin/echo ab) =~ "a(b)" ]] { echo y }`, `--no-taint` | prints `y` |

Probes: `files/port-f7-probes.tar.gz`, directory `m/` (m1–m16).

## Cause (read, not yet confirmed by a fix)

`transform/stmt_handlers.rs::try_handle_match_into` lowers the subject and
the pattern with a bare `expr_to_value`, so a capture reaches the backends
as a `FuncCall`, `CmdSubst` or `CmdSubstBody` value, not as a lifted
temporary. `handle_test_command`, a few lines below, runs
`functions::lift_test_captures` for exactly this reason. This is the same
shape as `bug-fn-capture-in-test-operand-ice` (done), which fixed the
`[[ ]]` path only.

## Fix direction

Lift both operands before building `MatchInto`, through the lifting the
other operand paths already use. Do not write a private lifter (see the
shared-walker rule in AGENTS.md). A taint-enabled spec can pin only the
function-capture forms; the external forms need `--no-taint`.
