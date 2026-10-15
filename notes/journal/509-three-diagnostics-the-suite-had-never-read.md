# 509 — Three diagnostics the suite had never read

2026-10-14 · follows 508

## A new way to aim the coverage queue

The cold-line queue has been ranked by function for a while now, which finds
clusters but not families. This session added a different axis: take every
place in `sash/src/` that raises an `SB` code, and intersect it with the cold
lines in `coverage.db`.

The appeal is that a raise site is *self-labelling*. A cold arm in a walker
needs a probe before you know whether it is a gap, a shadow, a totality arm or
an ICE; a cold `parse_error(.., "SB4014", ..)` announces up front that someone
thought a user could get here. Either they were right and no spec says so, or
they were wrong and the diagnostic is a lie about the language.

SQLite has no regex, so the extraction goes through `GLOB`:

```sql
SELECT file, line_num, content FROM lines
WHERE file LIKE 'sash/src/%'
  AND content GLOB '*SB[0-9][0-9][0-9][0-9]*'
  AND trim(content) NOT GLOB '//*';
```

Minus the test modules, the `.expect(`/`unreachable!(`/`assert` lines, and
then joined against `SELECT file||'|'||line FROM cov_lines WHERE count = 0`.

**277 raise sites, 148 distinct codes, 56 codes with more than one site.**
Three of the 277 were cold. That is a good hit rate for an axis this cheap —
and the three turned out to be one of each kind.

## One was a real gap

`expr.rs:1782`, `expect_rparen`, `SB4014 "expected ')'"`. It has exactly one
caller: `parse_slurp_form`, the `$(<file)` form. Nothing in the suite had ever
written a malformed one.

```
$ sash -c 'x=$(< a b)'
-c:1:3: error [SB4014] expected ')'
```

That is `sl.14` now, in `misc.sh` alongside the rest of the slurp family. The
assertion is on the **message text**, not the code — SB4014 is raised at ten
places in `parse/`, so `$err == *SB4014*` would pass if any of them fired. This
is the lesson from journal 475 applied before the fact rather than after it:
a code is not a site, and a spec that asserts only the code is cover for
nothing in particular. `"expected ')'"` is unique to this one.

## Two were lies about the language

Both are the shape DG ruled on in the `len @a` case: an unreachable arm
handling a rejected spelling is worse than either alternative, because it
documents the spelling as supported.

**`parser.rs:705`, `SB4027 "expected \`if\`"`.** `parse_brace_else` peeks for
`Tok::If` and then calls `parse_if`, converting a `None` into that diagnostic.
`parse_if` cannot return `Ok(None)`. Every arm of its trailing match is an
`Err` or delegates to `parse_if_brace` / `parse_if_keyword`, and both of those
end in `Ok(Some(make_if(..)))`; the only `None`s in the region are `else_body`
locals.

**`parser.rs:3384`, `SB4014 "expected redirect operator"`.** `parse_redirect`
returns early for `AmpGt` and `HereString`, then maps the token through
`redirect_op_for_token`, which covers exactly `{Gt, DblGt, Lt, GtAmp, LtAmp,
LtGt}` — and all three callers have already gated: `parse_command_element`'s
match arm lists the eight-token set literally, `fd_prefix_binds` gates
`parse_fd_redirect` on five of them plus adjacency, and `next_trailing_redirect`
calls `is_redirect_op` outright.

Both are now `unreachable!()`.

### Both pass the rival test, for the same reason

DG's rule is that an unreachable line earns its place only if it catches a
regression *orthogonally* — one that re-asks the local guard's own question on
the same data is a second copy of one assertion and no more trustworthy than
the original.

The mechanical check is what the ICE can see that its guard cannot. Here the
answer is the same both times, and it is not the usual one about lowering:
**the guard and the ICE are about different functions.** `matches!(peek, Tok::If)`
is a claim about the token stream; the ICE is a claim about `parse_if`'s
contract, which the peek structurally cannot check. `parse_redirect`'s is
stronger still — it asserts that *three separate callers* still gate, and no
one of those gates can say anything about the other two.

### Only one of them can be tested

`parse_redirect` is private, so the test goes in `parser.rs`'s existing
`invariant_tests` module, and it reaches the arm by deliberately breaking the
caller contract — `Scanner::new("notaredirect")`, straight in. That is what
makes the panic a claim rather than a prediction.

`parse_brace_else` cannot be reached at all. Making `parse_if` return
`Ok(None)` requires editing `parse_if`. This is the category AGENTS.md says
not to chase: document the invariant in the arm and stop.

## The signature change I did not make — and a correction

There is a tidier fix for `parser.rs:705`: `parse_if` returns
`Result<Option<Statement>, Diagnostic>` but never produces the `None`, so
narrowing it to `Result<Statement, Diagnostic>` would delete the cold line at
the type level — exactly the thesis of `eliminate-ice-arms-via-types`.

**The commit message gets the reason wrong, and this is the correction.** I
wrote that the 27 functions sharing that signature do so as a deliberate
"`None` = not my form" protocol that lets `dispatch_statement` try them in
turn. That is not what the code does. `dispatch_statement` is a *total* match
on the head token — every arm commits, `_ => parse_simple_or_assignment` is the
catch-all, and nothing backtracks. There is no try-in-turn protocol to break.

What the `None` actually means is narrower and duller: **"there is no statement
here."** It originates in exactly one place, `parse_statement:270`, whose doc
comment says so outright ("Returns None for empty/comment-only lines"), and
three functions relay it — `parse_negated:1272`, `parse_suppressed:1353`,
`parse_list_operand:3174/3181`, each handing on a `None` that traces back to
that EOF check.

Which means **23 of the 27 can never return `None`**. The `Option` is not a
protocol there, it is repetition, and every caller pays for a case that cannot
arise. `parse_brace_else` is simply the one place where somebody answered the
impossible case with a *user-facing diagnostic* instead of ignoring it, which
is why coverage found this one and not the other twenty-two.

So the fork is three-way, not two:

- **A — leave it** (what is committed). One `unreachable!()` in the one spot
  that has to name the case. Zero churn.
- **B — narrow `parse_if` alone.** Deletes the cold line, but makes one of 27
  signatures different from the rest and puts a `.map(Some)` at
  `dispatch_statement:298`. The worst of both: real churn, no systemic gain.
- **C — narrow all 23 that never return `None`.** `parse_statement` keeps the
  `Option` because it is the real producer, and `dispatch_statement` keeps it
  because it relays through `parse_negated`/`parse_suppressed`, so its arms
  become `parse_if(scanner).map(Some)` and so on. This removes the whole class
  of impossible-case handling rather than one instance of it, at the cost of
  touching 23 signatures and their call sites.

I took A because it is reversible and the other two are DG's call. My earlier
framing — "breaking a 27-function convention" — overstated B and C, since the
shared signature is not a convention carrying meaning. The honest objection to
C is only its size.

## Numbers

Sweep **8377** — 4073 c + 13 skipped + 4291 native. Predicted 8375 + 2 before
running it (one new id, dual-mode file), which is the only reason the total is
evidence of anything. Zero failures, zero flips, two new ids and both of them
`sl.14`. Liveness 258.

`mk test` 125 + 27 + 20 — the 125 is 124 plus the new ICE test.
