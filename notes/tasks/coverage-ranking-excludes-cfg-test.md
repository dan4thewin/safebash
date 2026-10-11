---
status: done
priority: P3
kind: chore
---

# coverage ranking counts `#[cfg(test)]` code as production cold lines

`ice_text` (`sash/src/transform/expr_handlers.rs`) sits in the by-function
cold-line ranking with 4 uncovered lines. It is a **test-only helper** — the
whole module is `#[cfg(test)]` — and its cold half is the `String` downcast
branch that no current ICE payload takes.

It is not work, and it displaces real candidates from the head of the queue.

## Done 2026-10-11 — option (1), filter at load

`contrib/covdb.py` learns the line ranges of `#[cfg(test)] mod` blocks per
file and drops the DA / BRDA / FN records defined inside them. Chosen over
filtering at query because it cannot be forgotten: every consumer of
`coverage.db` gets the honest ranking, including ad-hoc SQL.

The range scan looks for `#[cfg(test)]` followed by `mod NAME {`, closing at
the next **column-0 `}`**. Brace counting would be wrong — the ICE tests are
full of `panic!("… {inst:?}")` — whereas a `}` inside a string literal is
never alone at column 0. All 24 such modules in the tree have that exact
shape; there is no `#[cfg(test)]` on anything but a module.

Measured on the union lcov: **1240 lines, 24 branches and 183 functions
excluded**, and the `sash/src` cold-line count drops 339 → 329. So test code
was contributing ten cold lines to the production ranking, `ice_text`'s four
among them. `ice_text` now reports zero.

`mk cov-db` prints the exclusion counts, so a scan that silently matched
nothing is visible rather than inferred.

## The interaction with the union — preserved

This does **not** stop unit tests from *warming* production lines, which is
the whole point of `mk cov-union`. The filter is on lines *defined* inside a
`#[cfg(test)]` block, not on which runs count. Confirmed in the same
measurement: the thirteen unit tests added this session warmed 22 production
lines across five functions, all of which left the queue.
