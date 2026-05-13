title: DD#21 — Error Suppression with - Prefix
status: implemented

`- cmd` suppresses failure (equivalent to `cd /tmp || true`).

## Pipeline coercion

Sash has pipefail always on (DD#20). Every pipeline segment's exit
status must be handled — checked, ignored (`-`), or propagated.
`- cmd1 | - cmd2` is per-segment suppression.

**Coercion from bash with `set -o pipefail`:** No `-` inserted.
Compiler flags unchecked segments. Author decides.

**Coercion from bash without pipefail:** Insert `-` on every
non-terminal segment (preserves bash default: only last matters):

```bash
# bash (no pipefail):
cat file | grep pattern | sort | head -5
# sash:
- cat file | - grep pattern | - sort | head -5
```
