## 210 — non-colon parameter family rejected (`-` `+` `=` `?`) + SUBSET.md deleted

**On `hand-parser`.** DG-directed follow-up to journal 209. DG asked "Isn't
`${x-4}` invalid sash?" — it should be, and it was a **silent-degrade gap**.

### The bug

`split_operator` didn't recognise the non-colon default family (`-` `+` `=` `?`),
so `${x-4}` fell through `parse_parameter_expansion` to a bare `${x}` with the
`-4` **silently discarded**. Worse, `${x+4}` returned `$x` (5) instead of the
alternate (bash: 4) — a silently *wrong* answer. The whole family was accepted
and degraded, exactly the "reject rather than silently degrade `${x#a}`→`$x`"
failure DD#65's machinery exists to prevent; the non-colon forms had slipped
through because `split_operator` never emitted them as operators.

### DG decision

"clarity favors a single spelling. DD65 gives the answer." → **reject** all
non-`:-`/`:?` forms (SB4034), **no compiler coercion** ("there is no coercion,
but conversion is a separate concern" — `-`→`:-` migration is sash-convert's
job). One spelling: the colon forms `:-` (default) and `:?` (require, DD#77).

### Fix

- `parse/expr.rs` `split_operator`: add `-` `+` `=` `?` to the one-char ops, so
  `${x-4}` parses to operator `-` (not a dropped bare `${x}`).
- `parse/parser.rs` `is_dropped_param_operator`: add the four → SB4034 via the
  existing `check_rejected_param_expansion` path. Message now points at
  `:- / :? / :off:len / :s/`.
- The per-element array path (`parse_subscript_operator`) is self-guarded by its
  own `matches!` allow-list, so it's unaffected; the converter uses
  tree-sitter-bash (not the sash parser), so `convert.sh` 57.7a
  (`${HOME-default}`→`${HOME:-default}`) is unaffected too.

Specs `spec/var_modifiers.sh` vm.dr19-22 (one per non-colon op).

### SUBSET.md deleted

DG: "remove SUBSET.md, it's worse than useless." It claimed non-colon forms are
"silently coerced to colon forms" (contradicting this decision) and still listed
the DD#65-dropped `:=`/`:+` as *supported* — drifted from the DDs, actively
misleading. Deleted `docs/SUBSET.md`; repointed the active references
(README, AGENTS, GUIDE file-listings; REVIEW.md's Phase-1 table) at
`notes/decisions/` as the subset authority. Historical journal/task mentions
left as dated records.

Also corrected journal 209 + the DD#65 identity-element addendum, which had
wrongly said bare `-` "matches bash / is unaffected" (that rested on the
`sash -c` code-string-vs-file test mistake).

### Verification

Full 3-mode sweep vs HEAD `174164e`: **0 regressions** native/c/janet; +4 each
(vm.dr19-22). fmt + clippy clean. DD#65 non-colon addendum recorded.
