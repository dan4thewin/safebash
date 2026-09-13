# 280 — compound stages, and a blank line that ate the input

2026-09-13. Continuing DG's list: groom, then the known bugs worst-first,
then the sweep. The last named bug was `c-compound-pipeline-stage`. It took
three lines. Chasing why the un-gated spec file still failed took the rest of
the session and turned up two bugs nobody had filed, one of them silent data
loss in the construct DD#99 calls "the dominant construct by an order of
magnitude."

## Compound pipeline stages: three lines

Native lowers a compound pipeline stage — a function body, a `{ }` group, a
loop — into its own SSA block and names the stage `__compound_<id>`. In the
forked child it simply jumps there (`codegen/inst_handlers.rs:170`):

```rust
name.strip_prefix("__compound_")
    .and_then(|s| s.parse::<usize>().ok())
    .map(|id| builder.ins().jump(ctx.cl_blocks[&id], &[]))
```

The c emitter had no such case, so the stage name fell through to
`emit_pipeline_exec` and became `sash_exec("__compound_2", …)`. The runtime
rejects a non-absolute path, loudly: `sash: bug: bare command passed to
sash_exec`.

The fix is an exact analogue, because the c backend already emits every SSA
block as a `_blk_<id>:` label inside `main`:

```c
    __t0_pids[0] = sash_fork();
    if (__t0_pids[0] == 0) {  /* child 0 */
        sash_dup2(__t0_pipe0[1], 1);
        sash_close(__t0_pipe0[0]); sash_close(__t0_pipe0[1]);
        goto _blk_2;            /* was: sash_exec("__compound_2", …) */
    }
```

`_blk_2` is the function body; it already ends by falling to the return block
and then `__exit:`, so the child exits with the right status without any extra
machinery. I checked the C-specific hazard — `goto` into the scope of a
variably-modified type is ill-formed, and splat argv emits VLAs — but the jump
leaves the child's `if` block rather than entering a VLA scope, so
`/bin/echo @{xs:~} | g | /bin/cat` compiles and runs. Parity with native across
eight shapes, including nonzero exit propagation from the stage.

## The un-gating found the real bug

`spec/pipe_functions.sh` had been native-only, gated on exactly this gap. I
un-gated it: **10 of 11 passed**. The straggler, `pf.11`, feeds a function's
output to a `while readline` consumer.

The reflex is to assume it's the new code. It wasn't. The same loop fed by
`/bin/printf` — no compound stage anywhere — failed identically. The compound
work was finished and correct; I had walked into something else.

```
fn f { echo a; echo b }
f | while readline x; do attest x [[ -n $x ]]; echo "got $x"; done

native: got a / got b                 rc 0
c:      got a / got b                 rc 1
        ./test_bin: attest: x failed the gate
```

The body ran a third time with an empty `x`. That is the shape of
`c-pipe-while-read-eof-extra-iter`, a P3 I had marked "premise obsolete, needs
a fresh repro" during grooming. Here was the fresh repro.

## It was not an extra iteration. It was never stopping.

The emitted C:

```c
_blk_1: /* while.header */
    sash_pair __t2 = sash_read_line(__pipe_read_fd);
    x_ptr = __t2.ptr;
    x_len = __t2.len;
    __exit_status = (__t2.ptr == (const char*)0) ? 1 : 0;
    if (__test_result) goto _blk_2; else goto _blk_3;
```

The read writes `__exit_status`. The branch reads `__test_result`. Nothing
connects them. `__test_result` held `1` from the pipeline that set up the loop,
and nothing in the body ever reset it, so the condition was a constant.

EOF was never observed at all. The loop terminated only because `attest`
aborted the process on the first empty value — an accident of that particular
body. Give it a gate that admits empty and it runs forever:

```
/bin/printf "a\nb\n" | while readline x { attest x [[ -n $x || -z $x ]]; echo "[$x]" }
c: [a] [b] [] [] [] [] [] …   killed by the 10s hang guard
```

So the filed P3 understated it by a wide margin. A hang is not an extra
iteration. Worth recording *why* the filing was wrong: the person who wrote it
observed the symptom through a body that happened to abort, and described the
symptom rather than the mechanism. The mechanism was one `writeln!` naming the
wrong variable.

## And then native was wrong too

