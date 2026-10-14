title: "CLOSED: per-rule integration tests for validate.rs (goal met by spec/, artefact retired)"
status: closed
closed: 2026-10-14
priority: P7
kind: work
source: opus-47-review
depends: []
gates:
  - "WITHDRAWN — every gate below names sash/tests/validate/, a surface deleted"
  - "in journal 304 because its harness was fail-open. The task's GOAL (every"
  - "rule asserted somewhere) was met by spec/: 92 of 93 codes, and the 93rd"
  - "(SB5027) was closed in 55b9eb6. See the 2026-10-13 re-measurement below."

`validate.rs` is 1087 lines of safety-critical code. It absorbed
~1600 lines of CST-based rules plus 975 lines of taint analysis. The
postmortem (addendum #4) already made this point:

> If the tests are sufficient to validate rule behavior, the rewrite
> is safe — tests catch regressions. If the tests are insufficient,
> the two-parser state is ALSO unsafe.

Today the rules are exercised indirectly via `spec/*.sh`,
`brush_compat_tests`, and a handful of `taint_spec_tests`. There is no
per-rule test file.

## Scope

For each SB code currently emitted by `validate.rs`, create
`sash/tests/validate/sb{NNNN}.rs` (or a single file with one `#[test]`
per code). Each test:

1. Compiles a snippet expected to **pass** validation cleanly.
2. Compiles a snippet expected to **fail** with the specific SB code.
3. Optionally: a snippet expected to fail with a *different* SB code,
   confirming the rule scope.

## SB codes that live in validate.rs today

(from a grep of `validate.rs`):
SB5001, SB5002 (heredoc edge), SB4001, SB5006, SB5007, SB5009, SB5011,
SB5012/5013/5014, SB5017, SB5018, SB5019, SB5020 (taint), SB5023.
Plus the SB4xxx codes the parser emits but that validate.rs may also
gate.

## Why P7 (medium effort, no decision needed)

This unblocks aggressive refactoring of `validate.rs` itself (current
max CC inside it is `validate_stmt_ctx` at 43, fine, but the next
restructure will be safer with per-rule tests).

Also lets the next adversarial audit aim at specific rules with
known coverage.

## Out of scope

- Re-implementing any rule.
- Re-architecting the file. Tests first; structure later.

## Re-measured 2026-09-12 — the filing's sizing is off by 6.6x

`validate.rs` is **7222 lines**, not the 1087 the filing states. The scope
described ("one test file per SB code") is therefore far larger than the
estimate it was filed under. `sash/tests/validate/` still does not exist.

Re-derive the SB-code inventory and re-estimate before scheduling; the listed
codes (SB5001, SB4001, SB5006, …) were enumerated against a file less than a
sixth of the current size.

## Re-measured 2026-10-13 — obsolete in its prescribed form; its GOAL is 92/93 met

The 2026-09-12 note above asked for a re-estimate before scheduling and nobody
ever did one, so the task sat out of the queue for a month on its own gate.
Discharged now. Three findings, and they point at closing it rather than
sizing it.

**1. The prescribed artefact is a retired surface.** Every gate on this task
names `sash/tests/validate/sb{NNNN}.rs`. `sash/tests/` was deleted wholesale in
the 2026-09-14 test redux (journal 304) after its harness was found fail-open —
it called `codegen::compile_to_object` directly, so `validate` never ran, and
31% of its passing tests compiled programs the real compiler rejects. A suite
of per-rule *validator* tests is precisely the thing that harness could not
have tested. AGENTS.md now directs all new end-to-end coverage to `spec/`.

**2. The sizing is wrong again.** `validate.rs` is **11745** lines (filed at
1087; re-measured once at 7222). It raises **93** distinct SB codes; the filing
enumerates about fourteen. Nothing about the 2026-09-12 figure survived either.

**3. The goal was already met while the task sat blocked.** The point of
"one test per rule" is that every rule is asserted somewhere. Measured:

```sh
comm -23 <(grep -o '"SB[0-9]\{4\}"' sash/src/validate.rs | tr -d '"' | sort -u) \
         <(grep -ho 'SB[0-9]\{4\}' spec/*.sh | sort -u)
```

**92 of 93** codes are asserted by the spec suite. The one exception was
**SB5027**, and it was a genuine hole of exactly the shape this task was filed
to prevent: the line is *warm* (6 hits in `coverage.db`), three specs
(`av.m1`-`av.m3`) run programs that raise it — and all three asserted only
`-n $err`, which the co-fired SB5020 satisfies on its own. Deleting the rule
outright would have left the suite green.

Closed by tightening those three specs to name both codes. **Proved by
reversion**: stubbing `check_for_header_splat`'s condition to `false` turns
exactly three specs red, where before the change it turned none.

## Recommendation

**Close.** The artefact is retired, the estimate has never once been accurate,
and the goal is met. The residual worth keeping is not a Rust suite but the
one-line `comm` above — a code is only covered if a spec *names* it, and this
task's real finding is that "some program reaches the rule" is not coverage.

## Closed 2026-10-14 by DG

DG: "close/refile as recommended". Closed, not completed — the prescribed
artefact was never built and should not be. The durable residue is the census
in AGENTS.md ("a warm line is not coverage"), which does this task's job in one
line and keeps doing it.
