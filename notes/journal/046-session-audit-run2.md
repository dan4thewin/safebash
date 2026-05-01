# Session 2026-04-24 (cont.) — Adversarial Audit Run-2 Remediation

**Goal:** Fix all 12 findings from the adversarial audit of taint/bless/DD#46.

**Approach:** TDD — wrote 16 failing tests for audit findings, then
implemented fixes until all 43 taint spec tests pass, 502 total.

### Audit Pipeline

5 models (gpt-4.1, claude-sonnet-4, claude-sonnet-4.5, gpt-5.2,
gpt-5.3-codex). ~60 raw findings → 12 deduplicated. Two remediation
plans (gpt-5.3-codex, claude-sonnet-4.5), reconciled by gpt-5.2.
All artifacts in `audit/run-2/`.

### Fixes

**T1 — Herestring/heredoc false clean** (CRITICAL, 5/5 models)
`handle_read` now calls `has_clean_heredoc_or_herestring()` which
inspects redirect content for tainted expansions via
`subtree_has_tainted_expansion()`. Only marks target clean if
content is actually clean.

**T2 — Tautological bless** (CRITICAL, 5/5 models)
`handle_bless` now requires the test expression to contain `$VAR`
where VAR is the variable being blessed. New diagnostic: SB4012
"test expression does not reference $VAR — tautological bless".

**T3 — Bless without valid test** (HIGH, 3/5 models)
`handle_bless` now requires args to start with `[[`. New diagnostic:
SB4013 "bless expected [[ test expression".

**T4 — Arithmetic assignment taint laundering** (HIGH, 3/5 models)
`expr_is_tainted` now handles `arithmetic_expansion` nodes via
`arith_subtree_has_tainted_var()`. `y=$((x+1))` where x is tainted
now correctly marks y as tainted.

**T5 — Parameter expansion operand taint** (HIGH, 2/5 models)
Both `expr_is_tainted` and `check_expr_reads` now recurse into all
children of `expansion` nodes, catching `${safe:-$tainted}` patterns.

**T6 — Function global re-taint** (HIGH, 1/5 models)
`handle_function_def` now collects free variable reads via
`collect_free_reads()`. `handle_command` checks free-read globals
at call sites — if any captured global is tainted, emits SB4020.
New fields: `function_free_reads: HashMap<String, Vec<String>>`.

**T9 — Shift + positional** (MEDIUM, 3/5 models)
`collect_max_positional` now tracks cumulative `shift` count and
adds it to max positional reference. `f(){ shift; echo "$1"; }; f one`
now correctly computes required argc as 2 (1 + 1 shift).

**T10 — Direct $(getenv) in expression** (MEDIUM, 1/5 models)
`check_expr_reads` now handles `command_substitution` nodes —
if `expr_is_tainted` returns true, emits SB4020.

**T11 — Case pattern taint** (MEDIUM, 3/5 models)
`walk_statement` case handler now descends into `case_item` children
and calls `check_expr_reads` on pattern expressions.

**T8 — Nameref rejected** (regression lock)
New rule: `rules/nameref_rejected.rs` — SB5003 rejects `declare -n`,
`local -n`, `typeset -n` including combined flags.

**T12 — Process substitution rejected** (regression lock)
New rule: `rules/process_substitution.rs` — SB5004 rejects `<(...)` and
`>(...)` process substitution nodes.

### Design Decision #49: Tautological Bless Rejection

**Context:** `bless x [[ 1 == 1 ]]` always succeeds at runtime
without constraining `x`. All 5 audit models flagged this.

**Decision:** The test expression in `bless VAR [[ expr ]]` must
contain `$VAR`. If it doesn't, the compiler rejects with SB4012.
This prevents tautological tests and tests on wrong variables.

**Tradeoff:** This is a syntactic check, not semantic. A test like
`[[ $x == $x ]]` would pass the check despite being tautological.
A semantic check would require expression analysis. The syntactic
check catches the common case (wrong variable, literal tests) with
zero false positives on real code.

### Design Decision #50: Function Free-Read Tracking

**Context:** Functions analyzed at definition time could read globals
that become tainted before call time.

**Decision:** Track which globals each function body reads (free
variable reads). At call sites, check whether any of those globals
are currently tainted. This is lightweight inter-procedural analysis
— no function signatures, no call graph, just a set of variable
names per function.

DG directed architecture and design. AI implemented.

### Design Decision #51: SSA Taint Verification Pass

**Context:** The CST taint analyzer is the primary enforcement gate.
After the audit run-2 remediation, it catches all known bypass
patterns. But it operates on syntax, not on the representation that
gets compiled. Any future bug in the CST pass could let tainted data
reach codegen undetected.

**Decision:** Add `ssa_taint.rs` — a second taint pass that runs on
the SSA representation after transformation, before codegen. It
classifies each VarRef version as tainted or clean based on how it
was defined:
- `getenv`, `argv`, `read` (non-herestring), external cmdsub → tainted
- `getenv` of known env var (auto-bless) → clean
- Known-safe external commands → clean
- User-defined function capture → clean
- Literal, bless target → clean
- Phi → clean unless a classified incoming is tainted
- Arithmetic/concat/param-expansion → tainted if any operand is

If any tainted VarRef reaches a use site (command arg, pipeline arg,
subscript, export), the pass emits an ICE ("CST taint pass missed
this") and compilation fails. This is the airbag — it should never
fire. If it does, that's a bug in the CST pass.

**Implementation:**
- `Bless` changed from `{var, test}` to `{target, source, test}` —
  produces a new SSA version. `source` is tainted, `target` is clean.
  Codegen copies the value on the pass path.
- `verify_ssa_taint()` called in `main.rs` before `compile_to_object`.
- 264 lines, no design decisions — purely mechanical verification.
- Special variables (`?`, `!`, `$`, `#`, etc.) and positionals at
  version 0 pre-seeded as clean.

DG directed architecture and design. AI implemented.

---

