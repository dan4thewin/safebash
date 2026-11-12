title: "Research: structural defect detectors beyond coverage"
status: open
priority: P3
kind: research
depends: []
gates:
  - "a candidate is adopted only if it finds a defect on code where the answer was NOT already known"

# Research: structural defect detectors beyond coverage

## Why

Branch coverage is our only systematic "what should I read next" signal, and
it is a **finite runway**: it ranks code that *exists*, so it is blind by
construction to a **missing** match arm — the single most common defect shape
this project has produced. Journal 347 (`str` was a parse error in every word
context) is the canonical case: coverage flagged fifteen cold *sibling* arms
and bought a reason to re-read, but could never have flagged the absent one.

At ~89.6% branches the ranking will be exhausted well before the stale
reasoning it correlates with. This task is about what replaces it.

## The recurring defect shape

**A canonical answer with a narrower duplicate written beside it.** Four
instances in one stretch: journal 344 (`has_mutation_inner`,
`walk_arith_var_reads`), 346 (a dead keyword list), 347 (the `str` bug), 348
(a front end that ran three times). `check_expr_reads` in `validate.rs` carries
a comment recording a fifth (`SlurpFile`, where "the `_` fall-through made it
invisible").

`block_dupes` is **blind to all of them**: it is textual (4-line shingles, no
identifier renaming), and the 347 pair shared **0 identical lines and 0
shingles** because `Tok::If => "if",` and `| Tok::If` are textually disjoint.

## Candidate 1 — sibling-cohort omission detector (prototype exists, WORKED)

Sibling dispatchers over one enum define a *cohort*. A function covering 84–100%
of its cohort is asserting exhaustiveness; one covering 40% is being selective.
Flag only the near-complete ones and name what they omit.

- Extract per-function enum-variant sets; peers = functions whose variant set
  nearly contains this one (`|S ∩ G| >= 0.95|S|`); cohort = union of peers;
  flag `cohort - S` when `|S| / |cohort| >= 0.80`.
- On the whole compiler this yields **3 hits**, not hundreds — a reviewable list.

**It found a real bug.** On HEAD it flagged `transform/expressions.rs::expr_to_value`
as omitting `ExprKind::Test`. That was triaged as a probable false positive by
reasoning from the constructors; testing it instead found that `[[ … ]]` in a
pipeline compiled clean and died at run time (fixed, journal 349, SB4022).

Known pitfalls, both hit while prototyping — **the prototype is not trustworthy
as written**:

- Naive brace counting breaks on `{` inside string literals (`Some("{")`),
  which silently dropped `classify_word_token` from the index and produced a
  **false 0 hits**. Use indexnudge's tree-sitter symbol table, not regex.
- The threshold is delicate: **0.85 misses the known positive, 0.80 catches
  it**, calibrated against exactly one labelled case. Calibrate against the
  full labelled set (344 ×2, 346, 347) before trusting it.
- Functions with a `_ =>` catch-all make omission legitimate; all three current
  hits have one, so the wildcard is not by itself a discriminator.

## Candidate 2 — producer/consumer reachability

A variant constructed by nobody is dead (journal 346, found by inspection); a
variant consumed by nobody is a gap. Cheap: `symbols` + `string_literals` in
the indexnudge DB already hold what is needed. Would have found 346
mechanically.

## Candidate 3 — language-surface matrix (MEASURED, FAILED as built)

Reserved word × syntactic context, from the spec corpus; an empty cell names a
construct nobody ever writes. Sounded like the best fit for a compiler and was
ranked first **on speculation**. Measured on the pre-fix corpus it was the
worst of the three:

- **63% of cells empty** (94 of 150) — unreviewable.
- The one cell that mattered (`str` × bare-arg) read `1`, a **false positive**:
  `\s+` in the regex matched a newline, so an `echo` line followed by a `str`
  declaration line scored as a hit. It would have **missed 347 entirely** and
  reported false green.

Only worth revisiting if driven by **`sash`'s own lexer**, recording
`(token, parser context)` per spec line. As regex it is worse than nothing.

## Rejected

- **Mutation testing.** Catches weak assertions, not missing arms. No mutant is
  "add the arm that was never written", so it could not have found 347. Poor
  ROI for the defects we actually produce.
- **The `--emit-c` differential oracle** is blind to this class by
  construction: both backends share the front end, so 347 was identically wrong
  in both modes.

## Honest status

Measured so far, **no detector has beaten rereading as a discovery mechanism**
— every bug in this stretch was found by reading a caller. What coverage
actually buys is **attention allocation**, and it remains the best allocator
tested. Candidate 1 is the only one that has independently surfaced a real
bug, and only once.

## Next step

Rebuild candidate 1 on the indexnudge tree-sitter symbol table, calibrate
against the labelled set (344 ×2, 346, 347), and run it on HEAD. Adopt into
`contrib/` and wire to the pre-commit hook **only** if it clears the gate
above. Note 348's shape (duplicated computation, not duplicated variant sets)
needs a different signal; none is proposed.
