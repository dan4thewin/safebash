# 420 — The step that changes nothing

`$!` → `$(getcpid)`, the second of DD#111's three. Unlike `$$`, this one is
**behavior-preserving by construction**, and saying so plainly is the point of
this entry: there was no bug to fix here, because `a875d58` had already fixed
the symptom by hand.

That is exactly why the step was worth taking. `a875d58` made `$!` int-typed by
**seeding `intrinsic_int` with `!`** — patching the type table so the
punctuation form agreed with the intrinsic it is sugar for. It worked, and it
left two encodings of one fact in the tree. The desugar removes the second
encoding instead of maintaining it.

## What came out

Two folds ran in the wrong direction and are gone:

- `bareword_shortcut_expansion` collapsed `$(getcpid)` into
  `SimpleExpansion("!")` — the intrinsic disappearing into the sugar.
- `parse_getcpid_word` did the same at the word level, returning
  `SpecialWord::Returned(SimpleExpansion("!"))`. It now returns
  `intrinsic_expansion("getcpid", span)`.

Both previously produced `SsaValue::Var("!")` via the parser fold; both now
produce it via `resolve_single_cmdsubst` → `nullary_magic_var`. Same SSA, one
path.

And the seed itself:

```rust
intrinsic_int.insert("!".to_string()); // $!
```

**is deleted.** The argument that it is dead is not "I looked and found no
callers" — that is the claim journal 386 warns about. It is structural, and it
has two halves:

1. `var_type` is only ever reached for a magic name through
   `ExprKind::SimpleExpansion`, and `assert_no_magic_sugar` now *guarantees*
   no `SimpleExpansion("!")` survives parse.
2. The only other way to write the name is `${!}`, and SB4003 rejects that as
   an indirect expansion before typing runs.

So the seed could not be consulted even in principle. I checked the second half
by running it rather than reading it.

## The neuter, and what it did *not* catch

Removing the `"!" => "getcpid"` arm while leaving the seed deleted makes `$!` a
String again, and four specs fail across three files:

| id | file | what it pins |
|---|---|---|
| `cov.252b` | coverage.sh | `int c=$!` + redirect |
| `55.19` | errors.sh | `p=$!; wait $p` — the canonical idiom |
| `isink.11` | errors.sh | `wait $!` under the int-only rule |
| `spv.4` | misc.sh | `$!` and `$(getcpid)` are the same value |

The interesting result is the one that **did not** fail. `sx.25` in
`arithmetic.sh` —

```
/bin/sleep 0 & int a=$(( (getcpid) )); int b=$(( $! )); echo $(( a == b ))
```

— passes with the desugar neutered, because inside `$(( ))` a string operand
coerces anyway. It reads like a guard on the two spellings agreeing and it is
not one; the arithmetic context erases the distinction it appears to test. I
have left it alone (it pins what it was written to pin, the sexpr name) but it
is worth knowing that it is not evidence for DD#111.

## No new specs, deliberately

The four above already pin every property this change could break, and the
change adds no new observable behavior. Writing `spv.9`–`spv.12` to mirror the
`$$` block would have been padding — four more ids asserting what `cov.252b`
and `spv.4` already assert, inflating the suite total while adding nothing a
neuter can distinguish. The honest report for a structural step is that the
existing gate covers it.

## AGENTS.md: the regression check can now fail

Fixed the vacuity found in journal 419. `commit_hash` is not a sweep identity —
`runs` stores the git HEAD sha, so every sweep against a dirty tree records the
same one, and `MAX(rowid) … WHERE commit_hash = 'BASE'` hands you the run you
just did.

The replacement compares by **row order**: sweeps are never parallel, so each is
a contiguous block of rowids and a test's previous result is its previous row.
The baseline is `rowid < cur.rid`, strictly earlier, so it is structurally
incapable of self-comparing however dirty the tree is. `cur` joins `tests` so
retired ids cannot raise a phantom flip out of two stale historical rows, and a
companion query lists ids with no earlier row instead of letting new tests
vanish into the join.

Three additions that matter more than the query:

- **The cheap tell**, to run first: a hash whose row count exceeds its distinct
  id count covers more than one sweep. Keep the `* 1.0` — integer division
  reported 26056 rows over 6522 ids as `3` when it is four sweeps.
- **A non-vacuity probe**, because this is a gate: comparing each test's first
  recorded status to its current one must report hundreds (231 today). A zero
  there means the machinery is broken, not that the tree is clean.
- The commit-hash form is kept for deliberate cross-commit work, but takes the
  **first** sweep at that hash rather than the last.

Every SQL block in the section was executed against `spec.db` before committing,
and the new check was then used as the gate for this commit.

## Gates

- Sweep **6518**, unchanged and predicted — no specs added, none retired.
- New regression check: `flips=0 new=0 total=6518`; non-vacuity probe 231.
- `mk test` clean, clippy clean.

## Next

**`$?` → `$(status)`**, the last and largest. The trap is unchanged and is the
whole risk: `validate::find_dollar_question` (2744) and `stmt_uses_status`
(2754) match `SimpleExpansion("?")` and nothing else. Their doc comment claims
to cover `$(status)`, which is true today only because
`bareword_shortcut_expansion` folds it into that node — and that fold is now
the **only** arm left in that function. Reverse the direction without moving
them and SB5012's availability state machine goes silently vacuous.

They must move in the same commit, with a neuter proving SB5012 still fires.
Confirm first what `$(status args…)` does: `nullary_magic_var` declines it on
`args.is_empty()`, but nobody has established what the non-empty form means.

After that, `intrinsic_int`'s `?` seed goes the way of `!` — by the same
structural argument, once the assertion covers it. `#` stays (function-local
`$#` is not sugar for anything).
