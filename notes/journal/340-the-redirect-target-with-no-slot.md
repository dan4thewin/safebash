# 340 — the redirect target with no slot

Sweep **5223 passing, 10 skipped, 0 failing** — predicted exactly
(3 specs × 2 modes = +6). Units 83 green.

Found while probing the two cold `continue` guards in
`init_positional_params`, ahead of DG's dead-code purge. The probe was
meant to establish whether `var_name == "?" || var_name == "$"` was
provably unreachable and could go. It wasn't. It was load-bearing, and
wrong.

## Two bugs, one gap

`$$` (pid) and `$!` (last background child) are first-class readable
values. All of this has always worked:

```sh
str s=$$
/bin/echo hi > $s      # writes to a file named by the pid
```

Used *directly* as a redirect target, both went wrong, in two different
ways:

| input | before |
|---|---|
| `/bin/echo hi > $$` | **ICE** — `unresolved redirect target — variable $$ has no slot` |
| `/bin/echo hi > $!` | **compiled**, redirected to the EMPTY filename |

The cause is one gap with two faces. Redirect-target codegen is
slot-based (`redirect_expr_simple_expansion` → `find_highest_version`),
but `$$` and `$!` are served by intrinsic codegen, not by a `VarSlot`.
So:

- `$$` hit the guard above, which **skips** it deliberately ("served by
  other mechanisms — no VarSlot needed"). True in value position, false
  here. No slot was ever created, and the lookup ICEd.
- `$!` had no guard at all, so the pre-scan happily built it an **empty**
  slot, and the redirect silently went to `""`.

That second one is the `bug-redirect-nonsimple-target-empty` class —
the exact failure `spill_scalar_redirect_targets` was written to close,
leaking through a case its predicate did not name.

## The fix is the mechanism that was already there

`target_needs_spill`'s doc comment describes the remedy precisely: a
target the "ad-hoc redirect-target codegen, which only knows literals,
`$x`, and concatenations of those" cannot lower gets bound to a
synthetic scalar via `expr_to_value`, and the target is rewritten to
read that scalar. `${x:-f}` and `$a[i]` already take that path.

`$$` and `$!` belong in the same bucket for the same reason, so they
now say so:

```rust
ExprKind::SimpleExpansion(v) => is_slotless_special(v),
```

`> $$` now means exactly what `str s=$$; > $s` always meant. The
concat case falls out for free, because `target_needs_spill` already
recurses into `DoubleQuoted` / `Concat` parts — `> "$$.log"` works.
Both backends, native and c.

With no background child, `$!` is empty and `> $!` is a runtime "No
such file or directory" — same as the indirect spelling. Not silent
corruption.

## What this says about the purge

The guard I was about to delete as dead was reachable; it was cold only
because no spec redirected to `$$`. **A cold branch is not a dead
branch** — it is an unasked question, and the answer here was a bug.

After the fix the guard *is* genuinely dead: the spill rewrites the
target to `__redir_target_N` before codegen's scan ever sees it, so
neither `$` nor `!` can reach the pre-scan. It comes out with the rest
of Task A rather than on its own.

Specs `cov.250`–`cov.252`.
