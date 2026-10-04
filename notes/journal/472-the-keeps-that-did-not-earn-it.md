# 472 — the keeps that did not earn it

DG, reading the previous session's summary: *"why are we keeping lines that
are unreachable and not a backstop?"*

The honest answer is that we shouldn't be, and we were. Reviewing my own DD#116
verdicts against the rubric rather than defending them, **three of the keeps
failed**, and in two different ways.

## The rubric, restated

DD#116 lets a cold line stay only if it earns the place with at least one of:

- **loud-vs-silent** — it turns a wrong answer into an error;
- **totality** — the arm must exist, so only its *body* is a choice;
- **specification fidelity** — the line *is* the rule, not a second opinion.

Anything else is a **rival**: a second, contradicting answer to a question some
other code already settles. Rivals get deleted. And AGENTS.md draws the
orthogonal line: *unreachable by construction → `unreachable!()`; merely
unexercised → the correct arm*.

Four of the eleven sites I closed yesterday were "keeps". The test is cheap and
I did not apply it properly.

## Keep 1 — `"strlen" | "arrlen" => VarType::Int`

I wrote that this was specification fidelity: `transform/pipelines.rs` builds
an AST `${arr arrlen}`, so the operator is live and the arm is cold only
because type-check runs first. That reads well and is wrong on every clause.

- **Not totality.** The match ends in `_ => VarType::String`. The arm is
  optional; deleting it changes nothing that can run.
- **Not specification fidelity.** DD#75 §2 / DD#102 say a length is an int, and
  `is_int_returning_intrinsic` is where that lives — keyed on `len` and
  `strlen` as *intrinsics*, which is the live spelling. This arm was keyed on
  `${#x}`, retired by DD#75 §4. Two answers to one question.
- **Unreachable, and not marginally.** `parameter_expansion_type` has one
  non-test caller. The only AST producer of operator `arrlen` is
  `transform/pipelines.rs:91`, downstream of it. Nothing in the tree produces
  operator `strlen` in an AST at all — the parser function still *named*
  `try_strlen_or_arrlen` emits `len_retired`, and every `${#…}` spelling is
  SB4038 at parse. I probed all four.

So: a rival. Deleted.

The part worth sitting with is the unit test I wrote to "pin" it. It built the
node by hand and asserted `Int`. It passed. It will always pass. What it
actually pinned was *my claim about pass ordering*, and it made a dead arm look
verified — which is worse than leaving it cold, because a cold line still shows
up in the queue and a green test does not. AGENTS.md warns about manufacturing
a node to warm a line; I did it to *bless* one.

### The twin

Chasing the producer turned up the same relic downstream.
`transform/expressions.rs` read `if op == "strlen" && state.collections.contains(…)`
and promoted to `arrlen` — DD#75 §2's dispatch-on-type, keyed on the retired
spelling, unreachable for exactly the same reason. The live implementation of
that same dispatch is `expr_handlers::try_len_intrinsic`, for `$(len x)`.
A second rival of a second function. Deleted.

Two stale copies of one rule, in two passes, both surviving the surface's
retirement because nothing fails when a dead branch is wrong.

## Keep 2 — `attest_arith_near_miss`'s `[] => None`

Yesterday's comment: *"The parser always consumes a name after `attest`, so
this is only reachable from a hand-built IR. Kept as a loud-vs-silent
backstop."*

Those two sentences contradict each other. "Only reachable from a hand-built
IR" **is** the definition of unreachable-by-construction, and the body was
`None` — the silent answer. Probed: `attest (( x = 1 ))` is SB4014 "expected
variable name after 'attest'", in both spacings.

Now an `unreachable!` naming SB4014, pinned by `#[should_panic]`. The panic is
the claim. If the parser's guarantee ever breaks, it breaks loudly in the
opener's own sweep instead of waving a nameless `attest` onto the generic
command path — which is how a bare `attest` came to *exec* in the first place.

## Keep 3 — `attest_scalar_form_error`'s map/array split

This one I had already flagged, because its own comment convicts it: *"The arm
is cold because `type_check` sees few map attests, **not because it cannot
run**."* Under DG's standing rule — every line warm by spec except ICEs — that
sentence is a spec gap wearing a backstop's coat.

Finding the shape took four probes, and the first three were all shadowed by a
different diagnostic:

