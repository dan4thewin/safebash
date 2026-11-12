title: Build a "delegate management" skill from monitoring-postmortem rules
status: pending
priority: P3
kind: process
source: https://github.com/dan4thewin/safebash/blob/f02477dd31e67a0fe6fe3c4a359935569c4715d8/notes/opus-47-review.md
depends: []
redirected: 2026-06-07
gates:
  - skill artifact (script, prompt, or harness) exists and is tested on at least one autonomous batch
  - experiments documented in notes/journal/ showing what the agent did vs. what the skill prescribes
  - cross-reference from journal 053 to the new skill

## Redirection (2026-06-07)

Original framing was "add a paragraph to AGENTS.md citing the five rules
from journal 053." DG flagged this as a fumble: process rules embedded in
a wall of prose don't generalize, don't survive context compaction, and
don't catch the agent in the moment it would violate them. The pattern
this task is really about — *how a manager interacts with autonomous
delegates* — needs to be worked up as a **skill**, not as documentation.

A skill in this sense is the combination of:
- A reusable artifact (script, prompt template, or check harness)
- Experiments that show the artifact catching the failure mode it targets
- A short rubric the agent can apply in the moment ("am I about to
  monitor a process I can't influence?") rather than "did I read the AGENTS.md
  section about monitoring?"

## The original five rules (still the source material)

Verbatim from journal 053:

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

## Approach

1. Run a small experiment: launch a bounded autonomous batch and let an
   agent (with no prompt modification) handle the "process is running"
   state. Capture the tool-call trace. Select the runner when undertaking
   this task; the former root Goose runners were retired in journal 620.
2. Identify which of the five rules the agent violates and which it
   honors naturally.
3. Design an intervention — a pre-flight check, a one-shot
   confirmation pattern, or a "delegate mode" wrapper — that targets
   the violations specifically.
4. Re-run with the intervention. Measure: did the wasted-tool-call
   count drop? Did the agent still detect real problems?
5. If the intervention works, codify it as a reusable skill (a script
   under `contrib/`, a prompt fragment, or a check the runner enforces
   pre-launch).

## Why not just edit AGENTS.md

AGENTS.md is already large. Adding rules there assumes agents will
internalize them mid-task. The evidence from journal 053 is that they
don't — the rules describe a failure pattern, but reading them doesn't
prevent the pattern. The skill formulation forces a tested intervention
rather than a hopeful documentation entry.

## Related

- Journal 053 (the source rules)
- Journal 620 (retirement of the old Goose runners)
- Future skill could live in `contrib/` or be packaged as a Copilot CLI
  skill artifact
