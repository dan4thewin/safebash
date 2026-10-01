---
status: open
priority: P2
depends:
gates:
---

# `if ! attest …` is not recognised as a caught attest

DG (2026-10-02): *"why isn't `attest s g? || { exit 1; }` handled identically
to `if ! attest s g? { exit 1 }`?"* The first is fixed
(`block-falls-through-blind-to-group`). The second is a different defect with
a different cause.

```sash
str x=$(getenv HOME)
if ! attest x [[ $x =~ ^/ ]] { exit 1 }
echo "$x"
```

is rejected **SB5088** — *"attest of 'x' is not adjacent to the operation
that tainted it"* — which is false. The attest is adjacent. The diagnostic
also sends the author somewhere unhelpful: it advises moving the attest or
reaching for `${x:!}`, when the program is already in the shape DD#121 wants.

## Cause

`validate.rs::condition_attest` is keyed on syntax:

```rust
match &only.kind {
    StmtKind::Command(c) if expr_literal_str(&c.name) == "attest" => Some(c),
    _ => None,
}
```

`! cmd` is `StmtKind::Negated(Box<Statement>)`, so the `if` is never
recognised as a caught attest at all. DD#121's whole R2 machinery is then
skipped, the attest is walked as a *nested body* (where `JustTainted` starts
empty by construction), and R1 fires.

This is the same shape as the `Group` bug and the same shape as the SB5067
guard before it: **a guard keyed on one spelling of a construct**. The
peel idiom already exists two thousand lines up the same file —
`stmt_is_external_command` does `while let StmtKind::Suppressed(inner) |
StmtKind::Negated(inner)`.

## Why it is not a one-line fix

Peeling `Negated` in `condition_attest` is the easy part. The polarity has to
travel with it, because the negated form **swaps which body is which**:

```sash
if ! attest x [[ … ]] { exit 1 }     # then-body = the FAILURE path
if attest x [[ … ]] { … } else { … } # then-body = the SUCCESS path
```

Three places need the polarity:

- **R2's check** — which body must resolve the subject, and which may divert.
  Today `check_caught_resolution` assumes the else is the failure path.
- **`transform/control_flow.rs::lower_if_attest`** — `push_attest_retag`
  must go on the block reached when the attest *passed*, which for the
  negated form is the else/fall-through block, and the `Branch` terminator's
  true/false blocks swap. This is the code that was just corrected for the
  merge; getting the polarity wrong there is a miscompile, not a diagnostic.
- **R1** — the negated statement must be treated as the statement itself
  rather than a nested body, or `JustTainted` keeps resetting.

## The question for DG

DD#121 names three caught forms — `if attest …`, `attest … && CMD`,
`attest … || CMD` — and does not name a negated spelling. So this is not
purely a repair; it adds a form, and the DD should say so.

- **(a) Support it.** `if ! attest` becomes a fourth caught form, DD#121
  amended to name it. It is the natural spelling of "bail out if this isn't
  trustworthy" and it is what DG reached for unprompted, which is evidence
  about what readers expect. Costs the polarity work above.
- **(b) Reject it deliberately, with an honest diagnostic.** Keep the caught
  forms as DD#121 lists them, but detect the negated attest in a condition
  and say so — "a caught attest may not be negated; write
  `attest x [[ … ]] || { exit 1; }`" — instead of the false SB5088. Cheap,
  and keeps a single spelling for the analysis to reason about, which is the
  locality property R2 was bought for.

Either way the current behaviour is a bug: the diagnostic states something
untrue about the program.

A bare `! attest x [[ … ]]` (not in a condition) is also SB5088 today. It is
nonsense in any case — an uncaught attest aborts on failure, so negating its
status cannot mean anything — but it should say that rather than claim
non-adjacency. Covered by (b) either way.