| probe | result |
|---|---|
| `attest m` | SB4014 — not a complete form |
| `attest m [[ -n $m ]]` | SB5064 — scalar sigil on a collection |
| `attest m [[ -n "z" ]]` | SB5086 — always-true test |
| `attest m [[ -n ${m[k]} ]]` | **SB4030**, the arm |

Three gates stand in front of it, which is why it stayed cold and why "few map
attests" was a plausible-sounding story. It is genuinely reachable, and the
backstop claim is genuinely correct: without the split, a scalar attest on a
map advises `attest x in m` — the *array* binder form — and the user who
follows that advice earns a second diagnostic. Specs 61.23-61.25 warm both
arms and a control.

## Keep 4 — `detect_array_iter`'s `return None` guards

This one survives review. "Only `$(keys NAME)` iterates" is the rule itself,
and the `None` is the function stating it rather than a second opinion about
it. No change.

## The score

Four keeps, three wrong: one rival (plus its downstream twin), one ICE
mislabelled as a backstop, one real backstop that needed a spec instead of a
comment. The common thread is that **naming a property is not testing one.**
`a2967a4` and my own follow-up both added prose that asserted reachability, and
in two of three cases the prose was false — provably so, in under a minute,
with the binary sitting right there.

A keep should cost a probe. If you cannot produce the shape that reaches the
line, you do not know it is a backstop; you know you have a story about one.

## A gate fault, found by predicting

Adding three specs should produce six new ids. The sweep's companion query
reported **zero**.

`61.13`-`61.15` already existed in `runs` — real specs on 2026-09-27 at
`deb8a26`, later deleted. The purge drops the `tests` row and keeps the history
by design, so the ids looked free to both mechanical collision checks (nothing
*current* used them) while each still carried a week-old row. The regression
query therefore compared three brand-new specs against three unrelated retired
ones. Both sets passed, so it said nothing.

The source's high-water mark for `61.x` was 12. The database's was **22** — ten
retired ids in the gap. Renumbered to `61.23`-`61.25`; the companion now reports
exactly the six I added. AGENTS.md gains the query and the tell.

This is the third time a spec-suite fault has been caught by predicting a number
before running, and the second time the symptom was a count that looked
*reassuring* — zero new ids reads like "nothing unexpected happened".

## The eleven, audited

DG followed up: *"have all the 11 shadowed items been handled now?"* Audited
against the tree rather than from memory, and one gap turned up — the same
shape as the three above.

| # | site | verdict | evidence |
|---|---|---|---|
| 1-2 | `dynamic_subscript_var`'s `${name}` arms | **bug** | warm by specs 80.123-130 |
| 3 | `parameter_expansion_type` `strlen\|arrlen` | rival, deleted | all four `${#…}` → SB4038 |
| 4 | `push_source_char` `_ => 1` | ICE | `Scanner::new(&str)` — untestable by construction, ranges named instead |
| 5 | `logos_input` Err arm | rival, deleted | 2 unit tests |
| 6 | `"" => return` on a command name | rival, deleted | `$c hi` / `${c} hi` → SB5011 |
| 7 | empty `exec` target | rival, deleted | `exec ''` → SB5011 |
| 8 | `check_arith_reads` `SubscriptAssign` | ICE | `$(( a[i] = 7 ))` → SB4020, `#[should_panic]` |
| 9 | `parse_assignment_lhs` unclosed subscript | ICE | `a[0=5` → SB5011, `#[should_panic]` |
| 10 | `parse_split_subscript_index` SB4014 | keep | 40-token index → SB5011; unit test |
| 11 | `parse_assign_op` `_ => Err` | keep | **had no test** |

Ten were concluded. **#11 was not** — and the way it slipped is instructive. My
own summary said it was "already annotated by `a2967a4`", so I treated it as
reviewed. It was annotated by `aab5868`, a different commit with a different
purpose, and nothing had ever executed the claim.

Probed now: the shape that originally broke it, `m[$s ] x ]=9`, is SB4041
today — so the keep is sound, and `lhs_index_close_from` being shared by both
the probe and the parser is what makes it structural. But "sound" was an
inference until the probe, and the arm is specifically the thing that makes a
future disagreement *loud* instead of a return to silently assigning the
literal `]=9`. That is worth executing. One unit test, 107 → 108.

