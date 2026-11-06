---
title: "Questions left by F6's function classifier (journal 547)"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the remaining Q1/Q3 questions; DD55 records the ruling"
  - "the specs the chosen option flips are rewritten, and new specs pin it in both modes"
---

F6 unit A (journal 547, DD#55 addendum 2026-10-30) classifies every function
as int, status or void and enforces DD#55 at the call. Four shapes surfaced
while writing it; each repro below was compiled with the F6 unit A binary.
Q2 and Q4 were settled by the 2026-11-06 return-kind ruling and implemented
in journal 577 (`enforce-function-return-kinds`). Q1 and Q3 remain questions.

DD58's 2026-11-05 integer/status ruling settles implicit int-to-status
conversion. Pipeline-stage enforcement is in `q-int-function-pipeline-status`;
return-producer compatibility is in `enforce-function-return-kinds`.
Q1's choice of capture surface remains separate. A valid integer-value
discard is distinct from interpreting an integer as a status.

## Q1. An int function's capture means two things

`$(c)` on an int function binds the int, and the body's stdout goes to the
real stdout. A redirect makes the capture body a statement, so the same call
captures stdout and must discard the int:

```sh
fn c -> int { echo out; return 7 }
int n=$(c)          # prints "out" on stdout; n=7
str y=$(c 2>&1)     # SB4016 "returns an int — capture with $((c)) or discard with `-`"
str y=$(- c 2>&1)   # y=out
```

So `$(- c)` is SB4076 but `$(- c 2>&1)` is legal. Question: should one
capture spelling mean one thing (for example, `$(c)` of an int function is
an error that points at `$((c))`), or does the split stand?

## Q2. `-> int` with no `return` — implemented

```sh
fn f -> int { echo hi }
int n=$(( f ))      # prints hi; n=0
```

The F6 binary compiled this and supplied zero. Under DG's 2026-11-06 rule,
the missing integer is an error: a returning path must supply a result.
It is not a ceremony error, and SB4017 is being retired. An explicit
integer `return 0` is legal. The new kind/completeness task owns the
missing-result refusal, implemented in journal 577 with SB4037 even for an
uncalled definition.

## Q3. A redefinition that changes a function's class

A call is checked against the definition in force where it stands (DD#55
addendum 2026-10-30, nonvoid.25-nonvoid.29), so one name can be void at one
call and a status at the next:

```sh
fn f { echo two }
f                   # legal: the f in force is void
fn f { return 3 }
f                   # 4:1 SB4016 "returns a status"
```

No DD says whether a name may be defined twice, or which body a call runs.
DD#86 says only that the compiler hoists the name. The transform inlines the
definition lowered last before the call, in textual order. Question: should a
second definition of a name be rejected, or at least one that changes the
name's class? `q-fn-definitions-in-end` is related, and
`bug-int-call-by-name-redefinition` is a miscompile of the same shape.

## Q4. A redirected tail call is a status — implemented

```sh
fn g { echo hi }
fn f { return g >/dev/null }
f                   # SB4016 "returns a status"
```

`return g` alone made f void, as g is. With a redirect, unit A
classifies the producer as a status, because the redirect is part of the
producer. A redirect-open failure aborts whether or not `-` is present
(`q-suppression-on-user-functions` y02/y08/y11), so the status the call can
return is g's own.

The accepted kind-preserving forwarding rule settles this: a redirect does
not invent a status result. f remains void when it forwards void g;
status/integer forwarding likewise retains its kind. This is part of
`enforce-function-return-kinds`, not a new exception to the return rule.
Journal 577 implements kind preservation and checks that a redirect-open
failure cannot become a void or integer result.

Pointers: `sash/src/transform/functions.rs` (`classify_producer`,
`exit_summary`, `returned_callees`, `classify`, `static_classes`), `ssa.rs`
(`VersionState::call_class`), `statements.rs` (`lower_function_stmt`),
`stmt_handlers.rs` (`check_non_void_bare_call`), `expr_handlers.rs`
(`check_function_capture`).
