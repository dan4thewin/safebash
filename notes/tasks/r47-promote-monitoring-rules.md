title: Promote monitoring-postmortem rules into AGENTS.md
status: ready
priority: P6
kind: work
source: opus-47-review
depends: []
gates:
  - AGENTS.md contains the five rules at section-1 level
  - notes/journal/053-monitoring-postmortem.md cross-references AGENTS.md

[journal 053](../journal/053-monitoring-postmortem) ends with five
rules derived from a half-session of unjustified `pgrep`/`tail`
monitoring of an autonomous batch. The rules are excellent and they
are *not* in AGENTS.md, which means the next agent doesn't inherit
them.

## The five rules (verbatim from journal 053)

1. **Autonomous processes get ONE confirmation check.** Launch → wait
   10s → confirm process exists → report to DG → stop.
2. **Don't monitor what you can't influence.** If the only action
   available is "kill it," and killing it isn't warranted, don't
   check.
3. **Don't intervene in running batches.** If a task fails, the runner
   handles it (preserves branch, moves to next task). Post-mortem, not
   mid-flight correction.
4. **Runner bugs get fixed between runs, not during.** Note the bug,
   let the batch finish (even with failures), then fix and rerun.
5. **Context is a finite resource.** ~35 wasted tool calls ≈ 15–20k
   tokens of noise.

## Action

Add a new section to AGENTS.md, near the top, titled "Autonomous
processes" or similar. Verbatim or lightly edited rules. Reference
journal 053 for the source.

## Why P6 (cheap, no decision needed)

This is a one-paragraph addition to AGENTS.md. Pure process; no code.
The cost of *not* doing it is that 4.7+ models repeat the same
pattern, because they read AGENTS.md but not every journal entry.

## Related

AGENTS.md already has a "Learnings from 2026-05-11 session" section.
Same pattern applies — but those learnings were promoted; these
weren't.
