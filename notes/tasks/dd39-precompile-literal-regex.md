title: Precompile literal regex patterns at runtime init
status: open
priority: P3
kind: work
depends:
  - dd39-compiletime-regex-validation
gates:
  - "Verify that the implementation actually reuses compiled literal patterns; ordinary match correctness alone does not establish reuse"
  - tests
  - commit

## Status correction (2026-11-10)

The housekeeping commit `c958d3d9` marked this task done. Current source
inspection at `7a378603` does not support that completion claim: native
and C emission call the pattern-string runtime interfaces, and the five
match/substitution entrypoints in `sash-rt/src/lib.rs` call `regcomp9`
on use (lines 2516, 2552, 2598, 2670 and 2809).

Reopened to reconcile DD39's original requirement with the implementation.
Recheck the original 39.9 gate when undertaking this task. No new runtime
measurement or cache design is claimed. DG's accepted
SB5023 diagnostic improvement is separately owned by
`q-sb5023-names-the-test-not-the-pattern`; its possible diagnostic catalogue
does not select this runtime mechanism or depend on delivering it.

## Measured repeated-use cost (2026-11-17)

The DD131/DD132 evaluation compares original main `6c7b899f` with trial
`76376c4e`, using matched compiler/runtime pairs and both native/C backends.
A thousand Boolean matches of `abc` against `^a` fail with 34,471,936 bytes
of configured heap and pass with 34,476,032 bytes, identically on both
versions and backends. Four thousand matches exhaust a 64 MiB heap.
Fresh match-group arrays similarly need about 35 MiB at 1,000 iterations
and exhaust 64 MiB before 4,000. These are configured-heap sufficiency
brackets, not measurements of peak or live allocation.

Quiet `r?` validation frees its candidate/program but does not supply a
cached program to consumers. Validating an immutable candidate once, or
reconstructing and validating it on each iteration, has the same measured
heap bracket as literal matching in the trial. `rglob` owns and releases
its per-pattern program separately; that is not literal reuse across calls.

[Journal 654](../journal/654-glob-scratch-regex-evaluation.md) records the
sources, measurement bounds and evidence. The inspected ordinary Boolean
match path recompiles and retains its program and terminated input copies.
Literal reuse and temporary/result ownership are distinct: a cache alone
does not establish that dynamic patterns or all operation-only storage
are reclaimed. This task remains open, with no new cache representation
or general ownership strategy selected by the evaluation.

## Original implementation sketch

Retained as the earlier proposal, not a new implementation selection.
Revisit the approach when undertaking the task.

For literal regex patterns, emit one `regcomp9` call per unique
pattern at binary startup (or lazily on first use). Store the
`Reprog*` in a global. All `regexec9` / `sash_regex_replace` call
sites for that pattern reuse the cached compiled program.

### Codegen changes

- Collect all literal regex patterns during SSA walk
- Deduplicate
- Emit global slots for each `Reprog*`
- Emit init sequence: `regcomp9` each pattern, store in global
- At use sites: load from global instead of calling `regcomp9`

### Runtime changes

- `sash_regex_match` and `sash_regex_replace` need variants that
  accept a precompiled `Reprog*` instead of a pattern string
  (or add `sash_regex_match_compiled` / `sash_regex_replace_compiled`)

Ref: DD#39
