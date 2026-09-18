---
title: "BUG: a redirect on a user-function CALL is silently dropped (`f > out.txt`)"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "`fn f { echo hi; }; f > out.txt` either writes out.txt or is a clean compile error — never silently writes to stdout"
  - "same for `>>`, `<`, `2>`, and with arguments"
  - "no redirect reaches codegen without either taking effect or being rejected"
---

# A redirect on a function call is silently dropped

```
$ cat p.sh
fn f { echo hi; }
f > out.txt
$ sash p.sh -o prog && ./prog
hi                     <-- went to stdout
$ ls out.txt
ls: cannot access 'out.txt': No such file or directory
```

Bash writes `hi` to `out.txt`. sash accepts the program, ignores the
redirect, and writes to stdout. **Silently wrong output** — the failure
mode the project already ruled unacceptable for `x=5 >f` (fixed to
SB4046; see `bug-redirected-function-def-unregistered`, and the note at
`spec/errors.sh:1154`, "discard it in the transform, diverging silently").

## Scope — measured

Every redirect operator on a user-function call is dropped: `>`, `>>`,
`<`, `2>`, and with arguments. All of these are **accepted** and none
takes effect.

Controls all work correctly: `echo hi > out.txt`, `/bin/echo hi >
out.txt`, `/bin/cat f | /usr/bin/tr a b > out.txt`, and a redirect
written *inside* the function body.

## Cause — confirmed from `--emit-c`

```c
/* fn f { /bin/echo hi; }  ;  f > out.txt  */
_blk_2: /* call_f */
    const char* __t1_argv[] = { "/bin/echo", "hi", 0 };
    __exit_status = sash_run(...);          /* no sash_redirect at all */

/* /bin/echo hi > out.txt */
    i64 __t0_saved = sash_dup(1);
    i64 __t1_rf = (sash_redirect(1, "out.txt", 1) < 0);
    ... sash_dup2(__t0_saved, 1); sash_close(__t0_saved);
```

`lower_redirected_stmt` lowers the body, then calls
`attach_redirects_to_result(ssa, block, result_block, …)`, which inspects
**only** `block` and `result_block` and attaches to each one's *last*
inst. A user-function call is **inlined**, so its body lands in
intermediate blocks (`_blk_2: call_f`). Neither `block` nor
`result_block` holds the command, so the redirect attaches to nothing and
is discarded without a diagnostic.

## The stale comment that hid it

`lower_redirected_stmt`'s doc (statements.rs:234-236) still says:

> For compound bodies (while/for/if/case/group/subshell/**function**),
> bracket with `__redirect_push`/`__redirect_pop`.

That machinery was **deleted** by the earlier redirected-definition fix,
which removed `lower_redirected_function` and the `function_redirects`
push/pop. So the comment describes a path that no longer exists, and
reads as if function bodies were handled when they are not.

## Found by

The coverage grind, again. Five cold lines in this area
(`attach_redirects_to_result` 327/332, `attach_redirects_to_last_inst`
344/353-355/357) were all symptoms: `result_block` is never distinct, the
empty-block early return never fires, and the `Pipeline` and catch-all
arms are unreachable — because the only shape that would exercise them
is the one that silently fails.

## Fix — approach fork, for DG

- **A. Support it.** Attach the redirect around the whole inlined body
  (re-introduce a push/pop bracket, or attach to every block the body
  produced). Matches bash. `f > out.txt` is ordinary, everyday bash —
  unlike `f(){…} >file`, which DG rejected as a counter-intuitive corner.
- **B. Reject it** with a spanned SB4046-style error ("a redirect cannot
  attach to a function call"). Cheap, consistent with the existing
  rejection family, and honest. But it removes a common bash form from
  the subset.

Either way the silent drop must go. **A** preserves the subset; **B** is
a real restriction. Recommend **A** unless DG wants the subset narrower,
since the whole point of the earlier SB4046 work was that a dropped
redirect must never be silent — and rejecting an everyday form is a
bigger subset change than the corner case that established the rule.

Whichever is chosen, fix the stale `lower_redirected_stmt` comment and
re-check the five cold lines above: under **A** several become live;
under **B** they should be deleted as unreachable.
