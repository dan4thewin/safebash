title: "bug: getenv and status bypass arity validation, ICE at runtime"
status: done
priority: P1
completed: 2026-09-12
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


## DONE 2026-09-12 — and the blind spot was 6x wider than filed

Fixed in two parts. **The filing's scope note was right to ask**: re-running
the comparison found this was never a `getenv`/`status` pair — it was
**every value-producing intrinsic**.

Measured before the fix, statement position, one program each:

| intrinsic | before | after |
|---|---|---|
| `getenv` `status` `getpid` `getppid` `getcpid` `getcwd` `epochseconds` `argc` `signum` `dtoi` `itod` `arg` | **ICE** (`sash: bug: bare command passed to sash_exec`) | SB5073 |
| `random` | SB5011 (correct — never in `BUILTINS`) | unchanged |

Eleven ICEs, not two. And in `$( )` position the arity hole was just as wide:
`$(getpid X)`, `$(argc X)`, `$(epochseconds X)`, `$(getppid X)`,
`$(getcwd X)`, `$(signum 1 2)`, `$(dtoi 1 2)` and `$(getenv HOME EXTRA)` all
**silently discarded** the extra arguments.

### Part 1 — arity (`ssa.rs::value_intrinsic_arity`)

`getenv`/`status` were not the only names missing from the table; so were the
whole nullary family. Added `Some(0)` for `status`/`getpid`/`getppid`/
`getcwd`/`epochseconds`/`argc`/`signum`/`random`, `Some(1)` for `getenv`, and
`Some(2)` for `dtoi`/`itod` (both take *digits* plus a value — `$(dtoi 1)` was
accepted before). `arg` is left out deliberately: DD#60 makes it genuinely
variadic (`$(arg N)` / `$(arg N DEFAULT)`).

This one table drives both SB4045 *and* the transform's ICE backstop
(`resolve_single_cmdsubst`), so filling it closes the cmdsub half everywhere.

### Part 2 — statement position (`validate.rs`, new **SB5073**)

The obvious fix — deleting these names from `BUILTINS` so they fall to SB5011
like `len` does — is **wrong**, and measuring it is what showed why:

- `is_valid_command` falls through to `resolve_on_path(name)`. On a machine
  with a real `getenv` on `PATH`, a bare `getenv` would then *silently exec
  it*. The same source would compile to different programs on different
  machines.
- `is_external_source` (validate.rs:3627) classifies any non-`BUILTINS` name
  as external, which is a **taint** classification shared with the transform's
  `clean_pipe` backstop.

So the names stay in `BUILTINS`, and a separate `VALUE_ONLY_INTRINSICS` list
drives a dedicated SB5073 in `check_cmd_in_stmt`. `argv` is deliberately
excluded — DD#60's `argv [N] a` out-param form is a legal statement, and
via.12 pins that.

### Verification

12 specs `via.1`-`via.12` in `spec/errors.sh`, beside the existing SB4045
block. **Fail-first confirmed**: exactly via.1-via.9 fail against a stashed
pre-fix build; via.10-via.12 pass both before and after (they are regression
guards, not discriminators).

Sweep **5035 / 38 / 514**, zero non-janet failures, **zero flips** vs
`2e35cc2`.

`via.10`/`via.11` are guarded `native,c`: janet emits
`(os/spawn ["getenv" "HOME"])` — it tries to exec a `getenv` binary, the same
bug class on the janet side. Already filed and deferred as
`janet-getpid-class-intrinsics`; not re-filed.

### Also corrected

The filing claimed `getenv`/`status` have "no `type_check.rs` presence at
all". `status` is in fact listed there (`is_int_returning_intrinsic:159`) —
that is a *type* classification, not an arity check, which is why it did not
help. The root cause stands; the wording did not.
