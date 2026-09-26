---
status: open
priority: P3
depends: DD80-braced-key-discipline
gates: DG decision on whether an array index must be declared
---

# Array indices that are provably strings but not *declared* `str`

Journal 412 closed the declared case: `str s=abc; ${a[$s]}` is now SB4030 on
read, write and append. It is narrow on purpose. `TypeEnv::var_type` ends in
`unwrap_or(VarType::String)`, so "declared a string" and "never recorded" are
the same answer, and a bare `i=1` records `String` exactly like `str s=abc` —
sash types a bare numeric literal as a string, following bash.

Rejecting every string-*typed* index costs 8 specs in both modes, two of them
DD-blessed idioms (`attest i i?`, 60.70; `${arr[$1]}`, 80.31). So only a
positive `str` declaration is trusted today.

## Still silent

| shape | decidable? |
|---|---|
| `s=abc; ${a[$s]}` | yes — value-flow: `expr_type` already sees a non-numeric literal initializer |
| `s=$(cmd); ${a[$s]}` | yes — a capture is positively String |
| `${arr[$1]}` with a non-numeric argument | no — genuinely runtime |

The first two need `declared_str` to become "string by evidence" rather than
"string by declaration": populate it from `infer_assignment_type` when the RHS
is positively a string (non-numeric literal, capture, string intrinsic), not
merely when it defaults there.

**The blocker is 60.70.** `i=$(printf 1); attest i i?` is a capture — it would
be caught by the "capture is positively String" rule, and it is DD#93's
documented route for a tainted index. Closing the capture case requires
`attest x i?` to retype `x` as Int, which is arguably what `i?` (the integer
predicate) already proves. That is a DD#93 question.

## The larger question

Whether an array index should require `int` at all — making the index surface
fully declared, with `attest … i?` typing to Int so the idiom survives, and a
positional needing an explicit crossing. Maximum consistency with DD#77/DD#80;
a real breaking change to a common idiom. DG's call.

## See also

- `notes/journal/412-the-type-that-was-only-half-known.md`
- `notes/tasks/undispositioned-string-to-int-sites.md` — the sibling inventory
- spec 80.47 — the backstop pinning the deliberate silence
