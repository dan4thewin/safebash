# 478 — the copy that declined differently

2026-10-08. Continuing the coverage queue at `ab405f8`. Eleven unexamined
candidates. One was a spec gap on an idiom everybody writes; three were
unreachable arms answering quietly; two looked like bugs and were not.

## The ranking did not move

Re-measured after DD#124: 98.29% lines, 444 uncovered. The DD#124 code was
fully covered by its own specs, so the queue came back in exactly the order it
went in. That is itself worth noting — a change that adds 23 specs and 224
lines of validator and moves the ranking not at all means the specs were
aimed at the code.

## The gap: `> f 2>&1`

`emit_one_redirect`'s guarded-dup branch, cold:

```rust
RedirectOp::DupOutput | RedirectOp::DupInput => match guard {
    Some(g) => {
        writeln!(e.out, "    if (!({g})) {{").unwrap();
        emit_dup_fd(e, fd, &r.target);
```

`guard` is set once an earlier redirect can fail, so `Some(g)` is "a dup after
an open" — which is `cmd > f 2>&1`. I did not believe it at first, because that
is the single most common redirect in shell. Compiling it says otherwise:

```c
i64 __t0_saved = sash_dup(1);
i64 __t2_saved = sash_dup(2);
if (!(__t1_rf)) {
sash_dup2(1, 2);
```

The branch is live and trivially reachable. What it had never been given was a
*spec*: `grep '2>&1' spec/*.sh` matches eleven files, and every single hit is
the harness's own bash redirecting `$SASH`, never a compiled program. The
suite's redirect block (`rd.1`-`rd.8`) covers opens, and `rd.8` covers a dup
*before* a failing open — the one ordering that leaves the dup unguarded.

rd.9 pins the idiom and rd.10 is its ordering control. Measured against bash
first, both orders byte-identical:

| | `$out` | file |
|---|---|---|
| `cmd > f 2>&1` | `after` | `out` / `err` |
| `cmd 2>&1 > f` | `err` / `after` | `out` |

The pair matters more than either half. rd.9 alone is satisfied by an
implementation that merges the streams whatever the order; rd.10 is what makes
it an assertion about ordering.

## The copy that had already drifted

`extract_func_args` had four cold lines, all of them declines:

```rust
let ExprKind::CommandSubstitution(stmts) = &value.kind else { return vec![] };
if stmts.len() != 1 { return vec![]; }
let inner = match &stmts[0].kind {
    StmtKind::Redirected(r) => &r.body.kind,
    other => other,
};
if let StmtKind::Command(c) = inner { c.args.clone() } else { vec![] }
```

That is a shape test. It is also, line for line, `single_command_subst`'s shape
test — the function that decided this value was a `FuncCall` in the first
place. Enumerating the producer (`handle_command_substitution` →
`single_command_subst` → `resolve_single_cmdsubst` → `func_call_capture`)
shows there is no other route to one, so all four declines are unreachable.

But the two copies **do not agree**, and the disagreement is the interesting
part. The canonical one has this:

```rust
// A redirected capture (`$(cmd > f)`) declines the fast path and falls to
// `CmdSubstBody` ... The fast path carried its redirects in the SSA node
// instead, which the C backend discarded outright and native dropped
// whenever the target was a literal path, so `$(/bin/echo hi > f)` captured
// `hi` and left `f` untouched.
if matches!(stmt.kind, StmtKind::Redirected(_)) {
    return None;
}
```

The copy *unwraps* `Redirected` and carries on. That is the pre-fix behaviour,
still sitting there, kept harmless only by the accident that the producer
declines first. This is the third time the "never fork a shared walker" rule
has had something to say in this repo, and the first time the fork was found
by its coldness rather than by a bug.

Replaced with a call to the canonical test. Four cold lines become one ICE,
and there is no longer a second answer to the question.

## The arm that stopped mutating

`lower_arith_mutation_structured` ended in:

```rust
_ => {
    // Should not reach here for valid code (SB5007 rejects bare expressions)
    Statement { kind: StmtKind::ArithTest(Box::new(expr.clone())), span }
}
```

