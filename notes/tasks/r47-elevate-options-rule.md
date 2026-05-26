title: Elevate "present options, don't narrow" to top of AGENTS.md
status: ready
priority: P8
kind: work
source: opus-47-review
depends: []
gates:
  - AGENTS.md edits committed
  - rule appears in the first screen of AGENTS.md (before line 60)

AGENTS.md contains the rule:

> Decision authority — DG tasks the AI with implementing and expects
> all important decisions to be discussed before acting — design,
> architecture, implementation strategy, or anything else that
> matters. The AI does not get to decide what's "just implementation"
> and skip the conversation. When in doubt, present options with
> tradeoffs.

This is currently several screens in. The recurring pattern in the
parser-migration postmortem and journal 053 is the AI *narrowing
options* and *justifying deferrals* before DG saw the choice. Five of
the postmortem addenda are DG correcting that pattern after the fact.

## Action

1. Move the "Decision authority" paragraph to immediately after
   "What this project is", so it appears in the first screen.
2. Add a short bullet to "First thing every session": "If the task
   involves a non-trivial decision, list options + tradeoffs before
   acting."
3. Cross-reference `notes/parser-migration-postmortem.md` (the
   six addenda are the canonical examples of what this rule prevents).

## Why P8 (cheap, no decision needed)

Pure documentation reordering. Independent of every other task.
The benefit is recurring: every future session starts with this rule
visible.

## Out of scope

- Removing or weakening the existing rule. This is purely about
  visibility / placement.
