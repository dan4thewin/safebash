---
title: The ranking was mostly ICEs
date: 2026-10-04
---

Back to the coverage queue proper, with `coverage.db` rebuilt at `aab5868`.
Union: **97.55% lines, 98.78% functions, 98.12% regions, 91.27% branches**;
518 uncovered lines.

The by-function ranking put `emit_c.rs::lower_value_as_str_aggregate` at the
head with 6 cold lines, where it has sat for weeks. All six are `panic!`:

```rust
SsaValue::FuncCall { name, .. } => {
    panic!("ICE: FuncCall should have been lifted during transform: {name}")
}
```

Should-never-run assertions, correctly cold, and they must stay cold. The
head of the queue was not work.

So rather than walk the ranking one function at a time, I classified the
whole cohort — contiguous cold runs, marked by whether the run mentions
`panic!`/`ICE:`/`unreachable!`:

| | runs | lines |
|---|---|---|
| ICE/panic | 64 | 64 |
| other | 368 | 454 |

Every ICE run is exactly one line, because line coverage marks only the
panic expression — the `SsaValue::X { .. } => {` arm header above it counts
as covered by the match dispatch. So 64 cold lines are 64 intentional ICE
arms.

The 454 "other" lines are not all work either. The top three runs are my own
`Err(parse_error(…))` arms from this morning's commit, each documented
unreachable, each 5 lines only because rustfmt spreads the call across five.
**Ranking by raw cold-line count over-weights multi-line expressions**, which
is worth knowing before trusting the order.

## The one that was real

`transform/control_flow.rs:58` —

```rust
AttestForm::Map { key, val, map } => Self::Coll {
```

`AttestKind::of` has exactly one caller, `lower_if_attest`, and its three
arms map 1:1 onto `AttestForm`. Counting the suite:

- `if attest v in a` (the array arm) — **7** specs
- `if attest k v in m` (the map arm) — **0**

The statement form `attest k v in m …` is covered thoroughly in taint.sh.
The `if` form of it was specced nowhere.

That is not a cosmetic gap. `div.15`/`div.16` exist for the *array* arm
specifically to pin `bug-caught-attest-all-fail-path-undefined-handle` — a
**SIGSEGV** caused by an attested version defined on one path only, which is
the same hazard the codegen memory describes ("never create an SSA version
defined on only one path"). The map form reaches the same `AttestKind::Coll`
lowering with two binders instead of one, and nothing exercised it.

It works, as it happens. Both merge paths are correct, the retag lands (on
the passing path `${m[K]}` reads the attested value afterwards), and the
`60.30` both-binders rule holds in the `if` position too. Five specs,
`div.33`-`div.37`, both backends.

## Measuring is what found it

Worth recording *how*, because the ranking alone would not have: the useful
query was not "which function has the most cold lines" but "which cold runs
are not ICE arms", and then — for the one that looked like a language
surface — a `grep` counting how many specs use each sibling form. A cold arm
whose siblings have seven specs each is a much stronger signal than a cold
arm in isolation.

Sweep 7517 (3861 native, 3643 c + 13 skipped), zero failures, predicted
exactly.