Two things wrong with that comment. SB5007 is a *validator* rule and this is a
*parser* function, so it cannot be what protects it; and a bare expression
classifies as `Test`, so it never enters this function at all. The actual
guarantee is one line away: `lower_arith_command` routes only
`ArithKind::Mutation` here, and `classify_arith_expr` answers `Mutation` for
exactly `Assign`, `CompoundAssign`, `PostIncDec`, `PreIncDec`,
`SubscriptAssign` and `Comma` — every one of which the match already handles.

The body is the part that earns the change. `ArithTest` *evaluates its operand
and discards it*. Had a new mutating `ArithExpr` variant ever been added, the
arm would have accepted it and silently stopped mutating — no diagnostic, no
crash, a different program. Now total over all sixteen variants, so that
variant is a compile error here instead.

`parse_assignment`'s `SB4014 "expected variable name"` went the same way: it
reports on a head token that `looks_like_assignment` has already proved is a
`Tok::Word`.

## Two that looked like bugs and were not

`[[ $x == * { echo c; } ]]` prints nothing. For about ten minutes that was a
severe bug — a catch-all arm that never fires, with the no-match path
suppressed because `wire_no_match_path` sees a default arm and declines to
build one. Then `x='*'` printed `hit`. `*` is an **exact literal** under DD#66,
not a glob and not a catch-all; the `||` arm is the default. Nothing wrong.

`echo hi 2>&1 > /nope/f` reports on the invoker's stderr where bash reports
through the dup. That is DD#120, decided deliberately after DD#70 left it open:
a redirect written on a command is a channel for *that command's* output, and a
command that could not be set up produced none. Reading `notes/decisions/`
before chasing is what it is for.

## Method

Reachability was established by enumerating each arm's producer, and confirmed
negatively: a 14-program corpus aimed squarely at these arms, through
`cov-batch`, warmed **none** of them. A prober that reports nothing is
indistinguishable from a broken prober, so I checked that its lcov was real —
2202 warm lines in `validate.rs` from five programs. It works; the arms are
unreachable.

That check paid for itself immediately. `is_taint_source`'s `AssocArray` arm is
cold, and `map m=([k]=$(getenv HOME))` is correctly rejected SB5021 — the
obvious conclusion is that the probe warmed it. It did not. Collection-literal
taint is decided by a different walker entirely, and those arms are totality
arms, category 3, leave them alone. Without the prober I would have filed a
spec that proved nothing.

## State

`mk test` 81+27+20, up from 78 — three ICE pins, each asserting that the panic
message names the invariant that guarantees it, so a test that stops panicking
is a finding rather than a cleanup.

Sweep 7801, predicted before running (7797 + 2 specs × 2 modes). 0 failing,
0 flips, exactly 4 new ids, gate self-test 248.

Coverage after: **98.33% lines** (499 missed of 29859), 98.78% functions,
92.19% branches. `coverage.db` 444 → 429 uncovered lines, current at `39d726f`.
`emit_c.rs:3620-3624` is warm; `extract_func_args`, `parse_assignment` and
`lower_arith_mutation_structured` have **no** cold lines left at all — the
union folds in the unit tests, so a category-4 arm pinned by one stops being
cold. That is the queue's intended end state for an ICE, and it is the first
time I have watched it happen.

## Left on the queue

- `emit_one_redirect`'s own ICE arm (`other => panic!`) — category 4, no pin yet.
- `lower_value` (5) and `lower_value_as_str_aggregate` (6) — all ICE panics,
  same treatment, though `lower_value_as_str_aggregate` ends in `other =>`
  rather than naming its variants.
- `sysroot_file_exists` — the `.squashfs` branch is live code with no spec, but
  speccing it needs `unsquashfs` and a real image. An environment dependency,
  not a gap. Worth asking whether that path is still wanted.
- `insert_case_phis`'s single-path arm needs `items == 1 && is_default`, which
  is only the `||`-only dispatch that SB4068 rejects. Unreachable, and
  orthogonal (it holds SSA snapshots, not the parse tree), so it would qualify
  as a backstop — but its body is a *correct* answer rather than a wrong one,
  which is the opposite of the arith arm's case for converting.
- `ice_text` (4) is `#[cfg(test)]` and should probably be filtered out of the
  ranking rather than worked.
