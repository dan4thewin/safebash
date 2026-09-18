---
title: The dead code was the symptom
date: 2026-09-18
---

# The dead code was the symptom

Two functions in `transform/statements.rs` carried five cold lines
between them, and they looked like ordinary over-generality:

- `attach_redirects_to_result` — line 327, the `redirects.is_empty()`
  early return, never taken in 258 calls; line 332, the
  `vec![block, result_block]` two-block case, never taken either, so
  `result_block` is always `block`.
- `attach_redirects_to_last_inst` — the empty-block return, the
  `Pipeline` arm, and the `_ => {}` catch-all, all at zero.

Five dead lines in code whose whole job is to attach redirects. The
question that opened it: what shape would put a `Pipeline` at the end of
a *simple command's* block? A function whose body is a pipeline, inlined
under a redirect. So I wrote `fn f { /bin/cat f | /usr/bin/tr a b; }`
followed by `f > o.txt` and diffed it against bash.

It diverged. So did the control.

## The bug

```
fn f { echo hi; }
f > out.txt
```

sash accepts this, prints `hi` to stdout, and never creates `out.txt`.
Bash writes `hi` to the file. Every redirect operator behaves the same
way — `>`, `>>`, `<`, `2>`, with or without arguments — and every one is
**accepted**. Builtins, externals, bare pipelines, and a redirect written
*inside* the body all work correctly. It is specifically a redirect
attached to a user-function call.

`--emit-c` settled the cause without any instrumenting:

```c
_blk_2: /* call_f */
    const char* __t1_argv[] = { "/bin/echo", "hi", 0 };
    __exit_status = sash_run(...);        /* no sash_redirect at all */
```

against the correct form, which brackets the call with `sash_dup(1)`,
`sash_redirect(1, "out.txt", 1)` and a restore. The redirect does not
survive the transform.

`lower_redirected_stmt` lowers the body and then attaches the redirects
to the last inst of `block` and `result_block`. A user-function call is
inlined, so the body lands in an intermediate block — `_blk_2: call_f` —
that neither of those names. The redirect attaches to nothing and is
dropped without a diagnostic.

Which is exactly what the five cold lines were telling me. `result_block`
is never distinct because nothing that produces distinct blocks ever gets
this far; the empty-block return never fires because the blocks it *would*
fire on are the ones nobody redirects successfully; the `Pipeline` and
catch-all arms are unreachable because the only shape that would reach
them is the shape that silently fails. The dead code was not
over-generality. It was the outline of a feature that does not work.

## The comment again

`lower_redirected_stmt`'s doc still says:

> For compound bodies (while/for/if/case/group/subshell/**function**),
> bracket with `__redirect_push`/`__redirect_pop`.

That machinery was deleted along with `lower_redirected_function` when
the redirected-*definition* bug was fixed. The comment describes a path
that no longer exists, and reads as though function bodies were handled.
Third time in four journals that a stale comment was load-bearing in the
wrong direction.

## Not mine to fix

The silent drop has to go — the project already settled that, when
`x=5 >f` was accepted-and-dropped and became SB4046. But *how* is a
language-surface question:

- **support** `f > out.txt` (bash-ordinary, and unlike `f(){…} >file`
  it is not an exotic corner), or
- **reject** it with a spanned SB4046-style error (cheap, consistent,
  but removes a common form from the subset).

Filed as `notes/tasks/bug-function-call-redirect-dropped.md` with the
evidence. DG decides.

## Also landed

`attest_map_form_error` (type_check.rs) built the "requires a map"
message in a function whose 10 calls all returned early — the diagnostic
had never once been produced. It is reachable and correct; nothing had
asked for it. Specs `93.55`–`93.58` now do, for an indexed array, a
scalar, an int, and a positive map control.

The obvious probe misses it, in the way that is becoming a theme: if the
attest condition does not validate *both* binders, an earlier check fires
and masks the one under test. Sweep 5605, predicted exactly.

One loose end noted while there: against an **undeclared** name the same
message asserts "'zz' is a scalar", where every other read site says
"assigned nowhere (possible typo)" (SB5031/SB5039). That is a false
statement about a name that does not exist, and it sends a user with a
typo'd map name to "use scalar attest". Raised with DG rather than
changed, since it is an error-code surface.
