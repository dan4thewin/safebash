title: "A `-` never prefixes a compound (`{`, `if`, `while`, `until`, `for`, `case`)"
status: open
priority: P3
kind: work
depends: [q-setup-failure-suppressibility]
gates:
  - "`dash_prefixes_command` (parser.rs) rejects a `-` before a compound, with a diagnostic that says to put the `-` on the command inside, in both backends"
  - "the six spec lines below are rewritten or re-pinned to the new diagnostic"
  - "DD#55 and DD#70 say a `-` prefixes one command"

DG, 2026-10-30 (`q-suppression-on-user-functions`, "Rulings, 2026-10-30"):
"suppression should never apply to a construct like for or while". This
rules that task's Q7, which has the details.

`dash_prefixes_command` (parser.rs:1644–1658) accepts a `-` before `{`,
`if`, `while`, `until`, `for`, `case`, `[[` and `((`. The `-` then covers
every command inside. `[[` and `((` are single commands, so they are not
in scope.

Six spec lines use the blanket:

| Id | Where | Form |
|---|---|---|
| bng.25 | errors.sh:827 | `- until` |
| csfx.17 | errors.sh:2788 | `- { … } 2>…`, an SB4021 check |
| pf.47, pf.48 | pipe_functions.sh:337, :344 | `\| - while readline x { … }` |
| p518.22, p518.23 | pipe_functions.sh:543, :547 | `- { a \| b }` |

## Decision dependency resolved (2026-11-06)

p518.22/.23 discard a pipeline's construction failure (fork, pipe), and a
group was the only suppression spelling for that whole construction.
A `-` at a pipeline's head binds to the first stage alone.

DD130 now settles the governing policy: recoverable failures can be
handled; fatal faults cannot. Fork failure is explicitly fatal, so no
replacement discard spelling is needed for it. Re-pin the old compound
spellings to the rejection, rather than preserve a way to suppress fatal
construction faults.

The dependency above was on the decision and is now satisfied; it does not
silently become a dependency on every failure-class implementation task.
Any actual recoverable-setup handling gap found during implementation
belongs with `runtime-failure-classification` and its consumers, not a
reopening of DG's ban on compound suppression.
