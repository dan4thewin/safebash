title: "Add CHECK hints for command captures in conditional test operands"
status: open
priority: P3
kind: work
depends: []
gates:
  - "DD57's CHECK hint explains the capture-operand failure boundary without adding suppression, a fallback or a hard converter error"
  - "convert.sh covers branch, loop and logical-list test conditions, including quoted/interpolated and nested capture operands"
  - "Controls distinguish literal text, actual status consumers and explicit inner handling; unrelated conversions and annotations retain their contracts"
  - "Hint assertions identify this message rather than any CHECK comment; before and reversion controls prove the new guidance, and normal gates pass"

## Decision (DG, 2026-11-10)

DG chose:

> hint

DD57 now requires a `# CHECK:` review hint for command captures used as
conditional test operands, not automatic rewriting or silence. The
converter leaves the code unchanged apart from the comment and its existing
independent conversions. It does not insert `-`, manufacture a fallback,
or refuse conversion.

The message explains the separate failure boundary: the enclosing test
handles its own boolean result, not failures inside its capture operand.
The author chooses appropriate handling or legal explicit discard. Apply
this to the test conditions of `if`/`elif`, `while`/`until` and `&&`/`||`;
quotes, interpolation and negation do not change the boundary. Equivalent
Bash test and capture spellings must be classified by their parsed roles,
not by searching for `$(` bytes.

The compiler's DD55 no-reach rule is already settled and stays unchanged.
Taint requirements remain independent. The direct external example below
is normally SB5020, but extracting and attesting the value does not itself
handle a failing acquisition.

This task retains its slug, P3 priority and open status; only the question
is answered. Implementation is pending. Journal 609 records the decision.

## Historical evidence (F7, 2026-10-26; not rerun for this ruling)

In the Bash example, `set -e` does not abort for a failing command inside
the conditional test operand: `if [[ -n $(grep zzzq "$f") ]]` takes the else
branch. The converter copied the line. F7 corrected sash's condition-status
classifier, so the capture's implicit check fires instead of being mistaken
for the enclosing test's handled result (DD55; journal 536).

Probe `conv/g.sh`, Bash with `set -e`, converted and with the `set -e` line
removed (`g2.sash`), as recorded then:

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

For this fixture the runtime difference is visible only under `--no-taint`;
normally SB5020 stops it before execution. The extracted form,
`x=$(grep ...)` then an `attest` then `[[ -n $x ]]` (`g3`), aborted on both
binaries except where `fec83f7` incorrectly unchecked the command just
before a test condition. That was B85, not a converter feature; spec 55.99
pins the fix.

The simple `$(- grep ...)` probes (`g4`, assignment; `g7`, operand) continued
on both recorded binaries. That is evidence for those direct external
commands, not a universal repair for functions or compound capture bodies.
The prefix discards only a legal result at its own operation; it does not
suppress nested operand/body checks or DD130 fatal faults. Status-less
intrinsics, void functions and integer captures have their own result-kind
restrictions. The old option describing this as suppressing every failure
is superseded.

Probes: `files/port-f7-probes.tar.gz`, directory `conv/` (g.sh, g2-g7).

## Implementation and proof

Use the existing converter annotation mechanism. Current source inspection
at the recording baseline `34a48bad` confirms `add_true_false_hint` emits
`# CHECK:` lines and `status_is_consumed` treats `command_substitution` as
consuming status. That classification cannot establish that an outer test
handles the command inside its operand. Reconcile any affected existing
hints rather than producing contradictory advice at the same site.

The hint should identify the capture boundary and prompt review, not
prescribe `$(- ...)` indiscriminately. Keep actual direct command conditions
and legal capture conditions distinct from operand captures. Preserve
explicit handling within a capture; do not describe that handled result as
uncaught. Single-quoted literal capture text and comments are not captures.

Add focused `spec/convert.sh` assertions for the message and its placement,
including the historical `if` and `&&` forms and the related contexts above.
Pin distinguishing text: an existing ShellCheck or `true`/`false` hint must
not satisfy the assertion merely because it also starts with `# CHECK:`.
Check that the new comment does not swallow following code and that
executable output remains unchanged apart from independent conversions.
Before/reversion controls must prove missing guidance, not a compiler
refusal the fixture already receives for taint.

The former gate requiring the unchanged converted program to run like Bash
under `--no-taint` is withdrawn: annotation does not repair runtime
semantics. No new compiler behavior or native/C spec flip is intended.
Choose fresh converter spec ids above the database high-water mark, derive
the actual gate predictions during implementation, and run the normal gates.
