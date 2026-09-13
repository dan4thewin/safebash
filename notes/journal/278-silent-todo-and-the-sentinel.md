# 278 — Four bugs behind one un-gating

DG asked whether the C backend's bugs and gaps were closed, then said:
*"start by grooming the tasks, then tackle the known bugs, worst first, then
focus on the sweep."* The grooming is in the previous entry. This one is the
bugs — and there were twice as many as the backlog knew about, including one
that was corrupting the heap in **both** backends.

## The worst bug, as filed

`c-exec-support`: `BuiltinCmd::Exec` had no arm in `emit_c.rs`, so it fell to
the catch-all and emitted `/* TODO command: exec (Exec) */`. Two silent
wrongs:

- `exec CMD args` — a no-op comment. Execution continued into statements
  native would never reach.
- `exec > file` — worse than a no-op. The redirect *was* emitted, but as the
  ordinary per-command save/restore pair, so it un-did itself on the next
  line and every subsequent write went to the original stdout.

The fix mirrors native. Two things were worth getting right:

**The redirect-only form is not a command body.** Native's `emit_exec`
returns `false` from `emit_exec_redirects_only` and simply discards the saved
fds. In the C emitter the redirects are applied by the *caller*
(`emit_command`), so the suppression has to happen there. The condition
derives purely from `(kind, args)`, which both call sites already hold, so
`is_persistent_exec` is a predicate rather than a threaded parameter — no new
argument on a function already carrying `#[allow(too_many_arguments)]`.

Only the **success** path suppresses the restore. If the open fails there is
no persistent redirect to keep, so that branch restores and aborts as before.

**`args` is the whole argv.** For `exec CMD a b`, `name` is the literal string
`"exec"` and `args[0]` is the program. That rules out `emit_argv`, which
prepends a `name` literal as argv[0]. Native's `build_argv_on_stack` lowers
every element uniformly; the C side now does the same. Neither backend
expands an `@a` splat here, and that parity is deliberate.

## Un-gating found the second bug

`spec/exec.sh` had been `native`-only since the feature landed. Dropping the
guard: **18 of 19**. The one failure was not `exec` at all — `close 9` on an
unopened fd should abort with `close: 9: Bad file descriptor`, and C exited 0
silently. `BuiltinCmd::Close` had no arm either. Same catch-all, same silent
no-op.

That is two in one sitting from the same structural cause, which was reason
enough to stop and ask what else was hiding.

## The third bug: enumerate, don't guess

41 `BuiltinCmd` variants; 21 had no textual reference in `emit_c.rs` at all.
That number is misleading — most are expression intrinsics (`strlen`, `join`,
`basename`) or control flow (`break`, `return`) handled on entirely different
paths. The honest test is not "is the variant named" but "does a program using
it emit the TODO." So: one small probe per builtin, `--emit-c`, grep for
`TODO command`.

Ten of them were fine. One was not:

```
unset        *** TODO *** TODO command: __table_delete (TableDelete)
```

`unset m[k]` was a silent no-op in the C backend. The key stayed present,
`len` was unchanged, and the value still read back. Fixed by mirroring native
`emit_table_delete`; the `sash_table_delete` extern was already declared in
the emitted C, so only the emitter was missing.

**The lesson is the method, not the count.** "Which variants are unhandled"
is a question about the source; "which programs emit a TODO" is a question
about behavior. Only the second one is answerable by measurement, and it is
the one that found the bug.

## The fourth bug was not a backend gap at all

While confirming the `unset` fix, a probe that read a key back after deleting
it did this — in **both** backends:

```
── deleted key   rc=1  out=[[]]  err=[./test_bin: echo: write error: error 14]
```

Errno 14 is `EFAULT`. A shell does not get to fault on a map read.

The table is open-addressed with two distinct negative sentinels:
`TABLE_EMPTY` (−1) and `TABLE_DELETED` (−2). `table_find_slot_inner` is
correct — when the key is absent it returns the first DELETED slot it crossed,
which is exactly right for insertion. The callers were not:

| caller | guard | verdict |
|---|---|---|
| `sash_table_delete` | `idx == TABLE_EMPTY \|\| idx == TABLE_DELETED` | correct |
| `sash_table_get` | `idx == TABLE_EMPTY` | reads `entries.add(-2 as usize)` |
| `sash_table_set` | `idx != TABLE_EMPTY` | **writes** through it |

`get` returned a garbage `(ptr, len)` — hence EFAULT on the next write. `set`
took the *update* branch on the sentinel and wrote `val_ptr`/`val_len` through
a wild pointer. Both now test `idx < 0`, since every negative value means
"absent, insert here."

Reverting the runtime and re-running the new specs:

```
── churn one key    rc=0    out=[n=0 k=[39]]                 ← stale value
── churn many keys  rc=139  err=[the monitored command dumped core]
```

A segfault, reachable from ordinary shell code — set a key, unset it, repeat.
This was live in both backends and no spec touched it.

**Why it hid.** A never-set key is fine: its probe chain has no deleted slot,
so the EMPTY guard catches it. The bug needs an *absent* key whose probe chain
*crosses* a deleted one — i.e. any read after any colliding delete. And the
C backend's `unset` no-op meant the C side could never reach it at all. Two
bugs masking each other: spec `80.23` (re-set a deleted key) *passed* in C
while both were broken, because a no-op `unset` left the entry live and the
update branch was then legitimately correct.

That is the argument for fixing in dependency order and re-measuring after
each step, rather than fixing what the symptom points at.

## The structural finding — for DG

Both silent no-ops came from the same shape:

```rust
_ => {
    if !emit_builtin_extra(…) {
        writeln!(e.out, "    /* TODO command: {} ({:?}) */", name, kind).unwrap();
    }
}
```

An unimplemented builtin emits a **comment**. The C compiles, the binary runs,
the statement vanishes. `SsaInst` has the same pattern (`/* TODO inst: */`).
This is a fail-open default in a compiler whose entire premise is failing
closed, and it has now cost three silently-wrong builtins. Whether that arm
should become a hard compile error is a design call, so it is written up as a
task rather than changed here.

## Result

Sweep **4138 passing, 29 skipped, 0 failing** — zero flips, zero lost, +31:
19 from un-gating `exec.sh` in C, and 12 new specs (`80.22`–`80.27`, both
modes) pinning delete/re-set/churn semantics against bash.

Each fix was fail-first verified by reverting the source under test and
confirming the new specs actually fail. That caught a process error worth
recording: restoring `emit_c.rs` from a `cp` snapshot taken *before* the
`TableDelete` work silently reverted that fix, and the specs caught it
immediately. Snapshot per revert, or use `git stash` — not one snapshot for
a session.

`emit_command_body` crossed the complexity gate at cc 13 vs the 12.09 bar
(delta −10, so not dispatcher-exempt). Refactored, not skipped: the three
process/fd primitives moved to `emit_process_builtin`, reusing the
`emit_builtin_extra` "overflow handler returns bool" convention already in the
file. Now cc 11.
