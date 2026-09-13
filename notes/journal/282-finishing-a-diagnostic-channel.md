# 282 — Finishing a diagnostic channel, and a list that was load-bearing

Two fixes: the last c-mode skip, and the P1 PATH-twin bug.

## 55.30 — and the two paths I found behind it

The filed gap was narrow: c had no `sash_emsg` on the external-abort path, so
the parent's `command failed (exit 127)` line never printed. The runtime
already exported `sash_emsg_status`; c simply never declared or called it.
Three lines in `emit_exec` and the prologue.

Then I measured the rest of errors.sh. Eight tests used the *inline* form

```sh
&& { ! in_modes native || [[ $rerr == *"…"* ]]; }
```

which is not a skip — the test runs in c, it just doesn't assert the message
there. That is the audit pattern wearing different clothes, and it hides the
same thing. Five of the eight passed in c the moment `emit_exec` was fixed.
Three did not, and they were three *different* abort paths:

| spec | path | site |
|---|---|---|
| 55.8 | command substitution | `emit_cmdsub_exec_c` — `if (st != 0) sash_exit(st);` |
| 55.10, 55.35 | pipeline | `emit_pipeline` — bare `goto` on nonzero |

I fixed both rather than stopping at 55.30. A diagnostic channel that reports
for `/bin/false` but stays silent for `/bin/false | /bin/true` is a worse
surface than one that is uniformly missing — the user learns to trust a
message that is not always there.

The pipeline case had the only real design content. The message names the
**failing stage**, not the last one, so the name has to ride along with the
status through the first-nonzero fold:

```c
i64 p_status = 0;
const char* p_fail = 0; i64 p_fail_len = 0;
{ i64 p_st = sash_wait(p_pids[1]); i64 p_s2 = p_st;
  if (p_status == 0 && p_s2 != 0) { p_status = p_s2;
      p_fail = "/bin/false"; p_fail_len = 10; } }
```

which is c's version of what native does by materializing every stage name up
front and letting `wait_for_children` pick (inst_handlers.rs:68). `55.35`
(`- /bin/true | /bin/false`) is the test that proves it: it must report
`/bin/false`, the *second* stage, not the suppressed first one. It does.

All eight inline guards are now unconditional and 55.30 is un-gated.
**errors.sh has zero mode guards and passes 181/181 in both backends.**

`sash_emsg_status` skips a null name segment, so the pipeline path degrades to
the bare `command failed (exit N)` rather than crashing if a stage name is
ever absent.

## The PATH twin

`VALUE_ONLY_INTRINSICS` had ten entries and a doc comment that explained the
invariant and then got it wrong:

> The other value intrinsics (`len`, `keys`, `join`, `toupper`, `random`, …)
> are not in `BUILTINS` at all, so they already get SB5011 in statement
> position.

SB5011 is "command not found". It only fires when PATH lookup **misses**. So
the ten names outside `BUILTINS` were not protected by a rule at all — they
were protected by the accident that this machine has no `/usr/bin/len`. It
does have `/usr/bin/join`, `/usr/bin/basename` and `/usr/bin/dirname`:

```
$ sash --emit-c bn.sash        # bn.sash: basename /a/b
    const char* __t0_argv[] = { "/usr/bin/basename", "/a/b", (const char*)0 };
    __exit_status = sash_run((const char* const*)__t0_argv, sash_envp);
```

A statement-position `basename` forked coreutils. Not a c-backend thing —
`resolve::resolve_commands` runs at lib.rs:168, ahead of codegen, so native
did it too.

Per DG's scope-B call, all 11 value intrinsics (`len`, `strlen`, `keys`,
`join`, `sub`, `gsub`, `toupper`, `tolower`, `basename`, `dirname`, `random`)
are now in **both** lists: in `BUILTINS` so they are never PATH-resolved, and
in `VALUE_ONLY_INTRINSICS` so statement position is SB5073.

`random` is worth noting — it has no `BuiltinCmd` variant and never appears in
`from_name`. It is resolved by name in `transform/expr_handlers.rs`, which is
why it was invisible to a search of the builtin table. The index's
`string_literals` view found it as a `match_pat`.

`via.13-18` pin the invariant with a deliberate split: three names that have a
real twin on this host and three that do not. The second group is the one that
matters — it fails for the *right reason* (being value-only) rather than
because a lookup happened to miss, which is what makes the property
host-independent rather than accidentally true.

## Sweep

c **2057 → 2066**, native **2158 → 2166**, skips **10 → 9**, 0 failing.
17 flips: 55.30 un-gated plus 16 new tests. The gap reconciles: 2166 − 2066 =
100 = 91 native-only ids + 9 c-skips.

Fail-first was unusually informative here. Reverting both source files failed
**6** in native (via.13-18 exactly) and **14** in c — the same 6 plus 8 message
tests. Not 9: `55.16` asserts only the *child's* errno line, which the forked
child already wrote in both backends. The arithmetic matching the prediction
exactly is what says the tests are measuring the fix and not something nearby.

## Tasks closed

`value-intrinsics-exec-path-twin`, `c-dd55-abort-channel`, and
`dd55-abort-channel-remaining`. The last one's surviving gates were either
met here (pipefail abort in c, descriptive messages in c) or moot under
DD#104 (the janet gates).

## The remaining 9 skips

None is a c defect. Four (`104.x`) and three (`57.46x`) invoke the `sash` CLI
directly, so `SASH_MODE` never reaches them; two (`32.x`) assert an ELF
section that only the Cranelift-linked binary has. Each guard now records what
was measured and when.
