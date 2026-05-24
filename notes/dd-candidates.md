---
title: DD Candidates
modified: 2026-05-24
---

# DD Candidates

Potential design decisions that need further consideration before
becoming actionable. Not sufficient on their own to be the basis
for architectural decisions.

---

## Compound-command redirects/pipes

**Question**: Should sash support redirects and pipes on compound
commands (`done > file`, `fi | cmd`, `done &`)?

**Context**: Legal bash, not flagged by shellcheck. But unusual in
practice. Creates a hidden scope — the redirect applies to
everything inside the compound command, but you don't see that
until you reach the closing keyword.

**Sash idiom alternative**: Wrap in a function, redirect/pipe the
function call. Explicit, composable, easy to reason about.

```bash
fn generate_items {
    for x in a b c; do echo $x; done
}
generate_items > /tmp/out
generate_items | /usr/bin/wc -l
```

**Currently**: Implemented (spec tests 14.4, 14.5, 14.6 pass).
`maybe_wrap_redirects` in the parser handles it generically.

**Disposition**: Drop from the new parser now. The function-wrapping
idiom is the sash way. Spec tests 14.4–14.6 become "must reject"
tests (or are removed). A future DD could reinstate if a compelling
case emerges, but the default is exclusion from the safe subset.

**Open questions**:
- Are there cases where the function-wrapping idiom is significantly worse?
- Does removing this break any real-world converter output?
  (converter should emit function-wrapped form instead)
