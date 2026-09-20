# 376 — the OOM that was a fork bomb

*2026-09-20*

`mk cov-union` would not run. Journal 375 recorded it as environmental: a
shared host, another tenant holding 113 of 125 GB, the instrumented `--lib`
build SIGKILLed by the OOM killer. That diagnosis was wrong, and it was wrong
in a way worth writing down, because every piece of evidence for it was real.

## The evidence for the wrong answer

- The step died with **signal 9**, which is what the OOM killer sends.
- `free` really did show 7–11 GB available at the time.
- The dying step really was the heaviest in the script.
- Bounding `CARGO_BUILD_JOBS` to 4, then to 1, really did not help — exactly
  what you would expect if the peak were a single unsplittable rustc.

Four consistent facts, one plausible story, and I stopped. I even wrote the
mitigation into the notes: *don't lower `opt-level`, it breaks comparability;
retry when memory frees.*

## The fact that did not fit

Retrying with **115 GB free** got the same instant SIGKILL. An OOM kill that
lands as readily with 115 GB free as with 400 MB is not an OOM kill. I had
been treating "free memory" as the independent variable when it was a
*dependent* one — and the thing that made it drop was my own build.

`ps` aggregated by command:

```
cargo-llvm-cov       871.9 GB
```

**259,142 `cargo-llvm-cov` processes.** Load average peaked at **13,167**. The
"other tenant" eating the machine on a ten-minute cycle was me, once per
attempt. Each `stop_bash` left the tree running, so the attempts accumulated.

## The cause, which had been printing itself all along

```sh
eval "$(cargo $toolchain llvm-cov show-env --sh $branch)"
...
(cd sash && cargo $toolchain llvm-cov --no-report --release --lib …)
```

`show-env` exports `RUSTC_WRAPPER=/…/cargo-llvm-cov`. Running the `llvm-cov`
*subcommand* inside that environment makes it re-enter itself once per rustc
invocation. cargo-llvm-cov says so, in the log, on every single run:

> cargo-llvm-cov subcommands other than report and clean may not work
> correctly in context where environment variables are set by show-env;
> consider using normal cargo commands

I had that line on screen at least four times. I read it as boilerplate noise
because it was filed under `warning:` and the `error:` two lines below said
`SIGKILL`, which matched the story I already had. "May not work correctly" is
a considerable understatement for *forks without bound*, but the tool was not
wrong and it was not quiet. **I was reading the log for confirmation rather
than for information.**

The fix is the one the warning names: inside a `show-env` environment, use a
plain cargo command. `LLVM_PROFILE_FILE` is already exported into `target/`,
so `cargo test --release --lib` drops exactly the profraws the report step
wants. `--ignore-run-fail` is gone with it, deliberately — a failing unit test
should fail the coverage run, not be measured and discarded.

Only the `--with-unit` path was ever affected. The spec path runs `cargo
build`, and `report` is explicitly exempted by the warning, which is why
`mk cov` and `mk cov-branch` have always worked and why this hid until DG's
union definition made me combine the two flags for the first time.

## What made it findable

The silent-failure fix from journal 375. Until then the step ran under
`>/dev/null 2>&1` with `set -e` above it, so the whole run aborted having
printed nothing. Fixing the reporting did not fix the bug, but it is the only
reason the bug ever became visible — the warning I eventually acted on was
text that fix had newly put on screen.

That is the second time in three journals that a failure path producing no
evidence was the thing standing between us and a bug.

## The union baseline, at last

`profraws from sweep: 5791`, specs **5770 passing + 13 skipped = 5783**.

| metric    | spec-only | union  | cold under union |
|-----------|-----------|--------|-----------------:|
| regions   | 96.42%    | 96.66% | 999 |
| functions | 98.45%    | 98.80% | **26** (was 33) |
| lines     | 97.19%    | 97.40% | **698** (was 746) |
| branches  | 90.70%    | 90.85% | **349** (was 354) |

Modest, and that is itself the finding: the union is the *correct* denominator
for DG's target, but it does not move the number much. The remaining 698 cold
lines are real work, not a measurement artefact.

All six ICE-guarded functions now read fully covered — the two that had been
sitting on my cold queue with tests already written, and the four converted in
375. The mechanism works end to end.

## Also probed: `capture_purity.rs`, no bug

Cold lines there led to the buffer-vs-memfd decision, so I went after it hard:
a redirect and then a pipeline buried inside every construct that can nest one
(`if`, `for`, group, `&&`-list, function body), checking both the captured
value and the file. All correct. `child_bodies()` turns out to be an exhaustive
match over `StmtKind` with no `_` arm, so no kind can hide a disqualifying
statement from the walk — the property I was trying to break is structurally
enforced.

Two cold regions remain, with different characters:

- `command_is_pure`'s non-literal-head arm is **unreachable**: SB5011 rejects
  a variable, quoted, concatenated or single-quoted head, and I probed all
  four in five positions (top level, capture, function body, pipeline, `if`,
  `for`). An ICE candidate.
- The `bodies.get(name)` walk at 95–101 is *measured* never-taken but **not
  proven** unreachable — it depends on whether every user-function call is
  inlined before this pass, which I could not settle without instrumenting a
  rebuild. Left alone. "Never observed" is not "cannot happen", and an ICE on
  that distinction would crash a legal program.

## Gates

Sweep 5783, 0 failing. Binary restored uninstrumented (8457168 bytes).
