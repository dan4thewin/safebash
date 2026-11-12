title: DD#21 — Error Suppression with - Prefix
status: implemented

1. A command-adjacent `-` discards one eligible result, without reaching
   operands or called bodies; fatal faults remain fatal (DD55/DD130).
2. Pipeline suppression is per stage; unsuppressed stages still contribute
   to pipefail.
3. A discarded result cannot also be consumed by a condition or return.
4. Bash conversion preserves the source's pipefail choice through explicit
   per-stage suppression.

---

`- cmd` suppresses failure (equivalent to `cd /tmp || true`).

## Pipeline coercion

Sash has pipefail always on (DD#20). Every pipeline segment's exit
status must be handled — checked, ignored (`-`), or propagated.
`- cmd1 | - cmd2` is per-segment suppression.

**Coercion from bash with `set -o pipefail`:** No `-` inserted.
~~Compiler flags unchecked segments. Author decides.~~ DD55 inserts the
ordinary implicit pipeline check; explicit stage discard is the author's choice.

**Coercion from bash without pipefail:** Insert `-` on every
non-terminal segment (preserves bash default: only last matters):

```bash
# bash (no pipefail):
cat file | grep pattern | sort | head -5
# sash:
- cat file | - grep pattern | - sort | head -5
```
