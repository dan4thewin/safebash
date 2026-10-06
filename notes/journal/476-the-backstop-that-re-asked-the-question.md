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

Commits `50ab2f2`, `6a44932`.

## Correction, same day — the answer nobody could ask for

DG asked the obvious verification question I had not asked myself: *"len @a and
join @a are SB5062, yes?"* I had asserted it from the doc comments. Measured,
it holds — every spelling is rejected, at the single site that raises SB5062
(`validate.rs:8833`): `len @a`, `join @a ,`, `len @args`, `join @args ,`,
`len @{a:-}`, `len @{a:~}`, `"$(len @a)"` and `join $a ,`; `len $a` is SB5064;
the bare forms compile. That also retroactively justified unifying the two call
sites, which had differed — `len` stripped the `$` sigil and `join` did not.
Neither can see a sigil at all.

But confirming it exposed that "give it the correct answer" had not actually
satisfied DG's rule. The arm stopped being a duplicate assertion and stayed
**unreachable**. By his own criterion it had not earned its line either, and he
named a cost I had missed:

> delete - it risks a reader mistaking the spelling as legitimate

That is the sharper point, and it is about the *reader*, not the compiler. A
panic at least reads as "this cannot happen". An arm that quietly handles `@a`
reads as **support for `@a`**, which is false, and the next person to touch the
function inherits the false belief. So the arm is gone;
`collection_operand_name` is the bare read, with a doc comment recording why
there is deliberately no splat case and what was measured.

The three unit tests written to pin the "correct answer" went with it, along
with their fixture and import. Nothing replaces them: the behaviour is pinned
by the specs for `len a` / `join a ,`, and a unit test for the rejected
spelling would assert SB5062 — the duplicate this whole thread started as.

**The refined rule:** *"the answer is knowable, so give it"* applies only when
the arm becomes **reachable**. Where the gate rejects every spelling, delete,
and let the diagnostic be the one place the rule is stated.

There is a methodological point too. I wrote "SB5062 rejects it" three times —
in a commit message, a journal entry and an AGENTS.md rule — on the strength of
a doc comment I had read, and it took DG asking to turn that into a probe. The
claim happened to be true. It was still unverified when I shipped it, and the
cheap check that settled it took one minute.

Gates after: `mk test` rc=0 (78 + 27 + 20), sweep **7751** again as predicted,
0 failing, 0 flips, 0 new ids, self-test 248. Census now **78 tests, 65 ICE,
13 ordinary**.
