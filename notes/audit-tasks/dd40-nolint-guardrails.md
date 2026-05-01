title: DD#40 — shellcheck skip with --nolint has no guardrails
status: ready
source: claude-opus-4.6
severity: gap

`--nolint` skips shellcheck entirely. No discussion of whether
certain shellcheck rules should be non-skippable, or whether
--nolint should be restricted to sash-native code only.
