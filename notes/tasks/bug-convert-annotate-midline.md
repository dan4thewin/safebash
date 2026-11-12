title: "BUG: `sash-convert --annotate` writes its `# was:` comment mid-line, which comments out the rest of the line"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "with `--annotate`, every program under What converts to one that prints what bash prints, as it already does without `--annotate`"
  - "no `# was:` comment is followed on its line by code or by string text"
  - "specs in spec/convert.sh pin i2 and d2 (the silent shapes) and i3 (a loud one); 57.187 keeps passing"

## What

`sash-convert --annotate` records each rewrite as `  # was: ORIGINAL`, written
right after the converted span (`apply_patches`,
sash-convert/src/convert.rs:3177, through `push_annotation`, 3275). When the
span is not the last thing on its physical line, the comment swallows the
rest of the line. Code after it disappears, and inside a string the comment
becomes part of the string.

| Id | Bash | Converted with `--annotate` | Today |
|---|---|---|---|
| i1 | `/bin/true` / `x=$?; echo "after x=$x"` | `x=$(status)  # was: $?; echo "after x=$x"` | SB5050: the echo is commented out, so `x` is never read |
| d2 | `/bin/true; x=$?; echo "first x=$x"` / `echo "again x=$x"` | `/bin/true; x=$(status)  # was: $?; echo "first x=$x"` | silent: prints only `again x=0` |
| i2 | `/bin/true` / `echo "rc=$?"` | `echo "rc=$(status)  # was: $?"` | silent: prints `rc=0  # was: 0` |
| i3 | `/bin/true` / `if [ $? = 0 ]; then echo yes2; fi` | `if [[ "$(status)" == 0 ]]  # was: [ $? = 0 ]; then echo yes2; fi` | SB4027 |
| a3 | `/bin/true` / `[[ $? == 0 ]] && echo ok` | `[[ "$(status)"  # was: $? == 0 ]] && echo ok` | SB4068 |

Bash prints `after x=0`; `first x=0` then `again x=0`; `rc=0`; `yes2`; and
`ok`. Without `--annotate`, every one of them converts to a program that
prints the same as bash, so the annotation alone breaks them.

The fault is pre-existing: HEAD before D1 (bf48bec) gives the same results,
with `$(status)` unquoted. D1 (journal 535) moved the comment past the quotes
it adds, so it can no longer land inside an operand (spec 57.187). It did not
change the placement otherwise.

## Fix

Recommendation (made during D1, not yet ruled): write each annotation on a
line of its own, above the statement that holds the rewrite. The end of the
physical line is not safe in general. It can fall inside a multi-line string
or `$( )`, where the comment would again become text. The exact placement is
the implementer's choice.

## Related

- `dd57-convert-annotations` (done): the feature.
- DD#57's addendum of 2026-10-25 cites this task.

Found during the batch-531 port (step D1, while making `--annotate` keep the
added quotes intact). Filed under DG's rule for the port that questions and
uncovered cases become backlog tasks.
