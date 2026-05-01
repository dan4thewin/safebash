title: DD#21 — missing pipeline coercion amendment
status: ready
source: claude-opus-4.6
severity: accuracy

DD#21 note mentions only `- cmd` syntax. The journal has a full
amendment on pipeline coercion: `- cmd1 | - cmd2` for per-segment
suppression, and how scripts without pipefail are coerced (insert `-`
on non-terminal segments). None of this is in the decision note.
