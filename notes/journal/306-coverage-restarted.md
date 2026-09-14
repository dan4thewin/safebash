# 306 — coverage, restarted: the measurement, and the artifact that moved it

DG asked to move from function-level coverage ("is each function called by a
spec?") to **full line and branch coverage**, with a disposition rule for
whatever is left: unreachable code is either dead (delete it) or a
should-never-run assertion (keep it, and unit-test that it catches what it
claims).

This session rebuilt the measurement, found the first big mover, and DG
turned it into a design change rather than a test-writing exercise.

## The tooling was gone, and the mkfile target was rotted

`cargo llvm-cov` was not installed — another rsync casualty, like indexnudge.
Reinstalled.

Worse, `mk cov-spec` could not have worked as written. It sets

    SASH="$PWD/target/llvm-cov-target/release/sash"

but `spec/run.sh` reads **`SASH_RELEASE_DIR`**, not `SASH`, and puts it on
`PATH`. The variable it exports is not the one the runner consumes, and the
`target/llvm-cov-target` path is not where this version of cargo-llvm-cov
builds either (`show-env` reports `CARGO_LLVM_COV_TARGET_DIR=target`). The
target has been stale since 2026-08-30.

## Then the same fail-open bit me directly

My first corrected run did `cargo llvm-cov clean --workspace` before
building, exactly as journal 189 prescribes. The build reported

    Finished `release` profile [optimized] target(s) in 5.33s

and the sweep produced **0 profraw files**. The clean does not invalidate
`target/release`, so cargo reused the uninstrumented artifacts and 4548 tests
ran against a binary that records nothing. A coverage run that measures
nothing reports no failure — it reports a *report*.

The fix is `rm -rf target/release` to force the rebuild, and the discipline
is a **canary**: run one `sash` invocation, assert a profraw appeared, abort
if not. Every measurement in this session ran behind that check. This is the
third instance of the same shape (the Rust harness bypassing `validate`, the
`2>/dev/null` in `test-regex.sh`, and now this): a gate that cannot fail is
not a gate.

## Baseline

Spec suite only, compiler only (`sash-rt`, `sash-convert`, `sash-expand`,
tests and `main.rs` excluded). Branch coverage needs `-Zcoverage-options=branch`,
which is nightly-only, so nightly was installed for measurement; the shipped
build stays stable. Both toolchains left the suite green (4548 passing, 0
failing), so instrumentation does not perturb results.

| metric | baseline |
|---|---|
| lines | 93.31% (1740 missed) |
| regions | 91.63% (2434 missed) |
| **branches** | **83.77% (615 missed)** |
| functions | 95.44% (93 missed) |

Note this is *spec-only* — the 77 in-crate unit tests were deliberately left
out, because "what the spec suite reaches" is the number the goal is about.
It is why `panic_payload_message` reads as uncovered here: it has unit tests
(`codegen/entry.rs:458`) and no spec can reach it, which is precisely the
disposition DG described.

## A triage database

`coverage.db` (untracked, like `spec.db`) holds the lcov line and function
data, so uncovered lines can be joined against indexnudge `symbols` and
ranked *by function* rather than read as a flat list of line numbers:

```sql
SELECT s.file, s.name, COUNT(*) AS uncov
FROM cov.cov_lines cl
JOIN symbols s ON s.file=cl.file AND cl.line BETWEEN s.start_line AND s.end_line
WHERE cl.count=0 AND s.kind IN ('function','method')
GROUP BY s.file, s.name, s.start_line ORDER BY uncov DESC;
```

That immediately showed the shape of the problem: one file dominated.

## `emit_ir.rs` — 30.8% covered, and the reason was structural

409 missed lines and **35 never-executed functions**, every one of them an IR
node emitter — `st_if`, `st_case`, `st_for`, `st_pipeline`, `ar_ternary`,
`ex_param`, `redirect`… The cause was not a missing test. `--emit-ir` was a
terminal stdout mode, so the emitter ran only when someone passed the flag,
and four specs over three tiny programs was the whole of it.

The sharp part: `emit_ir.rs` is **deliberately exhaustive, no wildcard arms**
(DD#104), so adding an IR variant is a compile error there. But totality only
buys the `match` arm. An emitter that is only ever *run* on `echo hello` gets
nothing from being total — escaping, nesting and node shape are still only
checked on the paths someone actually emitted.

I was about to write specs that pass `--emit-ir` over a wide program.

## DG: "let's have the compiler emit the binary, manifest, and ir on every run"

Which is the better answer, and not a test at all. It is **DD#109**.

I put three forks to DG rather than picking: destination (ELF sections vs
sidecar files), what happens to the existing flags, and scope. DG chose ELF
sections, keep the flags as stdout modes, native compiles only. DD#45 already
embeds `.sash_source` this way, so the pattern and the `--extract` retrieval
path existed; sections also mean the artifacts cannot desync from the binary
and 4548 spec compiles do not start littering two files each.

Result — one design decision, no new coverage-driven tests:

| | before | after |
|---|---|---|
| `emit_ir.rs` lines | 33.71% (409 missed) | **93.43% (39)** |
| `emit_ir.rs` functions | 44.44% (35 unexecuted) | **96.83% (2)** |
| compiler lines | 93.31% | **94.66%** |
| compiler functions | 95.44% | **97.11%** |
| compiler regions | 91.63% | **93.79%** |

Every program the suite compiles now renders its entire IR. A variant that
fails to render is a failure of every build, not a latent gap.

## A bug in my own change, found by asking what the common case does

First working version embedded all three sections — and `.sash_manifest` was
missing from the test binary. A **zero-length section does not survive into
the linked ELF**, and a program with no external dependencies has an empty
manifest. That is the *common* case: any all-builtin program. `--extract
prog manifest` reported "no .sash_manifest section found" and exited 1 for
it.

Fixed on the read side rather than by padding the section, which would have
broken the property that the embedded artifact is byte-identical to what the
flag prints. An absent `.sash_manifest` reads back as empty, scoped by
`.sash_source` so that a non-sash binary still errors — `--extract /bin/true
manifest` fails, as it should. Specs 109.4 and 109.7 pin both halves.

## State

`spec/cli.sh` 109.1–109.7 (plan 26 → 33). Sweep predicted 4548 + 7×2 = 4562
and observed 4562, 0 failing. `mk test` rc=0.

## Next

The disposition work DG actually asked for has not started. The triage DB
already names the cohort: after DD#109 the remaining fully-uncovered
functions are a small, analysable set — `bracket_compound_with_redirects`,
`lower_negated_stmt`, `emit_redirect_push`/`pop`, the three lexer `skip_*`
helpers, `opaque_form_to_string`, `ice_unreachable_command`. All have live
call sites, so none is trivially dead; each needs the journal-189 treatment
(is the call site shadowed by an earlier check, or merely untested?) before
it can be called dead code or a should-never-run assertion.

Also open, and raised by DG this session: **`sash-rt` is not measured at
all.** It is `#![no_std]`, `panic = "abort"`, has its own `_start` and links
no libc, so `-Cinstrument-coverage` compiles fine (verified: 2801 counter
regions, full `__llvm_covmap`) but nothing writes the `.profraw` out at exit.
Covering it means a profile dumper inside the runtime using raw syscalls,
plus clang for the ~1359 lines of vendored C. Options not yet put to DG.
