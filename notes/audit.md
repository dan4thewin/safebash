title: Audit

## Completed audits

**Run-1** (audit/run-1/) — First adversarial review. All findings remediated.

**Run-2** (audit/run-2/) — 5 models (gpt-4.1, claude-sonnet-4,
claude-sonnet-4.5, gpt-5.2, gpt-5.3-codex). 12 deduplicated findings
(T1-T12), all fixed. Key fixes:
- T1: Herestring of tainted data
- T2: Tautological bless → SB4012
- T3: Invalid bless syntax → SB4013
- T4: Arithmetic expansion taint propagation
- T5: Parameter expansion operand recursion
- T6: Function free-read tracking at call site
- T9: shift + positional range detection

## Brush test suite

622 total cases, ~300 NA (features sash rejects by design).
Last run (2026-04-25): 160 PASS, 59 FAIL_OUTPUT, 75 FAIL_COMPILE.
Effective pass rate ~67%. Detailed checklist: `audit/run-2/BRUSH_CHECKLIST.md`.

Top output failure categories:
- Array per-element substitution (5 cases)
- Parameter transformations (7 cases, now rejected by DD#52)
- Compound command redirects (3 cases)
- Arithmetic side effects (3 cases, intentional per DD#31)
