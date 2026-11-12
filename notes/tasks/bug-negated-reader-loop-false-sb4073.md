title: "Bug: a final-stage loop that reads under `!` (`until ! readline x`) is a false SB4073"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "w5 and w7–w10 compile and read their input in both backends: w5, w7, w8 and w10 print `got a`, `got b`, `after`; w9 prints `after`"
  - "a final-stage loop that really does not read its input is still SB4073 (pf specs unchanged)"
  - "specs pin w7, w9 and w10 in both backends"

## Found

2026-10-27, while porting F1+F4 (journal 537). It is **pre-existing**: the
same programs are SB4073 at `8e28c61` (before F1+F4). There SB4073 called
every loop a `while`; F1+F4 made it name the keyword (pf.67, pf.68).

A loop as a pipeline's last stage must read the pipe (DD#122), and SB4073
rejects one that never does. These loops do read it, under `!`, and are
rejected anyway, with the advice to "add one":

| Id | Program | Result |
|---|---|---|
| w7 | `/bin/printf "a\nb\n" \| until ! readline x { echo "got ${x:!}" }` then `echo after` | SB4073 1:24 "this `until` is a pipeline's last stage but never reads it — … add one, or drop the `\|`" |
| w8 | w7 with the builtin `printf` | SB4073 1:19 |
| w9 | `/bin/printf "a\nb\n" \| while ! readline x { echo never; break }` | SB4073 1:24 |
| w10 | `/bin/printf "a\nb\n" \| while /bin/true { if ! readline x { break }; echo "got ${x:!}" }` | SB4073 1:24 |
| w5 | w7 with `- until` | SB4073 1:26; at `8e28c61` SB4014 1:32 "expected expression" |

These work:

| Id | Program | Result |
|---|---|---|
| w11 | `/bin/printf "a\nb\n" \| while /bin/true { readline x \|\| break; echo "got ${x:!}" }` then `echo after` | `got a`, `got b`, `after` |
| u1 | `until ! readline x { echo "got ${x:!}" }`, no pipeline | compiles and runs |

Probes: `files/port-f1f4-probes.tar.gz`, directory `fr/` (w5, w7–w11,
ir/u1).

## Cause (read, not yet confirmed by a fix)

`parse_negated` (sash/src/parse/parser.rs) represents `! X` as a
one-command Pipeline with its negation flag set. `binds_a_pipe_reader`
(sash/src/ir.rs) returns false for every Pipeline, so a `readline` under `!`
is not seen as reading. Its one caller, `reject_nonconsuming_final_stage`
(parser.rs), then raises SB4073. F1+F4 modified that caller but not
`binds_a_pipe_reader`, which is where the fix belongs. So this goes to the
backlog under DG's rule for the port (fix a known bug in a function the port
modifies; file the others).

`bug-sb4025-advice-misdescribes-rhs` has the same root: a one-command `!`
pipeline is treated as a pipeline.

## Fix direction

Treat a one-command negated pipeline (the `!` sugar) as transparent in
`binds_a_pipe_reader`. The `!` inverts the status, and the reader still
reads. Then run w5 and w7–w10 to verify, in both backends; the abort path and
the DD#122 reader cleanup are what to watch.
