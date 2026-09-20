# 375 — the guards that only a unit test can reach

*2026-09-20*

DG settled the backstop question that had been blocking the 100% target:

> 100% would be the union of reachability from spec tests and unit tests.
> Unit tests should be able to bypass other layers. Any code that should be
> unreachable should halt with an ICE if it ever ran, no?

Three consequences, one of which cost me a measurement.

## The pattern already existed

I had been treating this as an open design question. It was not: the codebase
already carried `panic!("ICE: …")` guards paired with `*_is_loud_ice` unit
tests that call the guard directly through `catch_unwind` —
`redirect_reject_noun_is_loud_ice`, `joined_form_to_string`,
`opaque_form_to_string`, and four more. DG's rule is the convention those
tests were already written under. I had read the tests in an earlier session
and still filed the question. A stated convention with instances in the tree
is a source of truth; I should have applied it.

## My cold list was measured against the wrong thing

`mk cov` and `mk cov-branch` are spec-only **by design** — the comment in
`contrib/coverage.sh` says unit tests are excluded because they "hide exactly
the should-never-run assertions worth finding". Under DG's definition that
exclusion inverts: those assertions are *supposed* to be unit-covered, and
only a unit test can reach them. So `joined_form_to_string` and
`opaque_form_to_string` had been sitting on my cold-function queue while
already carrying ICE tests. Added `mk cov-union` (`--branch --with-unit`);
no existing target ran that combination.

And one entry on the list was simply wrong. I reported `expr.rs:230`
(`find(':')`) as unreachable. `@{a}` reaches it and raises SB5033 — a spec
gap, not a backstop. I had inferred it from a marking run that only exercised
the `=~` forms. **Reporting a line unreachable without a probe for that line
is the same error as reading a call site instead of running it.**

## Four guards converted

- `codegen/values.rs::intern_or_declare` — the declare-on-the-fly fallback is
  gone. It produced *correct* (un-deduplicated) data, which is precisely what
  made it useless: it papered over a drift between the literal pre-pass and
  codegen instead of reporting it. Dropping the fallback also dropped the
  `&mut ObjectModule` parameter from four call sites, which is what makes the
  unit test a two-liner against an empty map.
- `parser.rs::strip_sed_prefix` — `scan_one_sed_edit` compares the two bytes
  `s/` literally before scanning anything, so `take_sed_stmt_payload` cannot
  return a payload without them.
- `parser.rs::validate_static_sed_re` — same scanner, same guarantee, for the
  two remaining `/` delimiters. Its `None` arm used to `return Ok(())` and
  defer to a later error that could never fire either.
- `expr/parse_at_brace_disposition` — `parse_word_arm` only routes words that
  `starts_with("@{")`, and the lexer's `@{…}` scan consumes through the
  matching `}` or fatals with SB4057. Both halves of the strip are guaranteed.
  The `find(':')` below them stays a real SB5033, because `@{a}` reaches it.

Five `*_is_loud_ice` unit tests, one per guard. The **sweep is the proof they
are unreachable**: 5783 specs, zero ICE panics, zero flips. Converting first
and sweeping second is the execution-based answer — if any of them had been
reachable, a spec would have said so immediately and loudly, which is the
behaviour we want anyway.

## The measurement that would not run, and the bug under it

`mk cov-union` failed. Not slowly — instantly, printing nothing:

```
cov-union: bash contrib/coverage.sh --branch --with-unit …
EXIT=1
```

The unit-test pass is

```sh
(cd sash && cargo $toolchain llvm-cov --no-report --release --lib \
    --ignore-run-fail) >/dev/null 2>&1
```

and it was being **SIGKILLed by the OOM killer** — an instrumented release
build of the lib plus its test harness, on a shared host with 113 of 125 GB
taken by another tenant. `-j4` and `-j1` made no difference; the peak is one
rustc process. That part is environmental and will pass.

What is not environmental: with output on `/dev/null` and `set -e` above it,
the failure was **completely silent**. No message, no partial report — the
script just stopped, which from the outside is indistinguishable from a run
that found nothing to do. This is the same shape as the canary the script
already documents ("a gate that cannot fail is not a gate") and the same shape
as the two bugs in journal 374: a failure path that produces no evidence. The
step now captures its output and prints the tail before exiting.

So the union baseline is **not yet measured**. The spec-only numbers stand
(regions 96.42%, functions 98.45%, lines 97.19%, branches 90.70%) and are
known to *understate* the union by at least the seven ICE guards. Re-measure
when the host has memory.

## Gates

Sweep **5783, 0 failing, 0 flips vs `4a93503`**, zero ICE panics. Units **50**
(up 5). `cargo fmt`, clippy clean.
