---
status: next
priority: P3
kind: chore
---

# coverage ranking counts `#[cfg(test)]` code as production cold lines

`ice_text` (`sash/src/transform/expr_handlers.rs`) sits in the by-function
cold-line ranking with 4 uncovered lines. It is a **test-only helper** — the
whole module is `#[cfg(test)]` — and its cold half is the `String` downcast
branch that no current ICE payload takes.

It is not work, and it displaces real candidates from the head of the queue.

## The fix

`contrib/coverage.sh --lcov` already excludes `tests\.rs` and `/tests/` by
filename. That does not catch a `#[cfg(test)] mod invariant_tests` living
*inside* a production file, which is now the dominant unit-test shape in this
repo (AGENTS.md asks for it explicitly: "Put the module in the **same file**
— most of these functions are private").

Two options, DG's call on which:

1. **Filter at load.** `contrib/covdb.py` learns the line ranges of
   `#[cfg(test)]` modules per file and skips their DA records. Keeps the
   ranking honest for every consumer of `coverage.db`.
2. **Filter at query.** The ranking SQL in AGENTS.md joins against a new
   `symbols`-derived exclusion. Cheaper, but every ad-hoc query has to
   remember it.

(1) is the one that cannot be forgotten.

## Note the interaction with the union

This must **not** stop unit tests from *warming* production lines — that is
the whole point of `mk cov-union`, and journal 478 is the first time it was
watched working (three functions went from cold to no-cold-lines purely
because their ICE pins landed). The exclusion is about lines *defined* inside
a `#[cfg(test)]` block, not about which runs count.
