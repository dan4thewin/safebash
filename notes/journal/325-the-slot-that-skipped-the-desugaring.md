# 325 — the slot that skipped the desugaring

Bug #6, and the sixth time in this campaign that a cold branch was a bug.

The branch was `validate.rs:1755` block 0 branch 0 — the `ArithExpr::Comma`
arm of `validate_arith_in_condition`. DD#31 permits a comma condition whose
non-final parts are mutations, so something had to reach that arm. Four
spellings of `while`/`until` did not. The fifth, a C-style `for` header, did.

That is already the answer, though it does not look like one yet: if the
`for` header is the *only* thing that reaches the comma arm, then the other
three condition contexts never build a `Comma` node to validate. They
desugar it first. The `for` header does not.

## What the bypass cost

`lower_c_style_for` lowered its test slot with
`lower_arith_test_structured` — straight to a single `ArithTest`. Every other
condition goes through `lower_arith_command`, which asks
`classify_arith_expr` first: a comma holding a mutation anywhere classifies
as `Mutation`, and the mutation path splits it into a `Group` of statements.
The comma is gone before validation runs.

Skipping that left the mutation embedded in one `ArithTest` node, where
`check_arith_test_placement` correctly rejects it. So:

| | sash | bash |
|---|---|---|
| `while (( k++, k<3 ))` | `1 2` | `1 2` |
| `if (( k++, k<3 ))` | `1` | `1` |
| `until (( k++, k>2 ))` | `1 2` | `1 2` |
| `for (( ; k++, k<3; ))` | **SB4020** | `1 2` |

One of four. The other three had always worked.

DG's first question was the right one: the header already has slots for
mutations, so what is actually lost? The init slot does take a comma
(`for (( i=0, j=1; i<3; i++ ))` has always worked). But the slots are not a
substitute for this shape — `for (( ; k<3; k++ ))` tests before it
increments and prints `0 1 2`, not `1 2`. The equivalent is
`for (( k++; k<3; k++ ))`, which writes the mutation twice.

The fix is the bypass: call `lower_arith_command`.

## What the fix cost, measured

Old binary against patched binary, every for-header comma shape:

| shape | before | after |
|---|---|---|
| `for (( ; k++, k<3; ))` | SB4020 | `1 2` |
| `for (( ; k<3, k++; ))` | 3 diagnostics | 1 diagnostic, still SB5007 |
| `for (( ; k=1; ))` | SB5007 | identical |
| `for (( ; k<3, k>0; ))` | SB5007 | identical |

Row two is the entire cost, and it is wording, not enforcement. The message
it loses — *"final comma expression must be a comparison in condition
context"* — was the `for`-header-only spelling of a rejection the shared path
already performs. What replaces it is the message `while (( k<3, k++ ))` has
always given.

## The dead code the fix created

`validate_arith_comma_in_condition` had two halves. After the fix only one
of them can run.

A comma that holds a mutation classifies as `Mutation` and is decomposed, so
the comma arm of `validate_arith_in_condition` now only ever sees a
**mutation-free** comma. Its final-part check — "is the last element a
mutation?" — is therefore always false, and the `for` header was its only
route. Verified across five spellings after the fix: still cold.

DG's call: delete it. The reasoning that makes that safe is worth stating
plainly, because "delete a check that enforces a DD clause" reads badly
otherwise — **the clause is still enforced.** `validate_assignment_stmt`
rejects a mutation in condition position for all four contexts, on the
decomposed form. What was deleted is a duplicate that only the buggy path
could reach. What stays is the half nothing else can see: every non-final
part must be a mutation, which is checked on the comma itself because after
decomposition there is no comma left to check.

That asymmetry is the thing to remember. A rule enforced *before* desugaring
and a rule enforced *after* it are not interchangeable, and which side a
check sits on decides whether a lowering bypass can hide it.

## Verified

Six specs, `31.53`–`31.58`: the comma form across all four condition
contexts plus the two shapes that must stay errors. Sweep **4873 passing,
10 skipped, 0 failing**.
