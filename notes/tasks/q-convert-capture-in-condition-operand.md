---
title: "Question: sash-convert copies `[[ … $(cmd) … ]]` verbatim, and since F7 a failing `cmd` aborts"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the options below; the ruling is recorded in the converter's DD (DD#57) or in this task if it changes nothing"
  - "if the converter changes, convert.sh specs pin the `if` and the `&&` forms, and the output of the `if` form compiles and runs as bash does under `--no-taint`"
---

## What

In bash, `set -e` does not apply inside a condition, so a failing command in
a test operand is harmless: `if [[ -n $(grep zzzq "$f") ]]` takes the else
branch. sash-convert copies the line unchanged. Since F7 (DD#55's addendum
of 2026-10-26, journal 536), a failed capture in a `[[ … ]]` operand aborts,
because the capture is an argument and the test is the condition.

Probe `conv/g.sh`, bash with `set -e`, converted and with the `set -e` line
removed (`g2.sash`):

```
#!/bin/bash
# MANUAL: set — shell options not configurable in sash
f=/etc/hostname
if [[ -n $(grep zzzq "$f") ]]; then echo found; else echo none; fi
[[ -z $(grep zzzq "$f") ]] && echo empty
echo done
```

| | bash | `fec83f7` (before F7) | F7 |
|---|---|---|---|
| default (taint on) | — | SB5020 at 4:10 and 5:7 | same |
| `--no-taint` | `none empty done` | `none empty done` | rc 1, `/usr/bin/grep: command failed (exit 1)` |

Both backends agree in every cell.

So the change is visible only under `--no-taint`. In the default mode
SB5020 already stops the program at the capture. The rewrite it forces,
`x=$(grep …)` then an `attest` then `[[ -n $x ]]` (`g3`), aborts on both
binaries unless the capture is the last command before a `[[ … ]]`
condition, where `fec83f7`'s scan unchecked it (B85; spec 55.99 pins the
fix). So the tolerance seen before F7 was B85, not a converter feature.
`$(- grep …)` keeps bash's meaning on both binaries, as an assignment
(`g4`) and inside the operand (`g7`).

The converter has a precedent: `add_true_false_hint` writes a `# CHECK:` line
where sash aborts and bash only sets `$?`. Its helper `status_is_consumed`
treats any `$(…)` as reading its command's status, which is not true of a
capture inside a `[[ … ]]` operand.

## Options

- **(a) a `# CHECK:` hint** on a capture in a condition's test operand
  (`if`, `elif`, `while`, `until`, `&&`/`||`), saying that sash aborts when
  the command fails and that `$(- …)` tolerates it. This follows the
  `false` precedent and changes no code meaning. Recommended.
- **(b) rewrite to `$(- …)`.** This keeps bash's meaning, but it silently
  suppresses every failure, including a missing binary, which DD#55 asks the
  author to state explicitly.
- **(c) leave it.** SB5020 already flags the site in the default mode, so
  only `--no-taint` users see the change.

Probes: `files/port-f7-probes.tar.gz`, directory `conv/` (g.sh, g2–g7).
