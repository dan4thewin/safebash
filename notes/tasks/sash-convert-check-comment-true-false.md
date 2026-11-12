title: "sash-convert: emit a CHECK comment on bash `true`/`false`"
status: done
priority: P3
kind: feature
depends: []
gates:
  - "bash `true` / `false` in converted output gets a `# CHECK:` hint noting sash resolves them to /usr/bin/true|false (externalized), so a bare `false` aborts (DD#55) and `true`/`false` are no longer non-forking builtins"
  - "the hint fires on statement-position `true`/`false` (conditions, bodies, bare); does not fire inside strings/comments"
  - "0 converter-spec regressions"

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


---

## Outcome (2026-09-14)

Done. **The filing was accurate** — every claim re-measured against the live
binary and held, which after three consecutive rotted filings was worth
confirming rather than assuming:

- bare `false` aborts: `/tmp/t.bin: /usr/bin/false: command failed (exit 1)`,
  and the statement after it never runs;
- `false || echo fallback` does **not** abort (status consumed), exit 0;
- `while true` forks per iteration — timed at **11.1 s for 20,000 passes**,
  ~0.55 ms each, against a bash builtin that is free.

## One deviation from the gate: two messages, not one

The gate asked for "a `# CHECK:` hint" on statement-position `true`/`false`.
Implemented as **two** hints, because the two hazards are not the same size
and a single undifferentiated warning dilutes the serious one:

| position | message |
|---|---|
| `false`, status discarded | **ABORTS the program** (DD#55); use `! :` or a function returning 1 |
| everything else | status matches bash, but each evaluation forks and execs |

`status_is_consumed` walks out through `list` (`&&`/`||`), `negated_command`,
`command_substitution`, `pipeline`, and the *condition field* of
`if`/`while`/`until`/`elif` — a command in a loop **body** is in
discarded-status position like any other statement, so the field check
matters.

Caught while testing: my first version gave a bare `true` the abort message
("so a bare `false` ABORTS"), which is both confusing and false — `true`
cannot abort. The abort wording is now gated on the command actually being
`false`.

Strings and comments are untouched for free: the hint hangs off
`convert_command`'s `name` field, so only real command words reach it.

## Verification

Specs 57.141–57.147 in `spec/convert.sh` (plan 165 → 172), including a
control (57.147) asserting the hint does **not** fire on the word `true`
inside a string or a comment. Fail-first predicted 57.141–57.146 and observed
exactly those six; 57.147 was predicted to pass without the change (it
asserts an absence) and did.

Sweep **4500 passing, 9 skipped, 0 failing, 0 flips**.
