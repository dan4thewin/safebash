# 476 — The backstop that re-asked the question

DG, looking at the ICE arms accumulating in the transform:

> I still see no reason to keep an unreachable line unless it backstops a
> regression. We seem to be acquiring a lot of these — are the front-line
> validators so prone to failure? Surely it makes no sense to have a backstop
> for every error... if the backstop is essentially an identical test to the
> original then why is it more trustworthy; whereas if it has an orthogonal
> way to catch a problem, that is perhaps when it would be most valuable.

Three claims in there, and the third is the one that does the work.

## Are the validators prone to failure?

No — and the measurement says so plainly. There are **107 ICE sites** in the
compiler. Only **20 name an SB rule**, i.e. only 20 claim to backstop a
validator at all. The other 87 are structural: exhaustiveness pins, arms
unreachable because the same function already handled the case, totality
markers.

So the proliferation DG was seeing was not a vote of no confidence in the
validators. It was mine, in one narrow class, and it had a pattern.

## The mechanical test for orthogonality

DG's criterion sounds subjective until you notice it is decidable from the
ICE's **own parameter type**.

If the panic's parameter still holds the exact AST the named rule inspected —
`&ExprKind`, `&Command`, `&[Expr]` — and the rule is a `validate.rs` gate,
then *nothing happened in between*. The panic re-asks the gate's question
about unchanged data. It cannot catch anything the gate would not have caught
first, and it is free to drift from the rule it quotes.

If the parameter is `SsaValue` / `SsaInst` / a rendered `String`, or the named
rule lives in `parse/` (so the data went tokens → AST), it is orthogonal: it
also covers bugs in the lowering, which the gate structurally cannot see.

I got the classification wrong twice before it held. The first script read
*return* types too, which put `readline_target(&[Expr]) -> String` in the
orthogonal column. The second missed that `SB4014` is raised in `parse/`, not
`validate.rs`, so the two `attest` ICEs that cite it are orthogonal after all.
Final count: **11 duplicates, 9 orthogonal.**

Two of the orthogonal ones carry comments recording real bugs they caught —
`inst_handlers.rs:1155` (a wildcard handed the diagnostic a null segment) and
`validate.rs:2602` (a silent `None` is how a bare `attest` came to exec).
DG's criterion and the historical record agree, which is the best evidence
the criterion is right.

## Six removed

**`detect_array_iter` (4).** The gate (`SB5009`) decides a for-head `$( … )`
is allowed via `any(stmt_is_collection_intrinsic)`. The transform then
re-derived "is this `$(keys NAME)`?" by hand — `stmts.len() == 1`, then
Command, then Literal, then `== "keys"`, then `args.first()` — with an
`unreachable!` at each step. Two predicates for one rule, and the second one
spelled out in a different vocabulary from the first.

Now there is one: `ir::for_head_keys`, which *is* the shape, and the transform
reads it with `?`. All four panics went with the re-derivation. `None` is an
ordinary answer here — the same `None` every non-collection word gives, routing
to the word-list path `SB5009` has already vetted. Probed the three shapes the
old guards claimed to protect: `$(cat f; keys m)` → SB5073, `$(keys m; keys m)`
→ SB5073, `$(keys)` → SB4045. Nothing this declines reaches transform, and if
one ever did it would iterate as words rather than mis-lower as a collection.

**`try_len_intrinsic` + `join_intrinsic` (2).** These are the more interesting
pair, because the right move was not deletion.

Both guarded a splat operand against a *measured* consequence, carefully
documented: `expr_to_string` is a display renderer, so it writes `@a` as
`${aarrvals}`, which strips to `{aarrvals}` and matches no declared
collection. `len @a` would emit a `strlen` of an undefined var — a silent 0
where the element count belongs — and `join @a ,` a silent empty join.

The guard was right that the fallback was broken. It was wrong that a panic
was the fix. **DD#100 Part D retired `@a` as an *exact synonym* of the bare
name**, so the answer was knowable all along: the name is `pe.name`. One
shared `collection_operand_name` returns it, the general path then finds `a`
in `state.collections`, and `len @a` yields `arrlen` — the right answer. The
arm stops being unreachable-and-loud and becomes correct-and-cold.

This is AGENTS.md's existing rule — *unreachable by construction → ICE; merely
unexercised → the correct arm* — and I had not applied it because the arm
*looked* unreachable by construction. It was unreachable by **policy**
(SB5062), which is a different thing: policy can be reversed, and when it is,
the arm should work.

## Five kept, for a reason that is not orthogonality

`handle_argv` (×2), `handle_keys`, `readline_target` (×2) are duplicates by
the parameter test. They stay, and the reason is a second criterion the first
one does not capture.

`try_builtin` returns `Option<BlockId>` where `None` means **"not a
builtin"** — so a handler that declines does not produce a diagnostic, it
produces an ordinary command named `argv`, which compiles clean and dies at
runtime with `sash: bug: bare command passed to sash_exec`. That is not
hypothetical: the doc comment records that eight spellings reached it, which
is exactly why these handlers return `BlockId` rather than `Option` today.

So: declining is not a safe answer here, and the panic is the only way to say
"malformed". The test is not *is this a duplicate* but **what does declining
cost**. Where the fallback is a safe decline, delete the panic. Where it is a
silent catastrophic failure, keep it — and cite the consequence rather than
the gate name, because "SB4045 guarantees this" is just restating the
duplicate.

It is worth noting which sites survived: the three that already documented a
*measured* consequence. The ones that only named their gate are the ones that
turned out to be removable. That correlation is not a coincidence — writing
down what actually goes wrong is what forces you to check whether anything
does.

## Gates

`mk test` 81 + 27 + 20 green. Sweep **7751**, predicted before running
(3978 native + 3760 c + 13 skipped) — no new specs, so no change expected and
none seen. 0 failing, 0 flips, 0 new ids, gate self-test 248.

Unit-test census re-measured mechanically: **81 tests, 65 ICE assertions, 16
ordinary** (was 80/68/12). Three ICE tests became ordinary ones: they now
assert the behaviour the panics were reaching for — that an unrecognised
for-head is *declined* rather than mis-lowered, and that a splat operand
resolves to the bare name so `len` counts elements and `join` names the
collection. No spec can make those assertions; every shape involved is
rejected before transform runs.

Commit `50ab2f2`.
