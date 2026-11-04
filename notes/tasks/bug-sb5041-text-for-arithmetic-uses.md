---
title: "SB5041 says 'used as a command' for an arithmetic use"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "an arithmetic use before the definition (`(f)` or a bare `f` in `$(( ))`) gets text naming the arithmetic use and advising to define the function before it"
  - "the command forms keep today's text, including the absolute-path advice (functions.sh pins it)"
  - "fresh specs pin both arithmetic spellings with program-level definitions"
---

DD#81 Part A rejects a definition preceded, in execution order, by a use of
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
is no external to invoke. The fix is to define `f` above its first use.
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
`defined_int_function_body` (arithmetic). Fix: record the first use's
kind with the name and pick the text by kind.

Found during F3 (journal 541). F3 touched `lower_function_stmt`, but the fix
needs the kind from the four producers, which F3 did not touch. A neutral
rewording at the one site would drop the absolute-path advice that the
command case needs. So it was filed rather than fixed (decide-and-report).