Site 4 is the documented exception: `Scanner::new` takes a `&str`, so no unit
test can hand it an invalid byte. AGENTS.md's rule for that case is to make the
arm name its variants rather than chase it, and it does — `0x80..=0xBF |
0xF8..=0xFF`.

## Gates

`mk test` 107 + 27 + 20. Sweep native 3946 + c 3728 + 13 skipped = **7687**,
predicted exactly. Zero flips, self-test 248, zero failing, exactly six new
ids, one expected retirement (the three stale `61.1x` pairs).

The audit addendum added one unit test (108) and two doc comments; no compiled
behaviour changed, so the sweep was not re-run for it.

## The two keeps were not on the axis I thought

DG, reading the two surviving parser keeps: *"the 2 keeps deliver errors, that
makes them backstops, yes? but they aren't ICE?"*

Both halves are right, and together they name an incoherence I had walked past
twice. There are **two axes**, and I had been answering only the first:

1. **DD#116 — does the line earn its place?** A cold line stays only if it is
   *loud-vs-silent*, *totality*, or *specification fidelity*. An `Err` return
   is loud. So yes: on this axis both are backstops, and both keeps stand.
2. **AGENTS.md — can it run?** *Unreachable by construction → `unreachable!()`.
   Merely unexercised → the correct arm.* I never asked this one.

Answering the second cost one probe apiece, and both came back **unreachable
by construction**. `parse_simple_or_assignment` only enters `parse_assignment`
through `looks_like_assignment`, and both of that probe's branches pin the very
token each keep was guarding:

- Branch A requires `peek_n(1)` to be `Eq | PlusAssign` *and*
  `is_identifier_or_subscript(w)` — which, for a word containing `[`, demands a
  `strip_suffix(']')`. `parse_assignment` then consumes exactly the one Word,
  so `parse_assign_op`'s `peek()` **is** the probe's `peek_n(1)`.
- Branch B is `lhs_index_close_from(scanner, 1).is_some()`.
  `parse_split_subscript_index` calls the *same function* at `base = 0` after
  the `name[` word is consumed — the same absolute tokens, the same window.

So a diagnostic guarding a *compiler* invariant. If the invariant ever broke,
the message would be a **lie**: "expected `=` or `+=` in assignment" about a
program that visibly has one, "unsupported dynamic subscript index" about a
perfectly supported index. It would point the user at their own correct source.

The internal-consistency argument is sharper still. Site #9 —
`parse_assignment_lhs`'s `rest.find(']')` — is in the *same function family*,
guarded by the *same probe*, and has been an `unreachable!` all along. Three
sites, one invariant, two different answers.

Both are now ICEs naming the probe that guarantees them, the `Result` wrappers
are gone (`-> Expr`, `-> AssignOp`), and the two unit tests became
`#[should_panic]`. The tests had asserted `err.code == "SB4014"` — they were
executing the *wrong claim*, confirming a message rather than the invariant.

The adversarial probes, all shadowed: `a[=9`, `a[]=9`, `a[$i][0]=9`,
`a[$i] x=9` → SB5011; `m[$s ] x ]=9`, `a[$i ] x ]=9` → SB4041. The tell is the
40-token index: when the probe's lookahead *fails*, `looks_like_assignment`
returns false and `parse_assignment` is never entered at all — SB5011, as a
command name.

**Tradeoff, stated rather than buried** — and settled by DG in one sentence:
*"the panics are guards against regression — they never need to be the sole
defense, as a regression would be something we fix."*

I had framed it as a defense-in-depth question: if a future grammar extension
forgets to update the probe, is a panic worse than a diagnostic? That framing
was wrong, and the error is instructive. It treats the ICE as the thing
standing between a user and a bad outcome, which would make its quality as a
*message* matter. It is not. The probe is the defense; the ICE only has to
notice when the probe stops being one. A regression is not a condition to
degrade gracefully under — it is a condition to fix, and the ICE's whole job is
to make sure someone is told to fix it.

Which also disposes of "a parser conventionally errs rather than panics". The
convention is about *user* input. Neither of these arms can be reached by user
input, so the convention does not reach them.

Sweep: 7687 (3946 native + 3728 c + 13 skipped), zero flips, zero new ids, gate
self-test 248. `mk test` 108 + 27 + 20. Both messages were asserted by no spec.
