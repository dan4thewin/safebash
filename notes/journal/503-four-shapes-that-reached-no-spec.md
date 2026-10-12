# 503 — Four shapes that reached no spec

Continuing the cold-line queue. This entry is mostly about *method*: the
reasoning I did about which program reaches which arm was wrong three
times out of three, and the tool was right every time.

## The probe corpus disagreed with me, and it was correct

Three walkers in `validate.rs` each had a cold `ExprKind::AssocArray`
arm — `is_taint_source`, `expr_uses_tainted`, `check_value_cmdsub_reads`.
Those are the walkers that must agree (`reject_tainted_into_array` reads
the first two as a pair), so this looked like one shape warming three
lines.

I wrote the obvious programs: `map m=([k]=$(getenv HOME))`,
`x=([k]=$t)`, `array a=($(<$t))` and nine more. All twelve compile-fail
with the expected SB5021. `contrib/cov-batch.sh` reported **zero** cold
lines warmed.

That is the right answer, and the reason is in the caller:
`reject_tainted_into_array(e, …)` is handed each *element value*, not
the literal. By the time any of the three walkers runs, the `AssocArray`
node has already been taken apart. My twelve programs exercised the
SB5021 path thoroughly and could not have touched those arms.

**A green "nothing warmed" is only information if the tool can report a
positive**, so I checked that before believing it: two programs aimed at
`stmt_always_returns` (SB4037) and the report came back with both lines.
That cost one run and converted an ambiguous empty result into a finding.

## The four gaps

| line | arm | shape |
|---|---|---|
| `validate.rs:2536` | `test_references_var` → `TestExpr::Not` | `attest h [[ ! $h == zz ]]` |
| `validate.rs:3938` | `stmt_always_returns` → `Redirected` | `fn f { echo hi >/dev/null; }` as a predicate |
| `validate.rs:3940` | `stmt_always_returns` → catch-all | `fn f { while true; do :; done }` as a predicate |

All three are category 1: the compiler already answers correctly and no
spec said so. Each is silent when wrong — a `Not` arm that answered "no"
would reject a correct `attest`, one that answered "yes" would admit an
attest that validates nothing, and a `stmt_always_returns` arm that
answered "yes" would simply stop SB4037 firing. So each positive is
paired with a negative: `atop.5` is `atop.4` with the predicate naming a
different variable.

`atop.4`/`atop.5` in `spec/taint.sh` (337 → 339), `74.65`/`74.66` in
`spec/functions.sh` (182 → 184).

## Closing the loop

The snippet that warms a line and the spec that ships are different
programs, so `contrib/cov-batch.sh --spec` is the check that matters.
Run against the two spec files, it reported all three lines warm. That
is the artefact, not the snippet.

Sweep predicted at 8367 before running and got 4068 c + 13 skipped +
4286 native = **8367**. Zero failures, zero flips, exactly eight new ids,
liveness 258.

## Note on `AssocArray`

The three cold `AssocArray` arms are still cold and are now better
understood: no caller hands those walkers a map literal, because every
route into them decomposes it first. That makes them category 2/3 —
unreachable through the callers that exist — and the artefact is a unit
test calling each walker directly, which also pins that the pair still
agrees. Left for the next pass; it belongs with the other direct-call
tests rather than being bolted onto a spec commit.
