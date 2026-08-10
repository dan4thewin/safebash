---
title: "sash-convert: emit a CHECK comment on bash `true`/`false`"
status: ready
priority: P3
kind: feature
depends: []
gates:
  - "bash `true` / `false` in converted output gets a `# CHECK:` hint noting sash resolves them to /usr/bin/true|false (externalized), so a bare `false` aborts (DD#55) and `true`/`false` are no longer non-forking builtins"
  - "the hint fires on statement-position `true`/`false` (conditions, bodies, bare); does not fire inside strings/comments"
  - "0 converter-spec regressions"
---

# sash-convert CHECK hint for `true` / `false`

## Why

DD#69 addendum (2026-07-30) externalized `true`/`false`: they resolve to
`/usr/bin/true`/`/usr/bin/false` (DD#23), not inline non-forking builtins.
Behavior a bash author may not expect:

- a **bare** `false` (status discarded) **aborts** the program (DD#55),
  where bash just sets `$?`;
- `true`/`false` now **fork/exec** (in a pipeline stage, etc.), not a
  no-op status set;
- an internal always-fail is `! :` or a `return 1` function, not `false`.

So converted code that uses `true`/`false` should carry a `# CHECK:` hint
directing the reader to confirm the externalized semantics still match
intent (e.g. a `while true` busy-loop, a `false` used as a status stub).

## How

Follow the existing SC2086 CHECK-hint path in `sash-convert/src/convert.rs`
(`prefix_lines.push((line, "# CHECK: …"))`, ~line 918). Detect
statement-position `true`/`false` command words (not inside strings) and
attach a one-line hint. Suggested text:

`# CHECK: sash runs true/false as /usr/bin/true|false (externalized). A bare false aborts (DD#55); true/false are not non-forking builtins.`

Add converter specs mirroring the SC2086 hint specs.
