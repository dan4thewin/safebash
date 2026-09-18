---
title: A name collision that laundered an execve
date: 2026-09-18
---

# A name collision that laundered an execve

`cmd_subst_taint` (`ssa_taint.rs`) classifies the head of a command
substitution for the DD#82 SSA taint backstop. Its doc comment is
explicit about the fallthrough:

> Everything else runs via execve/execveat — tainted.

The code said something else:

```rust
if user_functions.contains(cmd.as_str()) || user_functions.contains(basename) {
    Taint::Clean
```

`basename` is `cmd.rsplit('/').next()`. So for a program that defines a
user function named `cat` and captures `$(/bin/cat f)`, the second test
matches, and a real `execve` is classified **Clean**.

## Measured, not argued

The branch data said both operands were cold, which is where this
started. Rather than reason about the call sites, I instrumented the
function to print `cmd`, `basename`, and both membership tests, then ran
it over a spread of programs.

```
--- external, attested
    CSTin cmd="/bin/cat" base="cat" c1=false c2=false
--- external, fn shares basename
    CSTin cmd="/bin/cat" base="cat" c1=false c2=true      <-- Clean
```

Two facts came out of that run, and the second was the surprise:

1. `c2` is true **only** in the collision case. There is no correct use
   of it: DD#23 rewrites an external head to an absolute path and
   nothing else acquires a slash, so a user function is always already
   spelled as a bare `cmd`.
2. `c1` is **never true at all.** I tried five shapes of user-function
   capture — with args, inside a pipeline, with a redirect, one that
   itself runs an external, and a recursive one (rejected, SB5037) — and
   not one of them reached the function. A user-function capture is
   lowered elsewhere and never arrives at this arm.

So the condition was never right and sometimes wrong. The fix keeps the
bare-name test (correct if anything ever does route here, and it cannot
produce a false Clean) and drops the basename fallback.

## Severity, stated honestly

This does not let a bad program through *today*. `validate`'s AST taint
pass runs first and rejects the unattested collision program with
SB5020, which is exactly why my first probe looked like a clean bill of
health — the rejection I saw came from the earlier pass, not from the
one I was testing. The backstop is defence in depth, and what was broken
is its ability to catch a `validate` bug on that class of input. A
backstop with a hole in it is worth fixing on its own account; it is the
layer that is supposed to be sound when the layer above is not.

It also explains why the branches were cold rather than hot: nothing in
the suite names a function after a binary.

## Specs

Four pins in `taint.sh` (`dd82.1`–`dd82.4`): the collision program
rejects unattested and runs attested, a no-collision control, and a
DD#24 control where a user function that *itself* runs an external is
Clean by design. They pass before and after the fix — they pin the input
class, not the defect, because the defect has no user-visible signature
while `validate` shadows it. Sweep 5591, predicted exactly.

The lesson worth keeping: the first probe "passed" for the wrong reason.
Seeing the expected rejection is not evidence that the pass you are
testing produced it.
