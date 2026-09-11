---
status: open
priority: P1
---

# Rust integration tests hang on retired DD#99 surfaces

`cargo test --test codegen_tests` never terminates. `mk test` therefore
cannot complete, which disables step 1 of the completion checklist
entirely.

## Cause

`codegen_tests::run_bash_native` calls `sash::codegen::compile_to_object`
directly, bypassing `validate`. Fixtures written before DD#99 still use
surfaces that DD#99 retired:

    echo -n "one:two:three" | while read -d: -r word; do ... done   # read_delim_colon

The real compiler rejects this (`SB5059`: ``echo -n`` is retired). The test
does not go through that gate, so the retired form lowers to something
whose loop never terminates, and the compiled binary spins forever. Killed
after 47 minutes; reproduced at `fff2a1b` with the working tree stashed, so
it is not a DD#100 regression.

`read_delim_colon` is the confirmed hang. The same cohort is larger:

| surface  | codegen_tests | brush_compat | taint_spec |
|----------|--------------:|-------------:|-----------:|
| `read -` |            10 |            2 |          1 |
| `echo -n`|             7 |            0 |          0 |
| `mapfile -` |          1 |            0 |          1 |

## Why it is filed rather than fixed

Two forks, both DG's call:

1. **What replaces each fixture.** `read -d:` has no DD#99 equivalent —
   `read` splits on NUL (`wordsplit.sh` nr.9). So the choice is rewrite to
   a different behaviour, or delete the test. Deleting a codegen test is
   not a mechanical edit.
2. **Whether the harness should validate.** Making `run_bash_native` run
   `validate` first would convert every one of these hangs into a clean
   failure, and would stop future retired surfaces from silently producing
   garbage binaries. But it changes what "codegen test" means — some of
   these may exist precisely to exercise codegen paths the validator
   refuses to reach.

Option 2 is the one that prevents recurrence; option 1 is needed either
way for the fixtures that are simply stale.

## Related

The wider cohort: ~141 Rust integration test failures at `fff2a1b` across
`brush_compat` (4), `e2e` (54), `oracle` (17), `janet` (16), `taint_spec`
(26), mostly diagnostic-code drift (`expected SB4020, got SB5020`) and
fixtures predating rules like SB5018. The spec suite — which is the
authoritative one — is green apart from known janet gaps.
