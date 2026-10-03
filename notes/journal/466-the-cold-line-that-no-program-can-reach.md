# 466 — the cold line that no program can reach

DG, setting the policy for the coverage queue:

> For any given function, I want every line warm by spec coverage except
> ICEs first. Then I want a unit test to cover the final ICE lines.

That is an order of operations, and it turns the queue from "what is cold"
into "what is cold *and reachable*". Journal 465 had already split the
cohort once — 64 ICE arms, 454 other lines. This session ran the second
split, and it is the one that matters: of the non-ICE lines, a large share
cannot be warmed by **any** spec, because `validate` rejects every program
that would reach them.

## The measurement loop

`cov-probe.sh` answers "does this snippet reach that line" for one line. The
inner loop of the queue asks the inverse, in bulk: thirty candidate spellings
aimed at forty cold lines — which landed? Probing one line at a time costs an
lcov render per line and re-runs the whole corpus each time.

`contrib/cov-batch.sh` (new) runs a corpus once, renders one lcov, and reports
every line that is cold in `coverage.db` and warm now. Six rounds of twenty
programs took the time one round of per-line probing would have. It also has a
`--spec` mode, which is the end-to-end check that closes the loop: the snippet
that warmed the line and the spec you actually wrote are not the same program,
and only the second one counts.

Two things cost me a cycle each and are worth writing down. The lcov records
absolute `SF:` paths under the **real** root (`/mnt/dg/src/safebash`), not the
symlinked one, so a `$PWD`-prefix strip silently matched nothing and reported
a clean zero — which reads exactly like "your corpus missed". And `sqlite3`
only honours `.import`/`.mode` on their own **input** lines, so the script has
to pipe its SQL rather than pass it as an argument.

## What warmed: 14 lines, 13 specs

Confirmed by execution, each from the spec as written:

| line | surface |
|---|---|
| `validate.rs:1520` | `for k in $(cmd 2>/dev/null)` — the for-source rule seeing through `Redirected` |
| `validate.rs:2302` | `[[ -n '' ]]` — the **single**-quoted empty word is a different IR node from `""` |
| `validate.rs:2337` | the always-FALSE half of the vacuous-test answer (`-n` on empty, `-z` on non-empty) |
| `validate.rs:3314` | a predicate whose body is a `{ }` group ending in `return` |
| `validate.rs:3315` | …and through a redirect on the `return` |
| `validate.rs:10026` | `read` arity — the two-arity shape of SB5057, never rendered |
| `statements.rs:346/358/371` | a redirect with no inst to attach to (`return` lowers to a terminator) |
| `statements.rs:449-453` | a mutation-free `?:` **beside** a hoisted arith mutation |

The last one is the nicest. 31.36 already rejected a mutation *under* `?:`;
the ternary arm of the hoist recursion is only ever reached by its legal
neighbour — an expression that contains a mutation somewhere **else**, so the
whole tree is walked, and a ternary that must come out unchanged. 31.139 pins
the ordering that makes it a real test rather than a shape: the ternary's
condition reads the variable the hoisted store just wrote, so a hoist that
moved the store after the ternary would take the other arm, silently, because
the value is discarded.

## What cannot warm, and why

Six candidate spellings in five of these functions were rejected by a
diagnostic I was not aiming at. That is the finding. Verified by execution:

- **`detect_array_iter`** (5 lines). The `DoubleQuoted` arm needs a quoted
  splat — SB5026. The `Redirected`-inside-`$( )` arm needs `$(keys m 2>…)` —
  SB5073. The three `return None` arms need a `$( )` for-source that is not a
  collection intrinsic — SB5009. And `$(argv N)`, the one spelling that would
  reach `cmd_name != "keys"`, is dispatched before this function is called.
- **`lower_one_pipeline_stage:912-916`** — a compound pipeline stage. SB4022.
- **`build_function_pipeline_stage:944-948`** — recursion into a function used
  as a pipeline stage. SB5037, including the mutual-recursion spelling.
- **`attest_arith_near_miss` 2588/2594/2598** — the first is the
  classify-succeeded path, which the caller never takes; the second needs
  `attest $x (( … ))`, which the parser rejects (SB4014); the third is already
  commented *"only reachable from a hand-built IR"*.
- **`type_check.rs:2050`** — `attest_scalar_form_error`'s map arm. Every
  predicate that would reach it is SB5064 (scalar sigil on a collection) or
  SB5020 (condition doesn't reference the variable).

There is a third class beneath these: the walker arms that exist because the
match is required to be **total with no wildcard** — `is_taint_source`'s
`AssocArray` and `Test`, `expr_uses_tainted`'s `AssocArray` and `SlurpFile`,
`first_expansion`'s pair. Those arms are deliberate; the convention that
forbids a wildcard is what has caught three bugs in three sessions. They are
not reachable and must not be deleted.

So a cold non-ICE line is one of four things, and only the first is work:

1. a genuine spec gap — write the spec;
2. a **validate-shadowed** arm in a later pass, for a form the first gate
   already rejects;
3. a **totality** arm, present because the no-wildcard convention requires it;
4. an ICE arm — DG's rule says these get a unit test, last.

(2) is the one worth a decision, and it is DG's: an arm that silently returns
`None`/`{}` for a form that cannot occur is exactly the shape that hid the
last three bugs — "the allowance that could never be taken" (432), "the guard
that was keyed on syntax" (440), "the twin that was never made total" (430).
Either it becomes an ICE (the invariant stated, and then unit-tested under
DG's rule), or it is deleted, or it is annotated as deliberate. I have not
changed any of them.

## Gates

Sweep **7543** (3874 native, 3656 c + 13 skipped), zero failures, predicted
exactly before running. `mk test` rc=0. All four spec-integrity checks clean
(only the documented `coverage.sh` variable-id mismatch).
