title: 323 — Three cold branches, one silent infinite loop

`lower_c_style_for` parsed each of the three slots of a C-style `for`
header and then threw the error away:

```rust
if let Ok(expr) = parse_arith_str(init_trimmed, span) {
    init_stmts.push(lower_arith_command(expr, span));
}
```

…and, for the condition:

```rust
} else if let Ok(expr) = parse_arith_str(cond_trimmed, span) {
    vec![lower_arith_test_structured(&expr, span)]
} else {
    vec![Statement::true_cmd(span)]
}
```

That last `else` is the whole bug. A malformed condition does not fail to
compile — it becomes `true`.

| source | compiles? | runs as |
|---|---|---|
| `for (( i=0; i <* 3; i++ ))` | **yes, silently** | **infinite loop** |
| `for (( i=0; i < 3; i +*+ ))` | **yes, silently** | **infinite loop** (update dropped, counter never advances) |
| `for (( i=*0; i < 3; i++ ))` | **yes, silently** | loop from the variable's default |
| `(( i <* 3 ))` — same garbage | no | SB4020 |

Bash reports a syntax error for the first one. sash compiled it to a
program that never terminates.

## How it surfaced

Three rows of the branch table, from the measure at `b9ca092`:

```
sash/src/parse/arith.rs|295|0|1
sash/src/parse/arith.rs|304|0|1
sash/src/parse/arith.rs|314|0|1
```

Three `Err` arms, never taken by any spec. That is the fifth time in this
coverage campaign that the cold branch **was** the bug rather than merely
pointing near one. The pattern is now consistent enough to state plainly: a
branch no test has ever taken is a branch nobody has ever checked the
behaviour of, and error-discarding arms are where that costs the most.

## The other half: DD#31 has no for-header exemption

I got to `lower_c_style_for` by a different route — chasing whether
`ArithExpr::PreIncDec` was dead. `$(( ++x ))` and `(( ++x ))` are both
SB4020, so `validate.rs`'s PreIncDec arm and `arith_brief`'s looked
unreachable. Before calling them dead I went looking for the spelling that
reaches them, which is the discipline that keeps this campaign honest, and
found it:

```
for (( i=0; i < 3; ++i ))   → compiled, ran 0 1 2, matched bash
for (( ++i; i < 3; i++ ))   → compiled, ran 1 2, matched bash
for (( i=0; ++i < 3; ))     → SB5007, for an unrelated reason
```

Three slots of one construct, three different answers, none of them the
SB4020 that the same spelling gets everywhere else. DD#31 is categorical —
*"Only postfix `x++` and `x--`. No prefix forms."* — and names no
exemption for a `for` header, so this was not a deliberate carve-out. I
checked `notes/decisions/` before touching it precisely because it might
have been.

Same root cause as the infinite loop: `lower_c_style_for` was the one
arithmetic entry point that did not go through the shared checks.
`parse_arith_expansion` and `parse_arith_command` both call
`reject_prefix_incdec`; the `for` header called `parse_arith_str` raw.

## The fix

One shared `parse_arith_slot` — parse, `reject_prefix_incdec`,
`reject_nested_mutation` — now used by `parse_arith_command` and by all
three `for` slots, and `lower_c_style_for` returns `Result`. The caller
already returned `Result<Option<Statement>, Diagnostic>`, so propagating
was one `?`.

An empty slot is still `true`, because `for (( ;; ))` is a deliberate
spelling. That is now the *only* thing that can become `true` — previously
a syntax error could too, and the code could not tell the two apart.

## A near-miss worth recording

`reject_prefix_incdec` recurses into BinOp, Assign, CompoundAssign, Comma
and Paren, and lets everything else fall through `_ => Ok(())`. Ternary,
UnaryOp, FuncCall and Subscript are **not** covered, so I expected
`$(( 1 ? ++x : 3 ))` and `$(( a[++i] ))` to slip. They do not: a second,
broader check catches embedded mutations first. The hole is real but
currently shadowed. I have left it alone rather than "fixing" a condition
whose only effect today would be to change which of two correct
diagnostics fires.

## Downstream

`sash-convert` emits prefix `++`/`--` verbatim in every context, so it
produces sash that does not compile. That is **pre-existing and general**,
not a regression from this change — `$(( ++i ))` and `(( ++i ))` have
always been SB4020; the for-header case merely used to be accepted by the
bug fixed here. Filed as `convert-prefix-incdec`. It is not a pure
rewrite — prefix in a *value* context has no postfix equivalent in sash —
so the disposition is converter policy and is DG's call.

## Ledger

Eight specs, `cov.160-167`: three malformed slots, three prefix slots, the
bare-`(( ))` control, and a comma-slot header that must keep working.
`spec/coverage.sh` plan 158 → 166.

Sweep **4853 passing / 10 skipped / 0 failing**, both modes. 36 lib tests.

Branch baseline re-measured at `b9ca092` before the fix: **86.35%, 512
uncovered** of 3752 — up from 510 because SB5077 added 40 new branches, most
of them immediately covered. The three `Err` arms this journal is about are
gone from the denominator entirely, which is the better kind of progress.
