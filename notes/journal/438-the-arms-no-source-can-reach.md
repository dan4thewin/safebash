# 438 — The arms no source text can reach

Three commits had renumbered `validate.rs`, `ssa.rs` and `ssa_taint.rs`, so
`coverage.db` was invalid by construction — every row is keyed by `(file,
line)` and records no commit. Rebuilt it: `mk cov-union`, `mk cov-db`,
`mk cov-reset`.

The union now reads **regions 97.24% · functions 98.77% · lines 97.95% ·
branches 91.12%**, 554 cold lines.

## Only two functions in `sash/src` are entirely cold, and both are taint checks

That is a good headline and a misleading one. Ranking cold lines by function
put `ssa_taint::check_assoc_use` (9 lines, all cold) at the top and
`validate::test_uses_tainted` (7 lines, all cold) second. A taint check that
never executes is the strongest bug hypothesis the queue can offer, so I
chased both.

Neither is a bug. Both are **unreachable by construction**, and the reasons
are worth writing down because the ranking will surface them again.

### `test_uses_tainted` — a `Test` is never a collection value

`expr_uses_tainted` must stay total over `ExprKind`: a `_ => {}` there leaked
three times, once as a taint bypass that shipped. So it has a `Test` arm, and
that arm calls `test_uses_tainted`. But its caller asks "does this value
*flowing into a collection* read a tainted name", and no accepted program puts
a test there:

```sh
array a=( [[ -n $t ]] )       # SB4014 at parse — expected expression
array a=( $([[ -n $t ]]) )    # a CommandSubstitution; SB5020, a different path
```

The arm is right and unreachable. `[[ ]]` operands genuinely *are* read
positions — `check_test_taint`, this function's diagnostic-emitting twin, is
what enforces that — so the answer has to be correct the first time some
future form does reach it.

### `check_assoc_use` and four siblings — `Assign` reads nothing

`SsaInst::taint_reads` deliberately returns no reads for `Assign`: a value
there *propagates* into its target, where `classify_inst_def` owns it. And
five `SsaValue` shapes are built **only** as an `Assign` value —
`ArgvIndex` and `TableValue`/`TableKey` by the for-loop binding lowerings,
`AssocArray` by a map literal, `ArithExpr` by a hoisted `$(( … ))`. So none of
them can arrive at `check_value_use`, which is where `check_assoc_use` is
called from.

Measured rather than assumed: `/bin/echo $(( x + 1 ))`, the test-operand
spelling and the redirect-target spelling all leave the `ArithExpr` arm at
count 0.

### The redirect-target arms — spilled before SSA exists

`collect_expr_var_name`'s `ParameterExpansion` and `Subscript` arms were cold
too, and here the IR actively contradicted the coverage: `--emit-ir` on
`/bin/echo hi > ${f:?no}` shows a `:kind :parameter-expansion` target, and
`walk_expr_tree` visits the root node. Yet one instrumented compile took the
`SimpleExpansion` arm exactly once and the others not at all.

`spill_scalar_redirect_targets` is why. Any `${…}` or `$a[i]` target — the
forms the ad-hoc redirect-target codegen cannot lower — is bound to a
synthetic `__redir_target_N` scalar and the target rewritten to a bare
`SimpleExpansion` of that name. The backstop only ever sees the post-spill
form.

Nothing is lost: the temp's own taint is derived from the spilled value by
`classify_inst_def`, so `> ${f:!?no}` on a tainted `f` still resolves through
`Trusted` to Clean and is allowed, while the un-vouched spelling is rejected.
Both measured.

## The two that *were* reachable

Two cold lines in the same file turned out to be genuine missing coverage,
not dead arms.

**`elide_taint`'s `taint_off` arm.** DD#91 `${t:!~}` — an elide that also
vouches for the read. Nothing in the suite had ever put one in an array
literal. It compiles and behaves correctly; it simply had no spec.

**`splat_taint`'s tainted-array return.** The array half of the splat
derivation — the half commit `a89a530` did *not* touch. That fix taught
`splat_taint` to consult the `:-WORD` default; the array's own taint was
already right, but nothing proved it, because every spec that tainted an array
was stopped by the front end before the backstop saw it.

Both reach the backstop through the `$( )` body idiom bkstp.1-8 established:
the front-end pass does not descend into a command-substitution body
(`bug-taint-escape-in-cmdsub-body`), so the SSA backstop is what decides.

`bkstp.9`-`bkstp.12`, as two pairs that differ by one character:

| id | program | outcome |
|----|---------|---------|
| 9  | `array a=( ${t:!~} )` in a body | accepted — `!` vouches |
| 10 | `array a=( ${t:~} )`  in a body | backstop — `:~` vouches for nothing |
| 11 | `array c=( @{b:-z} )` on tainted `b` | backstop — carried through the array |
| 12 | `array c=( @{b:!-z} )` on tainted `b` | accepted — `!` covers its own subject |

11 is the one that isolates the array read: its word is the clean literal `z`,
so `splat_taint` can only reach the right answer via the array itself. 12 is
the counterpart to `bkstp.7` — there `!` did *not* cover an unrelated word,
here it *does* cover the subject it names.

## The unreachable arms are pinned, not deleted

`transform/expressions.rs::every_expr_kind_renders` already set the precedent,
with the same reasoning written out: six arms measured cold across a full
sweep, kept because the match is exhaustive on purpose, and pinned by a unit
test instead of a spec. The mkfile says the same thing from the other
direction — the 100% target is the **union** of spec and unit reachability,
"which is what makes the should-never-run ICE guards measurable at all".

So five unit tests now pin what no source text can reach: the two
redirect-target forms (including the DD#91 `taint_off` skip), the five
Assign-only value shapes, `CmdSubstBody` answering nothing, and
`cmd_subst_taint` failing **closed** on a head it cannot resolve.

All five were proven able to fail. Three arms neutered at once —
`check_assoc_use` to a no-op, the `!pe.taint_off` guard dropped, and the
`And`/`Or` recursion reduced to its left operand — and exactly the three
corresponding tests failed.

## A tool that could not answer the question it was aimed at

`contrib/cov-probe.sh` reported `(no branch records at that line — wrong line,
or not instrumented)` for `ssa_taint.rs:399`. The line is real, the build was
instrumented, and the snippet did reach it.

A branch **body** — the arm of a `match`, the `then` of an `if` — carries no
`BRDA` record of its own; only the condition does. But the coverage queue is
ranked by cold **lines**, so aiming the probe at one and being told "no branch
records" reads exactly like "not instrumented", which is the one answer the
script's own canary exists to rule out.

Added a `DA` fallback: when a line has no branch record, report its line
count. `ssa_taint.rs:399` then answered `line count 1` immediately.

## Gates

Sweep **6740 passing, 13 skipped, zero failing** — 6753 total, predicted
before running (+8 = 4 specs × 2 modes). Zero status flips against the
previous sweep. `mk test` 67 passed (was 62). fmt, clippy, totality and
complexity gates clean.

One note for whoever reads the complexity means next: re-measured here they
are **cc 3.83 / cog 1.84** over 2006 functions, with 96 functions violating at
3×. AGENTS.md is right that these must never be quoted — an earlier figure in
this session's own notes was three times higher.
