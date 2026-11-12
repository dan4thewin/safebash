title: "Bug: a count-based strace fault (`when=N`, N ≥ 2) can land on an earlier fork, because strace counts a restarted syscall entry; pipeown.28 is exposed"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "pipeown.28 cannot pass or fail on an off-target fault: under the natural-restart recipe below, its log shows no restarted clone before the injected entry, in both modes"
  - "`run_sash_under` fails a strace-prefixed spec whose log shows a restarted entry (`= ? ERESTART…`) of the faulted syscall before its `(INJECTED)` line, with a message saying the fault landed off target. It is proven on such a log, and every current injection spec passes it"
  - "the census below is re-run, and every count-based rule with N ≥ 2 on a restartable syscall is on target by construction or covered by the detector"

## Found

2026-10-27, during F1+F4's reversion stubs (journal 537). Under stub S9b,
pipeown.28 (`spec/pipe_functions.sh:846-854`, written in coverage 527) failed
once in C mode. It exited with rc 1 and empty output, and wrote this to stderr:

```
/bin/cat: finished: No such file or directory
<bin>: /bin/cat: command failed (exit 1)
```

S9b does not touch that path, and the failure did not recur. Rates:

- across both stub runs, 1 failure in 266 runs of pipe_functions.sh, all in C
  mode (1/133 C, 0/133 native);
- 0 failures in 25 sweeps per mode;
- 0/150 per mode in an unloaded loop.

This is not a sash bug. The fault lands on the wrong fork.

## Mechanism

The spec injects `strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=3`.
There is no `-f`, so the count covers the main process only. The forks it
intends are printf (1), sh (2) and cat (3, EAGAIN).

- The runtime installs no SIGCHLD handler, so the disposition is the default,
  ignore. Under ptrace an ignored signal is queued anyway, because the
  kernel's `sig_ignored()` returns false for a traced task. So printf's
  SIGCHLD is queued.
- Suppose printf exits while the parent sits in clone entry 2's
  syscall-entry stop. The signal is then pending at `copy_process`'s check,
  so the clone returns ERESTARTNOINTR. The signal is delivered and ignored,
  and the clone is re-entered.
- strace counts the re-entry as entry 3 and injects EAGAIN into sh's fork. sh
  never runs, so `finished` is never written, and the else branch's
  `/bin/cat finished` fails. That is the observed failure exactly.
- Untraced, an ignored SIGCHLD is discarded, and a restart is invisible
  anyway.
- An injected entry never reaches `copy_process`, because strace swaps the
  syscall number. So the target entry itself cannot restart. Only the entries
  before it can.

Restartable here: clone (ERESTARTNOINTR), and a blocking read or wait4
(ERESTARTSYS). pipe2 and dup2 never restart.

strace 5.5 limits:
- a later `-e inject=` rule for the same syscall overwrites the earlier one,
  so "delay entry 2, fail entry 3" cannot be written;
- counters are kept per traced process.

## Proofs

Versions: strace 5.5, kernel 5.15.0-1089-azure. `p.sash` is pipeown.28's
program. Probes are in `files/port-f1f4-probes.tar.gz`, directory `p28/`.

1. **The symptom is a fault on sh's fork.** Run with
   `-e inject=clone:error=EAGAIN:when=2`. The result is rc 1, empty output,
   and the two stderr lines above.
2. **A pending SIGCHLD restarts the clone, and the restart is counted.**
   `-e inject=clone:signal=SIGCHLD:when=2` logs
   `clone(…) = ? ERESTARTNOINTR (To be restarted)`, then
   `--- SIGCHLD {si_code=SI_KERNEL} ---`, then a second `clone(` entry for the
   same fork. The output is correct.

   With `when=2+2`, every fork after the first restarts exactly once (sh, cat,
   the else branch's cat, in each iteration). That pattern requires the
   restarted entry to be counted. If it were not, only every other fork would
   restart.
3. **A real child exit does it, in both backends.** `r.sash` is `p.sash` with
   the producer `/bin/sh -c "echo one; sleep 0.1"`. Run it under
   `-e inject=clone:delay_enter=500000:when=2`, so the producer exits during
   entry 2's stop. Each log shows ERESTARTNOINTR, then
   `--- SIGCHLD {si_code=CLD_EXITED, si_pid=<producer>} ---`, then the
   restarted clone. 3/3 native and 3/3 C. The C binary was built with
   `sash --emit-c` and `gcc -nostartfiles -static -fno-stack-protector …
   libsash_rt.a -lpthread`.

## Exposure

There are 39 count-based injection rules, on 38 spec lines: commands.sh 14
lines, optional_args.sh 2, pipe_functions.sh 12 and signals.sh 10. Line 533
of pipe_functions.sh carries two rules.

| Rules | Verdict |
|---|---|
| 30 with `when=1` | on target: there is no earlier entry |
| pipe2 `when=2`: p518.17 (pf:519), pipeown.27 (pf:843), capext.17 (cmd:1458), capext.23 (cmd:1495) | on target: pipe2 never restarts |
| clone `when=2`: p518.18 (pf:523), p518.20 (pf:533), p518.24 (pf:551), capext.18 (cmd:1465) | on target: entry 1 is the first fork, so no child exists to exit during it |
| clone `when=3`: pipeown.28 (pf:853) | **exposed** |

## Options

- **(a) A detector in `run_sash_under`** (`spec/common.sh:116-120`).
  - When the prefix is strace with `-o LOG` and `inject=SC:…:when=N`, scan
    LOG after the run.
  - If an `SC(…) = ? ERESTART…` line comes before the `(INJECTED)` line, the
    fault landed off target, and the spec fails with a message saying so.
  - No retry: it detects, it does not mask.
  - It covers every future spec, but it turns a flake into a distinct red. It
    does not remove the flake.
- **(b) Restructure pipeown.28** so the producer cannot exit before sh's fork
  completes. The producer prints its lines, then polls (bounded) for
  `finished`, which only sh's child writes. The ordering is then guaranteed
  by construction rather than by timing.
- **(c) Block SIGCHLD in the traced program.** Rejected: the mask is inherited
  across fork and exec, so every stage would run with it.

## Recommendation

(b) plus (a), with no retry. (b) removes the one exposure, and (a) keeps the
next N ≥ 2 rule honest.

It is in the backlog because the spec predates F1+F4 (coverage 527), F1+F4
did not touch it, and it blocks nothing. That is DG's 2026-10-25 rule.
