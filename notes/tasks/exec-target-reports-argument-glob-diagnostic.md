---
title: "`exec` target with a `*` reports SB5029 \"in command arguments\" alongside SB5095"
status: open
priority: P4
kind: diagnostic
depends: []
gates:
  - "`exec /bin/ech*` reports one diagnostic, and it names the exec TARGET rather than a command argument"
---

# What happens

```
$ sash -c 'exec /bin/ech*'
error [SB5029] inline `*` glob is not allowed in command arguments — use an array: `array a=(*); … @a` (or quote "*" for a literal asterisk)
error [SB5095] `exec` target '/bin/ech*' is not an executable program — no such file
```

Two diagnostics for one fault, and the first one is misleading: `/bin/ech*`
is the `exec` **target**, not an argument, so "use an array" is advice that
cannot apply — an array splat is not a legal `exec` target either.

# Why it is only P4

The program is still rejected, and SB5095 — the diagnostic that names the
real fault — is present and correct. This is diagnostic *quality*, not a
wrong answer: nothing compiles that should not.

# Where it comes from

`exec`'s target sits in argv position, so the SB5029 arg-position glob check
sees it before the head-position rule does. Found 2026-10-10 while removing
`head_is_unjudgeable` (journal 488); deliberately left in scope-discipline
rather than widening that change.

The fix is presumably to exempt argv[0] of an `exec` from the SB5029 scan,
since `check_exec_target` already owns that position and reports it via the
shared `command_path_diag` authority. Confirm that is the only position with
this shape before changing the scan — the guard is keyed on syntax, and that
has bitten before (journal 440).
