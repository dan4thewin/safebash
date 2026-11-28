title: "SB5041 says 'used as a command' for an arithmetic use"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "reconcile the implementation with DD81/DD86's unique hoisted definitions before changing SB5041 advice"
  - "cover forward command, capture, pipeline and integer-value calls without reviving execution-order lookup"
  - "account for existing SB5041 specs rather than treating their old verdict as the current language contract"

## Reassessment, 2026-11-28

Deferred from DG's unattended low-complexity batch. The current native and C
compilers still give SB5041 for the example below, its bare `f` arithmetic
counterpart, and `f; fn f { echo hello; }`. But
[DD81](../decisions/DD81-function-call-discipline.md)'s first rule and
[DD86](../decisions/DD86-variable-scope.md)'s sixth require one hoisted
definition: calls on either side mean the same body. DD81 explicitly strikes
the old execution-order lookup rule.

The current rule, not a define-before-use restriction, is authoritative.
A wording-only fix would disguise the resolution defect. This owner now
requires separately scoped forward-definition work and a migration of the
old SB5041 assertions, rather than changing four use-kind producers merely
to justify the rejection. No compiler behavior or message was changed for
this item. Native/C evidence is retained with the unattended session's
diagnostic controls; the other selected items continue.

## Original report (superseded remedy)

The former DD#81 Part A rejected a definition preceded, in execution order, by a use of
the name that did not reach the function (SB5041). The message assumes the
earlier use was a command:

```
echo $(( (f) + 1 ))
fn f -> int { return 7; }
```

```
prog.sh:2:1: error [SB5041] 'f' is used as a command before this definition — a name is either a command or a function, not both (rename, or invoke the external via an absolute path)
```

Here `f` was never used as a command, and the advice does not apply: there
is no external to invoke. Defining `f` above its first use was the suggested
workaround, not the current hoisting contract.
The same text appears for a bare `f` in `$(( ))`. This wording predates F3:
the pre-port binary gives the same text for a top-level arithmetic use.
F3's END-definition examples (56.97, 56.98) are retired by uniform SB5071
(journal 566), but the program-level reproduction above remains applicable.

Cause: `VersionState::externally_used` is a set of names, so the emission in
`transform/statements.rs` `lower_function_stmt` cannot tell what kind of
use came first. Four functions insert into it, one per kind of use:
`cmd_handlers.rs` `handle_function_call_or_command` (a command),
`stmt_handlers.rs` `push_command_stage` (a pipeline stage),
`expr_handlers.rs` `resolve_single_cmdsubst` (a capture) and `functions.rs`
`defined_int_function_body` (arithmetic). The original proposed fix recorded
the first use's kind with the name and picked the text by kind; the
reassessment above supersedes it.

Found during F3 (journal 541). F3 touched `lower_function_stmt`, but the fix
needs the kind from the four producers, which F3 did not touch. A neutral
rewording at the one site would drop the absolute-path advice that the
command case needs. So it was filed rather than fixed (decide-and-report).
