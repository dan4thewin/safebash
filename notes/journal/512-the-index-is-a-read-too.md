# 512 - The index is a read too

2026-10-16. DG: "continue with coverage".

Started at `20c32bc`, with fresh coverage: 275 compiler production cold lines,
ten in `parse/expr.rs`. Read the whole cold functions and their producers,
then ran a corpus with positive controls. Four lines were source-reachable;
the other six required different treatment. The investigation found four
related correctness gaps, not just missing tests.

## The suffix that vanished

These compiled and printed the cell's value:

```sash
array a=(x); echo ${a[0]garbage}
array a=(x); echo ${a[0]&junk}
int array a=(7); echo $(( ${a[0]garbage} ))
```

The scalar `${x&junk}` already rejects with SB4047. The cell path's
`scalar_op_or_bare` silently replaced an unrecognized suffix with a bare
parameter expansion. Both paths now share the unknown-content sentinel;
the raw suffix survives to the same SB4047 diagnostic. `celop.1`-`celop.4`
cover arrays, maps, interpolation and arithmetic. This applies DG's DD#65
no-ambiguity rule; it is not a new language choice.

## A cell takes the existing trust modifier

`${a[0]:!}`, `${a[0]:!-fallback}`, `${a[0]:!?missing}` and `${a[0]:!~}` all
rejected with SB4039. The private cell dispatcher listed only `:-`, `:?`,
`:~`, so trust never reached the scalar parser. DD#91 and DG's DD#123
cell-is-variable principle settle the answer. These forms now use the
existing trust parser, and the existing lowering already preserves the cell
index and wraps its stored value in `Trusted`.

Applied that answer to words and arithmetic, arrays and maps, and braced and
brace-less forms. Trust still does not elide unless `~` is present, does not
declassify a later bare read, and does not legalize elision outside a list.
The int identity-element default remains distinct from the string path.
The old unsupported-cell-colon rejects retain SB4039; their diagnostic no
longer falsely says substring and sed on cells are unimplemented.

## The index had disappeared from the walkers

The new route exposed a deeper omission: `ParameterExpansion` has **two**
expression children, `index` and `operand`, but several walkers only visited
the operand. Reproducing the taint case required a separate vouched read of
`i`, because otherwise the dead-variable gate masked it:

```sash
array a=(ok)
str i=$(arg 1)
: ${i:!}
echo ${a[$(( ${i:-0} ))]:!}
```

Before the index fixes it compiled and printed `ok` with argument `0`.
Without the outer trust modifier the SSA backstop caught the index instead.
Trusting a cell's stored value does not vouch for the independent read that
selects the cell.

Fixed the shared expression walker, read enumeration, taint-use and
taint-source predicates, argv/retired-splat checks and integer-default
traversal. The shared walker is now exhaustive over `ExprKind` and opts into
the totality gate. The consequences are individually pinned:

- A variable read only as the index is no longer falsely dead; a typo in the
  index is SB5031.
- Unvouched index variables and sources are rejected in command arguments,
  scalar assignments and array literals, including vouched map-cell reads.
- Rejected operators nested in the index are visible to the shared walk.
- `$(argv)` in an index is SB5025, rather than reaching the zero-argument
  `argv` transform ICE; `$@` remains SB5045.
- An invalid int default in the index is SB4030, rather than the codegen
  "non-integer literal ... reached int lowering" panic.

## Trust is not a numeric conversion

`array a=(13); echo $(( ${a[0]:!} ))` still compiled after routing trust
correctly. The cell arithmetic gate only recognized `Subscript`, not the
operator-less `ParameterExpansion` produced by `:!`. Checking the scalar
sibling found the same escape: `str s=13; echo $(( ${s:!} ))` printed `13`.

Both now owe DD#77's disposition and reject with SB4030. The scalar check
uses the existing imported-int-operand traversal and the same scalar error
authority as bare arithmetic variables; the cell classifier recognizes an
undisposed trusted import. Real dispositions and int cells retain their
behavior. Extracted small classifiers/error helpers to clear the measured
complexity gate, without bypasses or a second arithmetic walker.

## Closing the ten lines

| Baseline cold material | Treatment |
|---|---|
| Cell `@Q` return | Source spec: SB4015, including the distinguishing transform text |
| Retired `[@]` disposition: two fallthrough lines | Source spec: the per-element classifier correctly declines; SB4038 |
| Newline inside an array literal | Source spec: multiline elements, with explicit `join` |
| `balanced_close`'s `(` and invalid-opener entries | Two direct API-contract tests; source callers enter on `[` or `{` |
| `attach_cell_index` / `trusted` silently accepting a non-parameter result | Two construction-contract ICEs, directly tested |
| Command-capture `None` / `Break` relay: two lines | Deleted after enumerating the caller guards and token consumers |

The two annotation guards are not rivals of a validator check: they guard
the operator parsers' result kind before attaching an index or trust flag.
A silent fallback loses precisely that annotation. No source spec can
supply the wrong constructed kind; direct-call tests do, and assert the
contract text. The balanced scanner tests instead assert its correct
generic API answers, including `None` for an invalid opener.

For the relay proof, `collect_cmd_sub_body` excludes EOF;
`cmd_sub_body_step` consumes newline/semicolon and handles the closer before
dispatching. A first-word shortcut miss consumes nothing: `try_special_word`
does both rejection checks before either `next`. `parse_statement` can
therefore never return its EOF `None` here. The sibling now calls
`parse_statement_here`, exposed only to the parent module, and the `Break`
variant is gone. Compiling the narrowed call enforces that contract; it
does not prove the old branch unreachable.

## Measurement and gates

31 new dual-mode specs: eight `celop`, 21 `ctrs`, two `alayout`. Predicted
**8403 + 62 = 8465**, with per-file plans raised by 31, not 62. Historical
high-water queries found the blocks unused; literal/variable collision
checks and plan counts agree.

Full native/C sweep: **4335 native + 4130 C = 8465**, 8452 passing and 13
existing C skips. Exactly 62 new IDs on the first sweep, zero on reruns,
zero row-order flips, no unexpected retirements. Comparison-query liveness
258. Unit gate: **135 + 27 + 20**, four new non-rival tests. Enforced fmt,
clippy, totality and touched-function complexity gates all pass.

The actual spec files, not only the initial snippets, ran under the
instrumented compiler. Their exported line counts in `parse/expr.rs` were
1133 = 1, 1164 = 1, 1543 = 1, 1817 = 3: all four source-reachable targets
positive. An initial standalone export lacked the runner's target-directory
environment and failed explicitly; using that recorded target directory
produced the counts. No empty report was treated as evidence of reachability.

Fresh full `mk cov-union` -> `mk cov-db` -> `mk cov-stale`: **expr production
10 -> 0; compiler 275 -> 265**. `parse/parser.rs` remains at zero. Raw
cargo-llvm-cov headline: **98.77% line / 99.23% function / 93.00% branch**.
Zero production cold lines is not a claim of zero cold branches. Final
measurement follows the diagnostic/comment review; uninstrumented binaries
are restored with `mk cov-reset`.

## The generated progress label

Final status review caught a provenance fault in the generated block being
committed with these results: `status-update.sh` called `MAX(commit_hash)` the
last run. It printed `fffc23f` although the last recorded row was `20c32bc`.
The generator now takes the hash from the last row, by rowid, rather than
sorting hashes lexically. Regeneration is checked against that row; the old
query yields a different hash, so this check would fail against the old code.
Counts and the spec gate are unchanged.
