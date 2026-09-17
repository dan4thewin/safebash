# 349 — a test in a pipeline compiled clean and died at run time

## What happened

DG asked a question that was not about code: *is there any tool or approach,
short of rereading everything, better than what we're doing?* Answering it
honestly produced a bug fix.

Two prototype detectors were built and measured against the pre-fix tree. The
second — a **sibling-cohort omission detector** — flagged three functions
across the whole compiler. Two were obvious false positives. The third was
`transform/expressions.rs::expr_to_value`, reported as omitting
`ExprKind::Test`.

I triaged that as a probable false positive. `ExprKind::Test` is constructed in
only two places, both of which produce *statements*, so value position looked
unreachable. That reasoning was wrong, and it was wrong in the way this project
has been wrong before: **I argued from the constructors instead of testing the
claim.** "Unreachable by construction" is exactly the claim journal 346 opened
with — and that one happened to be true, which is precisely why it is a
dangerous habit.

Testing it took one command:

```
$ [[ -n $x ]] | /bin/cat
sash: bug: bare command passed to sash_exec (expected absolute path)
[[: command failed (exit 127)
```

A clean compile that can only fail at run time.

## The bug

A bare `[[ … ]]` parses to a `Command` **named `"[["`** whose single argument
is the `Test`. Every ordinary context dispatches that name to test lowering, so
`[[ ]] && …`, `|| …`, `if`, and `- [[ ]]` all work. The pipeline path does not:
it lowers stages generically, so `build_argv` → `expr_to_value(Test)` → falls
through `_` → `handle_command_substitution` → `SsaValue::Expr(stringified)`.

The IR shows it surviving intact into codegen:

```clojure
{:kind :command :name {:kind :literal :value "[["}
 :args [{:kind :test :expr {:kind :unary :op "-n" ...}}]}
```

DD#23 resolution exempts `[[` from SB5011 (a bogus `z` *is* caught), so nothing
upstream objected either. The runtime's own assertion was the only thing that
noticed — it knew a compiler invariant had been violated.

## The audit found a cohort, not an instance

DG's call was *audit first, fix as a cohort* — and the audit paid. Testing every
statement form in pipeline position:

| form | before |
|---|---|
| `[[ … ]] \| cat` | compiles, runtime abort |
| `cat \| [[ … ]]` | compiles, runtime abort |
| `attest x [[ … ]] \| cat` | compiles, runtime abort |
| `(( n = 2 )) \| cat` | compiles, **silently loses the assignment** |
| `cat \| (( n = 2 ))` | compiles, aborts the producer with SIGPIPE |
| `if/for/match \| cat` | already rejected, SB4022 |

So **SB4022 already existed** — "a pipe on a compound command isn't allowed". The
gap was that `[[ ]]`, `attest` and `(( ))` were never classified as compound.
The `(( ))` case is the quietest of the five: it compiled, ran, exited 0, and
did nothing, because a stage is a forked child and the assignment dies with it.

Note the harness lied twice before it told the truth. The first audit reported
every case "rejected" — the unused prelude variables were tripping SB5050 and
masking the real result. A test harness that fails for the wrong reason reads
exactly like a pass.

## The fix

`reject_statement_form_pipeline_stage`, beside its sibling
`reject_pipeline_and_or_postfix` in `parse_trailing_pipeline`. It unwraps
`Suppressed`/`Negated`/`Redirected` (so `- [[ ]] | cat` cannot smuggle a test
through) and rejects `[[`, `attest`, `Assignment` and `ArithTest` stages under
the existing SB4022 code.

Deliberately narrow: **only the pipe is rejected.** `[[ ]] && echo` and
`[[ ]] || :` are legitimate status consumers and still work — pf.23 guards
against exactly that over-rejection. DG's ruling named validate; SB4022 is a
parse error, and reusing it keeps one user-facing message and one remedy
("wrap it in a function") for the whole compound family rather than splitting
the cohort across two error classes.

Eight specs, `pf.16`–`pf.23`, in `spec/pipe_functions.sh` — whose header
already documented *the identical runtime error* for functions as pipeline
stages. Same defect family, one file.

## What the detour actually established

Measured, not asserted:

- The **language-surface matrix** — which I ranked first, on speculation,
  without running it — is the worst of the three. 63% of its cells are empty,
  and the one cell that mattered was a **false positive** from a regex `\s`
  matching a newline. It would have missed 347 and reported green.
- The **omission detector** produces a 3-line report over the whole compiler
  and found this bug.
- Neither replaces rereading. Every bug in this stretch came from reading a
  caller. What coverage buys is **attention allocation**, and it is still the
  best allocator tested.

Filed as `notes/tasks/research-structural-defect-detectors.md`, with the
prototype's two failure modes recorded so the next attempt does not repeat
them: regex brace-counting silently produced a false 0, and the decisive
threshold (0.80 vs 0.85) is calibrated against a single labelled positive.

## Ledger

- `sash/src/parse/parser.rs` — `reject_statement_form_pipeline_stage` + call site
- `spec/pipe_functions.sh` — `plan=15 → 23`, specs `pf.16`–`pf.23`
- `notes/tasks/research-structural-defect-detectors.md` — new

Sweep **5354 passing / 13 skipped / 0 failing** (predicted 5338 + 2×8 = 5354).
Units 83.