Fixing c meant picking the right EOF predicate, so I read the runtime instead
of copying native. `sash_read_line` is explicit, and the comment is doing real
work (`sash-rt/src/lib.rs:1479`):

```rust
    if total == 0 && !got_newline {
        // True EOF
        return (core::ptr::null_mut(), 0);
    }
    // Empty line (got_newline with total=0) or normal line
```

True EOF is `(NULL, 0)`. An empty line is `(non-NULL, 0)`. The length cannot
distinguish them; only the pointer can.

Native tested the length:

```rust
let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
```

So native treated a blank line as EOF:

```
printf "a\n\nb\n" | while IFS= read -r x; do echo "[$x]"; done
bash:   [a] [] [b]
native: [a]
```

Exit status 0. No diagnostic. The loop just quietly processed the first stanza
and stopped. Any `readline` loop over a paragraph-separated file, a commit
message, or an HTTP header block silently truncated at the first blank line.

DD#99 settles whether that is a bug without needing DG: the DD fixes `readline`
as *byte-for-byte* `IFS= read -r`, and it says so in those words. `IFS= read -r`
succeeds on an empty line. This is decision-tree step 1 — a source of truth
already decides it, so apply it and report, don't ask.

Both backends now use the runtime's own distinction, `ptr != NULL`, and both
match bash.

## Why differential testing missed it

Native and c were *both* wrong, in opposite directions and in the same six
lines. Native stopped one line too early; c never stopped. Comparing the two
backends against each other would have shown a difference, but the suite had
no test containing an interior blank line, so neither backend was ever asked
the question. Bash was the only oracle that could settle it, and only for an
input nobody had written down.

The general lesson: backend-vs-backend parity is a cheap test and a weak one.
It confirms two implementations agree, which is not the same as either being
right, and it is systematically blind to a shared misreading of a runtime
contract. The runtime's comment had the answer in plain English the whole time.

## Two more stale native-only guards

Both of the gates that hid the c hang were, again, *claims about the c backend
written into the spec files*:

- `spec/wordsplit.sh:rl.7` — "the identical `while read v` shape also fails in
  c, so this is a pre-existing backend gap, not a `readline` one."
- `spec/commands.sh:14.46` — "a `while readline` consumer fed by a
  non-function producer does not terminate under the c backend."

Both were accurate when written. Both described a real bug. Neither was a task,
so nothing ever came back to them — the guard *was* the bug report, and it was
filed somewhere nothing reads. `14.46` even names the mechanism ("does not
terminate") precisely enough that the hang was documented in the repository
for as long as it existed.

That is the fifth stale guard in three sessions. The pattern is stable enough
to state plainly: **a skip is a silent bug report.** It suppresses the evidence
that would refute it, so it never expires. When a guard encodes a known defect,
the defect needs a task; the guard should point at the task, not stand in for it.

## Result

Eight bugs across the two sessions. This one's four:

| bug | scope | symptom |
|---|---|---|
| compound pipeline stage unimplemented | c | `sash: bug: bare command passed to sash_exec` |
| readline EOF written to `__exit_status` | c | loop condition constant → **infinite loop** |
| readline EOF tested with `len > 0` | **native** | blank line read as EOF → **silent truncation** |

Specs: `pf.12`–`pf.15` (compound stage status, loop inside a stage, adjacent
compound stages, compound mid-pipeline); `rt99.15`–`rt99.18` (interior blank
line, leading blank line, iteration count to true EOF, and a control pinning
the one-shot path that never consulted the predicate). `pipe_functions.sh`
un-gated (11 tests), `rl.7` and `14.46` un-gated.

Fail-first: reverting both source files fails all 15 c `pf.*`, c `rl.7`,
c `14.46`, and the three discriminating `rt99` tests in *both* backends.
`rt99.18` passed unfixed, which is why it is labelled a control rather than
left to look like coverage it isn't.

Sweep: **4199 passing, 26 skipped, 0 failing.** +29, and skips down from 28.

## Left for DG

`spec/wordsplit.sh` declares `plan=118` and runs 123. The drift is
pre-existing — the committed file already had five unaccounted tests — and it
is unrelated to this work, so I have not touched it. But `plan` exists to catch
silently lost tests, and a stale one has stopped doing that. Same failure mode
as the stale guards, one level up. Worth a sweep of every spec file's plan
count as its own task.
