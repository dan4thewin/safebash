title: SUBSET.md — unset semantics non-obvious
status: ready
source: gemini-pro
severity: gap

SUBSET.md lists unset as a builtin that "assigns empty." Bash's unset
removes a variable entirely. This coercion is non-obvious — a script
writer might expect `${var:-default}` to trigger after unset. Consider
whether unset should be rejected rather than silently redefined, or
at minimum document the semantic difference prominently.
