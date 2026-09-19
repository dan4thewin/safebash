# 372 — the redirect that went into the capture

A cold-line question — *what input reaches the `emit_redirects` call inside
`emit_cmdsub_exec`?* — turned out to have the answer **nothing useful**, and
finding out why cost two bugs.

## The bug

```bash
x=$(/bin/echo hi > side.txt)
```

Bash captures nothing and writes `side.txt`. sash captured `hi` and never
created the file. No error, no warning. The redirect simply did not happen and
the output went wherever the capture was pointing.

Every file redirect behaved this way — `>`, `>>`, `<`, `2>file`,
`>/dev/null`. The dup forms (`2>&1`, `1>&2`) worked, which is exactly why this
survived: the form people reach for first is the one that was fine.

## Two defects, not one

The cold line was **live and correct**. `strace` settled it in one command:
the `2>&1` child did `dup2(4,1)` then `dup2(1,2)`, so `emit_redirects` runs.
The `>file` child did `dup2(4,1)` then `execve` and **no `openat` at all**.

So the redirect was lost before it could be emitted — but instrumenting the
transform, `handle_cmdsub` and `emit_cmdsub_exec` showed `redirects=1` at all
three points. It reached codegen intact.

**Defect one (native).** `Interner::intern_value` walks a value's children via
`SsaValue::child_values()`, which yields `SsaValue`s. A redirect target is an
`ir::Redirect` — not an `SsaValue` — so the walk *cannot* see it. Statement
redirects are interned by a dedicated `intern_redirect_targets`; a capture had
no equivalent. The path literal was therefore missing from the literals map.

And `emit_open_redirect` responded to a missing literal like this:

```rust
let &data_id = literals.get(s.as_str())?;   // silently skips the whole redirect
```

with a doc comment saying so outright: *"the redirect is silently skipped
(matches pre-refactor behavior)"*. A missed compiler invariant was turned into
**wrong output** instead of a crash. That is the whole reason this was silent
rather than a loud ICE, and it is the more important of the two findings. The
same file already has `redirect_target_ice`, added because the *non-literal*
path's silent `(0,0)` was a bug. The literal path kept its silent skip.

**Defect two (C backend).** `--emit-c` showed no redirect code at all, for any
case. `emit_c.rs` destructured `SsaValue::CmdSubst { argv, suppressed, .. }`
and called `sash_capture_status(argv, envp, &len, &st)` — a runtime helper
that takes **no redirects**. The C backend dropped every capture redirect,
including the `2>&1` native handled. A pre-existing backend divergence that no
spec had ever caught.

## The fix DG chose

The decisive measurement was that `$(a | b > f)` already worked **in both
backends**. A pipeline capture takes the *body* path, where redirects are
lowered by the ordinary statement machinery. Only the single-command fast path
was broken — and it was broken because it re-implemented redirects itself, in
one backend.

So rather than repair two fast paths and keep them in step by hand, DG's call
was to delete the fast path's redirect support: `single_command_subst` now
declines when the statement carries a `Redirected` wrapper, and `$(cmd > f)`
lowers through `CmdSubstBody` like `$(a | b > f)` always has.

Three lines. **8/8 against bash in both backends**, where before it was 2/8 on
native and 0/8 on C.

The two things I expected to pay for, I didn't:

- **No extra fork.** I predicted a subshell; `strace` says one `clone` with the
  redirect and one without. I had assumed the cost instead of measuring it, and
  I was wrong.
- **No semantic drift.** DD#55 abort, `$(- cmd >f)` suppression and taint all
  behave exactly as the unredirected form does.

With that, `SsaValue::CmdSubst.redirects` and the `emit_redirects` block in
native's `emit_cmdsub_exec` became unreachable, and DG called for deleting them
rather than leaving them. `CmdSubstBody.redirects` went too — it was
constructed `vec![]` and every reader destructured it away, so it had never
carried anything.

The silent skip is now an ICE.

## What the method produced this time

Five bugs into the coverage grind, the pattern is unchanged: **the cold line is
rarely the bug.** `emit_cmdsub_exec`'s cold block was live, correct, and is now
deleted — not because it was wrong, but because asking what reached it exposed
an interner that couldn't see redirects and a backend that ignored them.

The corollary worth keeping: *a silent fallback converts a missed invariant
into wrong output.* Both defects here were invisible for the same reason — one
skipped a redirect it couldn't resolve, the other dropped a field it didn't
model. Neither said anything.

**Specs.** `csr.1`–`csr.9` in `commands.sh`, covering builtin and external
heads, `>`/`>>`/`<`/`2>file`/`2>&1`/`>/dev/null`, a non-literal target (the
`redirect_s` path), and the DD#55 abort. Sweep **5747, 0 failing, 0 flips**
(5729 + 9×2, predicted exactly); units green (92).

`csr.9` failed on its first run for a harness reason worth remembering: the
suite separates **compile-time stderr (`err`) from runtime stderr (`rerr`)**,
and a DD#55 abort is a runtime error. Asserting on `$err` passes vacuously in
the other direction — it was only the *inverted* assertion here that caught it.
