title: bug: getenv and status bypass arity validation, ICE at runtime
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`getenv` and `status` with wrong arity are rejected at compile time"
  - "`getenv` / `status` in statement position (not command substitution) are rejected"
  - "no accepted program can reach `sash: bug: bare command passed to sash_exec`"
  - "`$(getenv NAME EXTRA)` is an error, not a silent ignore"

# bug: `getenv` / `status` escape every validation gate

Found while reviewing `getenv-presence-handling` (2026-09-12).

`getenv` and `status` are recognized as intrinsics well enough to be **exempt
from command resolution** (so SB5011 never fires on them), but they have **no
arity or context validation anywhere**. Any malformed use therefore passes
every front-end gate, reaches codegen as an ordinary external command, and
the compiler emits a binary that dies at runtime on an internal assertion:

    sash: bug: bare command passed to sash_exec (expected absolute path)

An accepted program must never be able to print `sash: bug:`.

## Measured

| source | result |
|---|---|
| `$(getenv)` | compiles → `sash: bug:` panic |
| `$(getenv HOME EXTRA)` | compiles → **extra arg silently ignored**, yields `/home/dg` |
| `$(getenv HOME A B)` | compiles → extras silently ignored |
| `$(status X)` | compiles → `sash: bug:` panic |
| `$(status X Y)` | compiles → `sash: bug:` panic |
| `getenv` (statement position) | compiles → `sash: bug:` panic |
| `status X` (statement position) | compiles → `sash: bug:` panic |

Contrast — these are all correctly rejected at compile time:

| source | result |
|---|---|
| `$(nosuchcmd)` / `nosuchcmd` | SB5011 |
| `$(argv)` / `$(argv 1 2)` | SB5025 |
| `$(len a extra)` | SB4045 |
| `$(join a , extra)` | SB5040 |

## Root cause

Arity is enforced in `type_check.rs`. Every other intrinsic has an entry
there — `len`, `join`, `keys`, `match`, `matchall`, `read`, `readline`,
`argv`, `attest`. **`getenv` and `status` are the only two intrinsic names
with no `type_check.rs` presence at all**, which is exactly why they are the
two that fall through.

Their dispatch sites are `codegen/value_handlers.rs`, `emit_c.rs`,
`ssa_taint.rs`, `validate.rs`, `transform/expr_handlers.rs`, and
`parse/expr.rs` — i.e. they are handled late, in lowering/codegen, on the
assumption that a well-formed call is the only thing that can arrive.

## Fix sketch

Give both an arity/context check in `type_check.rs` alongside the others:

- `getenv` — exactly 1 argument today (see the note below), command
  substitution only.
- `status` — exactly 0 arguments, command substitution only.

Then confirm no path can still hand a bare intrinsic name to `sash_exec`.
The `sash: bug:` runtime assertion should stay as a backstop, but it must
become unreachable from accepted source.

## Interaction with `getenv-presence-handling`

That task weighs giving `getenv` an optional second argument
(`$(getenv NAME DEFAULT)`). The 2-arg form **already parses today and
silently discards the default** — which is the worst possible state: it
looks like it works. Whichever way that decision goes, this bug decides what
the *unsupported* arity does. If the default-argument design is adopted, the
arity check becomes 1-or-2 rather than exactly-1.

## Scope note

Only `getenv`/`status` were found in this blind spot, by comparing every
intrinsic name's dispatch sites against `type_check.rs`. Worth re-running
that comparison as part of the fix in case a newer intrinsic lands the same
way.
