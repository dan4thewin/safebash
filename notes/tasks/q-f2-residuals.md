title: "F2 residuals: wait, output and historical END behavior"
status: open
priority: P3
kind: question
depends: []
gates:
  - "R2's accepted DD55 single-write rule is implemented by bug-echo-partial-line-before-abort; this question record does not duplicate that work"
  - "remaining questions are reconciled with current DDs before selecting new behavior; retained live reproducers have specs in both backends"

Found during F2 (journal 539). Each is pre-existing unless it says
otherwise. Probes in files/port-f2-probes.tar.gz.

The tables retain historical measurements, not new runs at the decision
date. R2 is settled below; this file stays open for the other residuals.
The END examples predate DD128's implemented replacement and are not
evidence about current EXIT/ERR behavior.

## R1. `wait` with a pid <= 0 reaps an arbitrary job

`sash_wait_any` (sash-rt/src/lib.rs) calls `waitpid` for a pid > 0 and
`wait(2)`, any child, otherwise. A no-argument `wait` passes -1, so
`wait 0`, `wait -1`, and `wait $!` after a failed fork (`$!` reads -1 since
B20) all reap whichever earlier job finishes first. `wait -7` returns -1.

| Id | Program | Result |
|---|---|---|
| wq2 | `/bin/sleep 0.2 &` ⏎ `int p0=$!` ⏎ `int r=$(wait 0)` ⏎ `int s=$(waitstatus)` ⏎ `int q=$(wait -7)` ⏎ `int t=$(waitstatus)` ⏎ `echo "r0=$(( r == p0 )) s=$s q=$q t=$t"` | `r0=1 s=0 q=-1 t=-1`, every version |

Question: should a pid <= 0 be a runtime error (or, for a literal, a
compile error), rather than "any child"? The `emit_wait_value` comment
"waitpid(0) is process group" is also imprecise.

## R2. Settled: one write request per builtin-output invocation

DG, 2026-11-09: **"yes to a single write"**. DD55 records the rule for
`echo` and `write`: submit the complete fd-backed output as one request,
including separators and `echo`'s final newline. Do not split words,
interpolation components or the newline into separate writes. The rule
also applies outside background children.

The guarantee is bounded to pipe/FIFO requests of at most `PIPE_BUF`,
counting all emitted bytes. Larger output is allowed but can interleave;
other destinations have no universal guarantee. No launch-order promise,
global lock or rollback of a failed/partial write is introduced.
Buffer captures stay in memory, and DD55/DD130 failure handling remains.

The existing P2 `bug-echo-partial-line-before-abort` owns implementation
and proof, including stable operand staging, exact syscall shape and
short-write/error accounting. It already owns the same fragment-emission
path. Journal 605 records the ruling; no compiler/runtime change or new
concurrency measurement accompanies it.

### Historical evidence

| Id | Program | Result |
|---|---|---|
| b17a | `/bin/printf "1\n2\n3\n" \| while readline x { attest x [[ -n $x ]]` ⏎ `echo "got $x" &` ⏎ `- wait }` ⏎ `echo done` | lines interleave mid-line (`donegot \|3\|`; one run printed `got 3done`) |

The native/C source still emits fragments separately. The historical
question is answered; this table is not evidence that the repair shipped.

## R3. A bare `${s:?}` aborts with an empty message

| Id | Program | Result |
|---|---|---|
| sq | `str s=""` ⏎ `echo "${s:?}"` ⏎ `echo after` | aborts; runtime stderr is `./test_bin: ` and nothing more |

`: ${s:?}` does the same. DD77 does not say what the default message is.
Recommendation: name the variable, as bash does.

## R4. Children forked inside END are unchecked (B9's open question)

| Id | Program | Result |
|---|---|---|
| q1 | `END { /bin/false &` ⏎ `- wait` ⏎ `echo end }` ⏎ `echo main` | `main`, `end`, rc 0, no message |
| q2 | `fn g { /bin/false; echo "g after" }` with `g &` inside END | prints `g after` |

The child inherits END's unchecked status, as the DD56 addendum of
2026-10-28 records (END runs only in the main process; END is unchecked).
Question: should a child forked in END be checked like any other child?
