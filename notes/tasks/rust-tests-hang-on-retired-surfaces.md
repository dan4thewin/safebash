status: done
priority: P1
depends: []
gates:
  - "`mk test` completes"
  - "every retained behaviour has a spec test; the Rust integration suites keep only what a spec cannot express"

> **RESOLVED 2026-09-14 (journal 304).** Both gates now hold, by the more
> radical of the two routes the task contemplated: `sash/tests/` and
> `sash-rt-tests/` were deleted outright rather than migrated piecemeal.
> All 546 integration tests were classified with per-test evidence; only 4
> expressed anything a spec could not, and those became 20 new spec tests.
>
> `mk test` completes and is **green** — 152 C regex checks, 26 sash-grep
> checks, and 77 in-crate units (sash 30, sash-convert 27, sash-expand 20).
> Getting there needed three repairs beyond the deletions, each of which had
> been hidden behind a louder failure upstream:
>
> 1. a doctest in `validate::check_taint` — a 4-space-indented block of
>    *sash* source that rustdoc compiled as Rust;
> 2. `contrib/sash-grep.sash`, stale against DD#99 (`while read` → `while
>    readline`) and DD#42 (`$(argv 1)` → `$(arg 1)`), which failed to compile;
> 3. `contrib/test-regex.sh`, which built that program with `2>/dev/null` and
>    so turned the compile failure into 19 silent empty-match FAILs instead
>    of one loud fatal error. Hardened to fail fast.
>
> The root cause the task named — the harness bypassing `validate` — was
> worse than "hangs": it was **fail-open**. 49 of 159 passing bash-differential
> tests (31%) compiled programs the real compiler rejects.


# Rust integration tests: migrate to specs, discard the rest

`cargo test --test codegen_tests` never terminates, so `mk test` cannot
complete and step 1 of the completion checklist is unavailable.

## DG's ruling (2026-09-12)

> "It's not clear these tests have any value at all. We should discard any
> that language changes have left behind or that have spec tests exercising
> the same code. For any that do not have spec tests, spec tests should be
> added and the rust test discarded. Only if that's not possible, the test
> should be rehabilitated. This work should come after other code changes to
> the compiler, as otherwise it's a moving target."

So this is **not** a bug fix — it is a migration, with a fixed order:

1. **Discard** any test a language change left behind.
2. **Discard** any test whose code a spec test already exercises.
3. **Add a spec test, then discard the Rust test**, where coverage is real but
   spec-side absent.
4. **Rehabilitate** only where a spec test cannot express the case.

**Sequenced after the other compiler changes** — running it first makes it a
moving target. In the meantime `mk test` stays blocked and the spec suite
(which is authoritative anyway) carries verification.

## Measured state (2026-09-12, not estimated)

16 tests in `codegen_tests` touch DD#99-retired surfaces. Run individually
under a 15 s timeout:

| outcome | n | tests |
|---|---:|---|
| **hang** | 3 | `read_delim_colon`, `test_read_multi_vars`, `test_read_three_vars` |
| fail | 8 | `while_read_*`, `pipe_while_read_*`, `read_into_array_*`, `cmdsub_with_args` |
| pass | 5 | `echo_n_no_newline`, `echo_n_no_args`, `echo_n_multiple_args`, `noop_colon_in_if`, `mapfile_basic` |

The filing previously named one hang; there are three, all multi-target or
`-d` reads.

**The retired flags silently became literal data.** `read -a arr` now yields a
one-element array holding the string `-a`; `echo -n foo` prints `-n foo`. The
hang is the same cause: `read -d: -r word` consumes nothing, so the enclosing
`while` never terminates. Nothing crashes — the tests assert bash behaviour the
language no longer has.

Suite-wide retired-surface counts: `codegen_tests` 10 read-flag / 7 `echo -n` /
3 mapfile; `brush_compat_tests` 2 read-flag; `taint_spec_tests` 4 mapfile.

## Two things found while measuring

**The harness has no timeout.** `run_bash_native` ends in `cmd.output()` and no
test file mentions `Duration` or `timeout`. Any codegen bug that produces an
infinite loop hangs the suite forever rather than failing it. Whatever happens
to these fixtures, that property is worth not keeping.

**The 5 passing tests are live retired-surface codegen.** `echo -n` and
`mapfile -t` still work end-to-end in codegen; only the validator refuses them
(SB5059 / SB5060). Filed separately as
`remove-retired-surface-implementations` — DG ruled remove.

That also answers the old "should `run_bash_native` run `validate` first?"
fork: moot. The question only mattered for tests that reach past the
validator, and those surfaces are being removed.

## Related

The wider cohort is ~140 failures across the five completing suites
(`e2e` 54, `brush_compat` 28, `taint_spec` 26, `oracle` 17, `janet` 15) —
a stable baseline, mostly diagnostic-code drift (`expected SB4020, got
SB5020`) and fixtures predating rules like SB5018. All of it is in scope for
the same migration. See `audit-rust-test-suites`.
