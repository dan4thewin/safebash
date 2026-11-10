title: Review capture failures during conversion, without choosing suppression
date: 2026-11-10

## DG's decision

The next question was `q-convert-capture-in-condition-operand`, filed when
F7 corrected the condition-status classifier. Bash can test the output of
a failed `grep` in `if [[ -n $(grep needle file) ]]` without aborting under
`set -e`. In sash, that capture is an operand, not the operation whose
result the condition handles.

The AI recommended the converter's existing `# CHECK:` category, rather
than automatically discarding the capture's status or leaving the
difference unexplained. DG selected:

> hint

This records a converter policy, not authorization to implement it now.
The compiler's DD55 no-reach rule is not reopened.

## Review rather than guessed intent

The hint explains that an uncaught failure inside the capture can abort.
The author decides whether to handle its status or explicitly discard it
where legal. The converter adds no `-`, fallback value or hard error.
Other mechanical conversions and annotations keep their contracts.

The rule covers conditional test operands in branches, loops and logical
lists. Quoting, interpolation and negating the outer test do not turn
captured text into a handled command status. An actual status consumer,
including a legal capture condition, is a different role; existing inner
handling must not be described as absent.

The old proposal advertised `$(- ...)` too broadly. It suffices for the
recorded simple external `grep` case, but does not suppress checks inside
called bodies or nested operands. Void/integer/status-less result rules
and fatal faults still apply. The hint must not promise a generic repair.

Taint and failure handling are independent. The direct external example
normally receives SB5020, which explains why F7's historical runtime
comparison used `--no-taint`. Extracting and attesting a value does not
itself handle failure of its acquisition.

## Evidence and acceptance

The F7 evidence remains in journal 536 and the task's archived `conv/`
probes; none was rerun for this decision. Current source-index inspection
confirmed the existing `add_true_false_hint` mechanism and
`status_is_consumed`'s unconditional treatment of command substitutions as
status consumers. An enclosing substitution alone cannot prove that the
outer test handles an operand command's failure.

The task's old acceptance criterion required converted output to run like
Bash under `--no-taint`. That is incompatible with the selected hint-only
policy: a comment does not change execution. Its replacement pins the
specific guidance, syntax-safe placement and unchanged executable code,
with role/literal controls and before/reversion evidence. A generic
`# CHECK:` assertion could be satisfied by an unrelated existing hint.

DD57 owns the policy; DD55 points to it without changing compiler
semantics. The existing P3/open task changes from question to work, and
the findings index and status distinguish the settled decision from
pending enforcement. No compiler, converter, runtime, spec or build code
changes here; no behavior probe, spec sweep or fresh coverage measurement.
