# 269 — Reviewing the native backlog, one task at a time

**DG: "let's review each one at a time, finalize any decisions, normalize
status, and queue for the next implementation batch."**

Nineteen open native-only tasks. The instruction was explicit about method —
*each one at a time* — and that turned out to matter more than it sounds,
because the thing that kept happening was not "decide yes or no" but "the
filing is wrong."

## The method

For each task: re-measure against the **live release binary** and the current
indexnudge index, not against the filing. Then decide, normalize status, and
assign a batch.

That is a slower pass than reading nineteen files and sorting them. It was
also the entire value of the exercise. Tally of what re-measurement changed:

| task | filed as | measured |
|---|---|---|
| `refactor-dd93-rename-complexity` | 7 violators | **4 self-cleared**, 3 left, all cognitive-only |
| `dd75-args-model-polish` | 5 items | **4 already done**; item 5 was a hidden fork |
| `getenv-presence-handling` | open question | settled — and exposed a **P1 bug** |
| `dollar-lt-file-forks` | P3 optimization | **P2 bug** — it execs `/usr/bin/cat` |
| `attest-map-form-validation-only` | ready | **already done** (2026-09-10), docs-only remainder |
| `eliminate-ice-arms-via-types` | 23 sites | **16** |
| `audit-string-keyed-builtin-dispatch` | 36 sites | **153** |
| `r47-validate-rs-per-rule-tests` | validate.rs = 1087 lines | **7222** (6.6×) |
| `remove-dead-case-first-runtime` | zero front-end refs | one missed ref in `c_tests.rs` |

Nine of nineteen were materially misdescribed. Two dissolved entirely. The
common failure is not carelessness at filing time — it is that a filing is a
**snapshot of a moving tree**, and nothing ages it. `validate.rs` grew 6.6×
while a task sat open describing how big it was.

## Four bugs found by reviewing a backlog

None of these were the task being reviewed. All four came from probing the
binary to check a filing's claim.

1. **`getenv`/`status` are unvalidated** (P1). They are the *only two*
   intrinsics with no `type_check.rs` entry — which is where arity is
   enforced — while also being exempt from SB5011. So `$(getenv)` and
   `$(status X)` compile, and the binary dies on
   `sash: bug: bare command passed to sash_exec`. Two holes in two different
   gates lining up exactly.

2. **`$(<file)` forks** (P3→P2). It does not merely fork: `parse_slurp_form`
   **desugars it to `$(cat file)` at parse time**. The linked static binary
   embeds `/usr/bin/cat` and makes two execve calls. `sash_read_all` exists,
   is wired, and is never called. Filed with a deliberate taint gate — the
   slurped value is tainted today only as a side effect of being an external
   capture, so a direct-read fix has to re-establish that explicitly or it
   punches a hole in the taint model.

3. **`sash-convert` conflates hashes and arrays** (P2, found earlier in the
   pass).

4. **`[[ -N f ]]` disagrees with bash** (P2). Found by sweeping all 18
   file-test operators × 3 target types against bash. Exactly one diverged.
   `sash-rt` uses strict `>` with a nanosecond tiebreak; bash uses `>=` on
   whole seconds. A freshly written, never-read file has mtime **exactly**
   equal to atime — same second, same nanosecond — which is precisely the
   case `-N` exists to report true. sash says no. No diagnostic; it compiles
   clean and answers wrong.

That last one is worth dwelling on. It is the **accepted-but-divergent**
class, which `fuzz-validator` has argued for months is the real threat and
the one the adversarial audit's threat model misses. It was found by hand,
by exactly the differential-vs-bash method that task proposes, applied to one
operator family for about ninety seconds. A crash-only or taint-only fuzzer
would not have flagged it. The task has been sitting `open` with nothing
built; this is the strongest argument for it yet, and it is now recorded in
the filing.

## Decisions DG made during the pass

- **The complexity ratchet holds at 3× indefinitely.** I brought the
  tightening plan ("3× → 2× → 1.5×") back with numbers: the cohort is 80
  violators at 3×, **357 at 2×, 518 at 1.5×**. The plan does not survive
  contact. DG: *"close, hold at 3x."* Grandfather-on-modify is the mechanism
  working as designed, not a backlog to burn down. AGENTS.md now says so.

- **No reserved builtin names.** The `shift` diagnostic item turned into a
  fork once I found that adding a name to the parser's removed-builtin table
  *reserves* it — `shift(){...}` works today, `select(){...}` is rejected at
  the call site. DG: *"drop the removed-builtin table altogether. error just
  as for any other unknown token in cmd position."* Checking what the table
  actually protected found three separate "nothing is lost" proofs:
  `source`/`.` keep their rejection via `check_dynexec`'s SB4001 (the table's
  SB4002 is redundant), the `export` arm in `validate_declaration` is
  **already dead** (nothing constructs `DeclKeyword::Export`), and
  `lexer.rs:1706–1711` are pure no-ops identical to the `_` fallback.

- **`--no-taint` is documented; `--no-taint-cst` is a testing aid** —
  accepted but unadvertised.

- **getenv presence is a lookup status, not a default argument.** DG's
  framing is the part worth keeping: *"the intrinsic isn't failing, but the
  lookup of a particular env var fails."* That dissolves the SB4025
  objection that intrinsics cannot fail — a not-found result **is** a result,
  not a malfunction. Shape is `home=$(getenv HOME) || home=/home/dg`.

  DD#42 says *"There is no silent empty return"* for `argv`. Applying that
  consistently forces a consequence I recorded rather than deferred: an
  uncaught `$(getenv NOPE)` must **abort**, which is a breaking change.

## On the CLI task

DG said "add fixing the usage message to the batch." Looking at it found
three defects, not one: two contradictory usage strings advertising different
flag subsets, no `--help` at all (it falls through to the error branch —
stderr, exit 1), and **unknown `--` flags silently ignored**. That last is the
real one:

    $ sash --emit-C h.sash     # capital C
    $ sash --jannet h.sash     # typo

Both produce a native ELF, exit 0, no warning. You asked for one artifact and
got another. `apply_and_check_flags` already rejects DD#57 converter flags
with exit 1, so the precedent to extend is right there in the same function.

The man page `docs/sash.1` is complete and correct — the binary is what
disagrees with it. And `sash.1` already lists the driver's silent-ignore in
its own table of *inconsistencies*, beside `read` and `sash-convert`.

Recorded in the filing: `parse_cli_args` is cc=11/**cog=10** against a gate of
12.46/6.58, so this task **will** trip the complexity gate. Better to know
before starting than to discover it at commit time and reach for
`SKIP_COMPLEXITY_GATE=1`.

## Measurement gotchas worth not re-learning

- **`$?` after a pipeline is the last element's status.** `cmd | head -1;
  echo $?` reports `head`'s rc. This produced two wrong readings that had to
  be re-measured before filing.
- An exact-token grep for `"--sysroot"` **misses it** — it is matched by
  prefix (`--sysroot=`). Flag inventories need prefix-aware searching.
- `@{}` is not an empty-map literal; it yields a confusing SB5033
  **recursion** error. Maps are built from arrays: `map m = @{arr:DISP}`.
- Taint (SB5020) blocks most probes. `attest v [[ $v != IMPOSSIBLE ]]` gets
  you to runtime behaviour.

## Where it landed

25 ledger rows (19 original + 5 filed during the pass + 1 bug found at the
end), all reviewed. **13 queued to `next`** — 5 of them bugs. 5 closed
outright. 5 re-scoped and left unqueued pending honest re-estimation. 1
deferred, 1 gated on compiler changes.

Two tasks closed because they were already done and nobody had noticed.
