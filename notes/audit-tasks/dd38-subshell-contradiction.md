title: DD#38 vs SUBSET.md — subshell contradiction
status: resolved
source: claude-opus-4.6, gemini-pro, gpt-5.4
severity: contradiction

Resolved: DD#38 updated to document mechanical coercion of bash-mode
subshells to sash primitives (local, save/restore cwd, explicit fork).
SUBSET.md updated with "Subshells (bash-mode coercion)" section.
GUIDE.md IR note clarified that Subshell exists in IR but is coerced
during lowering.
