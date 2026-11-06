---
title: "Resolved policy questions from F6's function classifier (journal 547)"
status: done
priority: P3
kind: question
depends: []
gates:
  - "DG's Q1-Q4 rulings are recorded in DD55/DD58/DD86"
  - "unimplemented enforcement has explicit backlog owners; closing this question record does not claim those fixes"
---

F6 unit A (journal 547, DD#55 addendum 2026-10-30) classifies every function
as int, status or void and enforces DD#55 at the call. Four shapes surfaced
while writing it; each repro below was compiled with the F6 unit A binary.
All four policy questions are settled. Q2 and Q4 were implemented in
journal 577 (`enforce-function-return-kinds`). Q3 rejects duplicate
definitions (DD86, journal 582); enforcement remains pending in
`reject-duplicate-function-definitions`. Q1 requires integer captures to
consume only the returned integer through redirects too (DD58, journal 583);
`bug-int-capture-redirect-switches-kind` owns that pending repair.
This record is closed as a question, not as implementation.

DD58's 2026-11-05 integer/status ruling settles implicit int-to-status
conversion. Pipeline-stage enforcement is in `q-int-function-pipeline-status`;
return-producer compatibility is in `enforce-function-return-kinds`.
A valid integer-value discard is distinct from interpreting an integer as
a status. Q1 preserves typed capture rather than restoring arithmetic-only
integer calls.

## Q1. Integer capture has one result channel — decided, enforcement pending

DG ruled on 2026-11-06 that capturing an integer function consumes only
its returned integer. A redirect cannot select stdout instead. Stdout
writes remain legal side effects; no purity restriction is introduced.
`$(c)` and `$(c 2>&1)` therefore both produce c's integer, while
`$(- c)` and `$(- c 2>&1)` both require SB4076.

The original F6 observation below records the behavior to replace, not
the accepted contract:

```sh
fn c -> int { echo out; return 7 }
int n=$(c)          # prints "out" on stdout; n=7
str y=$(c 2>&1)     # SB4016 "returns an int — capture with $((c)) or discard with `-`"
str y=$(- c 2>&1)   # y=out
```

F6 rejected `$(- c)` but allowed `$(- c 2>&1)` to capture stdout.
That split is now rejected. A non-integer wrapper can use ordinary `- c`
to discard the integer and expose stdout through its own capture; for
example, `fn output_c { - c }` followed by `$(output_c)`.
`bug-int-capture-redirect-switches-kind` owns the native/C fix, including
keeping integer captures out of SB4075's stdout-writer test. Anonymous
capture caller-status handling remains separate in
`q-status-function-ceremony`.

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

## Q3. Duplicate function definitions — decided, enforcement pending

DG chose "reject duplicates" on 2026-11-06. One function name has one
source definition: same-kind, same-signature and identical repetitions
are errors too, even when uncalled. Names stay global and hoisted; this
is neither a new function scope nor an overloading rule.

F6 checked the definition in force at each call (nonvoid.25-nonvoid.29),
which allowed one name to be void at one call and a status at the next.
The recorded example now requires rejection at the second definition:

```sh
fn f { echo two }
f
fn f { return 3 }    # duplicate definition: compile-time error required
f
```

DD86 and DD55 record the contract. The new P1 task
`reject-duplicate-function-definitions` owns enforcement and related
definition/class bookkeeping. The existing
`bug-int-call-by-name-redefinition` remains open until its reproducers
are rejected, rather than being repaired to support replacement.
Function placement is already settled separately: definitions are
main-list-only, not legal inside EXIT/ERR.

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
