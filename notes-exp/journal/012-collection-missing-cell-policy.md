# Missing-cell policy for the C++ target

After the lifecycle milestone, DG asked for the first collection decision.
The AI presented native typed-zero reads versus strict missing-cell
failures, recommending typed zero. DG selected that option.
[Experiment DD#6](../decisions/DD06-missing-cell-reads.md) records the rule.

Missing string cells read `""`; missing integer cells read `0`.
Reads never insert entries, while stored empty/zero cells remain present.
Cell dispositions keep their scalar value semantics, and missing-cell
handling does not hide key-evaluation or other operation errors.

The AI updated the runtime roadmap and status to remove this resolved fork.
Negative indices, length, and traversal during mutation remain for DG to
settle. No C++ runtime, native compiler, IR reader, emitter, or producer
changed. The protected producer draft remains uncommitted.
