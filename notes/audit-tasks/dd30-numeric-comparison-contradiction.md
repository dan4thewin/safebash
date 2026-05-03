title: DD#30 vs SUBSET.md — numeric comparisons in [[ ]]
status: resolved
source: claude-opus-4.6
severity: contradiction

DD#30 says `-eq`/`-ne`/`-lt` etc. are "rejected inside `[[ ]]`" and
coerced to `(( ))`. SUBSET.md lists them as accepted operators under
`[[ ]]` tests. Verify which the implementation does and reconcile.
