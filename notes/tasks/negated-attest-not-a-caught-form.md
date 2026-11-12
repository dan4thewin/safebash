title: "`if ! attest …` is not recognised as a caught attest"
status: done
priority: P2
depends:
gates:

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

`! cmd` never matches. **This filing originally named the wrong node.** It
said `StmtKind::Negated(Box<Statement>)`; that variant is documented in
`ir.rs` as appearing *only* as a stage inside a multi-stage pipeline. A
standalone `! cmd` is `StmtKind::Pipeline { negated: true, commands: [cmd] }`,
confirmed with `--emit-ir`. The consequence is the same — the `if` is never
recognised as a caught attest at all — DD#121's whole R2 machinery is then
skipped, the attest is walked as a *nested body* (where `JustTainted` starts
empty by construction), and R1 fires.

This is the same shape as the `Group` bug and the same shape as the SB5067
guard before it: **a guard keyed on one spelling of a construct**.

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

---

## Resolution (2026-10-02) — DG chose a third option: translate

DG: *"the `if ! attest` can be understood as `if attest { } else {...}`; if
the unmodified form is too vexing, why not translate?"*

Neither (a) nor (b). The polarity is **removed** rather than carried:

```
if ! attest C { A } else { B }   ==>   if attest C { B } else { A }
```

That makes the whole "three places need the polarity" section above moot.
`check_caught_resolution` and `lower_if_attest` never see a negated condition,
so their "resolve" and "divert" arms cannot drift out of step — the failure
mode that made (a) expensive.

### Where it lives

`parse/parser.rs::make_if`, the constructor both spellings of `if` now build
through. At construction, not in a later pass, for two reasons:

- **Totality.** A post-parse walk would need a *mutable* cmdsub walker, which
  does not exist — `structural_bodies_mut` deliberately excludes `$( … )`
  bodies and its doc says a rewriting pass that wants them "has to say so".
  Writing a mutable mirror of `walk_own_exprs` over ~31 `ExprKind` variants is
  precisely the fork-a-shared-walker hazard this project keeps getting bitten
  by. Normalising at construction reaches every `if` anywhere — including
  inside a command substitution — with no walker at all.
- **Precedent.** The parser already does this: `parse_if_brace`'s doc says
  "`else if` desugars to `else` + a nested `if`". It is also already
  attest-aware (`Tok::Attest`, and `statement_form_label` matches the name).

### Scope: attest-only

A blanket rewrite of every negated `if` was rejected. `if ! /bin/true { … }`
works correctly today and `--emit-ir` faithfully reports `:negated true` for
it; desugaring it would change a documented external seam (DD#104) for
programs that have nothing to do with attest. Pinned by `div.32`.

### The `elif` chain nests, it does not swap

The chain hangs off the *unnegated* condition being false:

```
if !C {A} elif C2 {B} else {D}   ==   if C { if C2 {B} else {D} } else {A}
```

A naive then/else swap would be a miscompile here. `negated_fall_through`
builds the nested `if`. Verified against bash on all three outcomes
(`div.30`, `div.31`).

### Not a new form, so DD#121 is not amended in substance

No negated caught attest reaches the analysis — only a new *spelling* that is
normalised away before validate runs. DD#121's three caught forms are still
exactly the three forms the analysis knows. A pointer was added to the DD so a
reader knows the spelling is accepted.

### Left alone, deliberately

`! attest x [[ … ]]` as a bare statement, and `while ! attest …`, still report
SB5088. Both are correctly *rejected* — a bare attest is uncaught, so negating
its aborting status is meaningless, and `while attest` is rejected unnegated
too, so the negated spelling is consistent. Only the **wording** is poor, and
it is poor for the unnegated `while attest` in exactly the same way, so it is
not a negation defect. Filed separately as
`sb5088-wording-for-non-caught-attest-positions`.

Specs: `div.24`-`div.32` in `spec/taint.sh`. Sweep 7257 -> 7275, zero failing.
