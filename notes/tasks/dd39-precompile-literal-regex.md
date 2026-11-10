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
