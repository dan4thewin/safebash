title: Operand-word collection keys and quoted arithmetic map cells
date: 2026-11-25

# Operand-word collection keys and quoted arithmetic map cells

DG chose ordinary operand-word keys in normal code, then refined the
arithmetic distinction: only a bare cell uses arithmetic inside its
brackets, unless the entire key is quoted. Dollar/braced cell imports
retain their normal word grammar even inside arithmetic. DG authorized
implementation and repair of the associated defects.

The AI implemented this on `err-attempt-trial`. Main remains unchanged;
this delivery does not adopt DD136 or resume the separate capture/read
backlog or coverage work.

## One word grammar, one explicit arithmetic distinction

These are ordinary operand-word keys:

```sh
$m[$prefix:foo]
${m[$prefix:foo]}
m[$prefix:foo]=value
unset m[$prefix:foo]
$(( ${m[$prefix:foo]} + 1 ))
```

In these forms, `i+1` and `a[0]` are literal string keys, not implicit
arithmetic or implicit nested reads. A nested read needs its dollar
spelling; an arithmetic index needs `$(( ... ))`. Quoting is needed for
spaces or an empty string, not merely to concatenate a variable and text.

Bare arithmetic cells are distinct:

```sh
a[i+1]
a[a[0]]
m["key"]
m["$prefix:foo"]
(( m["$key"] += 1 ))
```

The whole quoted key selects word parsing. Partial quoting does not
silently convert the remaining arithmetic text into a word. String map
keys do not make string cell values integers: DD77's explicit conversion
still applies. Dollar/braced imports remain rvalues, never mutation
locations. Array indices remain integer-valued, map keys string-valued,
with the existing decimal-literal array-index and numeric-map-key quoting
rules.

Cell `:!` does not force the key read. Assignment retains the key before
the RHS; compound mutation also retains the old cell value. Unset retains
all keys before applying mutations. Slot typing, immutable args, scratch,
readonly ERR imports, alias restrictions and taint remain independent.
Dynamic map-literal initializer keys were not added.

DD79, DD80 and DD123 now describe the same current contract. Historical
shape-whitelist and bare-map-refusal passages are qualified rather than
left as contradictory current rules. GUIDE and the outline agree.

## Shared parsing and scoped checking

`parse_subscript_index` now calls the existing `parse_operand_word` and
requires the complete key to be consumed. It retains source origin and
scanner-fatal propagation. Reads, unset and assignment extents share the
balanced raw bracket scan; assignment recognition checks the operator
after the matching close rather than searching later tokens for a `]`
or imposing an arbitrary token cap.

Operator-free arithmetic imports now follow `ATok::ParamExpansion`, as
operator-bearing imports already did. The narrower `ATok::Subscript`
route and its private helpers are deleted. Quoted tokens preserve their
raw source. Bare quoted-map locations reuse the direct-key parser, while
unquoted bare indices keep the arithmetic parser.

After inference stabilizes, the existing scoped `run_stmt_pass` invokes
`check_subscript_keys` through `ir::walk_own_exprs`. Every ordinary
subscript and indexed parameter expansion sees its actual lexical
environment and collection signature, including forwarded slots. Child
bodies belong to the statement traversal, so nested diagnostics are not
duplicated. The old private `check_subscript_types` and
`check_subscripts_in_parts` walkers are deleted.

The bare arithmetic-subscript visitor also delegates to the canonical
arithmetic walk, preserving its import and pre-increment leaf boundaries.
This closes both the skipped-key-type P1 and the second-arithmetic-grammar
P2 without creating another private recursive walker.

## Coupled arithmetic evaluation repairs

The wider key grammar exposed eager lifting and stale-value reads in
arithmetic. Shared SSA lowering now branches before lifting `&&`, `||`
or ternary alternatives. It retains an earlier arithmetic value before
a later lifted operand can mutate the underlying storage.

An internal `TestValue` operation supplies branch truth without publishing
a source-visible status. Both backends distinguish it from `LetExit`,
which also updates status. Thus choosing a lazy path cannot overwrite
the handling status before a key such as `"$?:$(key)"` reads it.

Integer defaults now carry their subject kind in `SsaValue::DefaultValue`.
They no longer duplicate the subject into a ternary. Native and C each
evaluate the subject once, select the literal fallback on zero, and retain
the existing word-context stringification. String defaults keep their
empty-string and conversion rules; computed defaults were not introduced.

Two concrete witnesses:

- `keyeval.4` printed `10` then `9` on the frozen original compiler;
  retaining the earlier `3` before a helper writes `9` makes it print
  `4` then `9`.
- `keyeval.3` now prints `3`, `7`, `7`, `3`. An independent
  duplicate-subject mutation changes the last count to `4`; disabling
  integer-default handling changes the middle values to `0`. The original
  compiler instead failed compilation, with no stdout. It did not supply
  the four-call observation.

The broader evaluation-order owner records this bounded progress and
stays open. General argument-list ordering, its historical status examples,
capture-entry checking and attached-redirect timing are not claimed fixed.

## Before-controls and independent reversions

There are 55 fresh dual-mode assertions across `wordkey.1`-`.22`,
`maparith.1`-`.17`, `keytype.1`-`.10` and `keyeval.1`-`.6`. Changed
contracts retire their old IDs rather than recycling them.

The frozen original working compiler fails 50 final changed-suite
assertions per backend: 49 in subscript and one in arithmetic.
`wordkey.16`, `maparith.14` and `keytype.10` are passing preservation
controls, not newly repaired behaviors. The two later diagnostic migrations
add exactly two more before-failures per backend. This is a frozen working
build, not an independently rebuilt historical commit.

The positive focused files pass 228 subscript, 484 arithmetic, 506 type
and 178 collection-parameter checks in each backend. The migrated coverage
file passes all 374 checks in each mode; only its two changed expectations
fail under the original compiler.

Ten independent mutations each ran the complete 228-check subscript file
in both modes:

| Mutation | Observed failing checks |
|---|---|
| Parse a key as an expression instead of an operand word | `wordkey.12`, both modes |
| Remove the scoped shared key pass | 30 per mode, including all `keytype.1`-`.9`; `.10` stays green |
| Lift both ternary alternatives eagerly | `maparith.10`, `keyeval.1`, `.2`, both modes |
| Omit retention of the earlier arithmetic operand | `keyeval.4`, both modes |
| Duplicate the integer-default subject | `keyeval.3`, both modes |
| Publish status from native `TestValue` | native `keyeval.6` only; C entirely green |
| Publish status from C `TestValue` | C `keyeval.6` only; native entirely green |
| Parse wholly quoted bare-map keys as arithmetic | `maparith.4`-`.13`, `.15`-`.17`, `keyeval.5`, `.6`, both modes |
| Disable native integer-default handling | native `keyeval.3` only; C entirely green |
| Disable C integer-default handling | C `keyeval.3` only; native entirely green |

Two initial predictions were too broad. The eager-lifting mutation did not
make `keyeval.5` fail; its preservation mechanism was not established.
The bare-map mutation leaves `maparith.1`-`.3` green because those are
dollar/braced imports, not bare locations. Its `.14` refusal also remains.
Two draft mutations failed to build before their executions: invalid field
names and a misplaced restoration. Neither is counted as a product defect
or a successful mutation experiment.

All mutations are restored. Source and compiler identities matched the
saved positive after restoration, including byte-identical compiler output.
The later spec-comment corrections and two coverage migrations have their
own final manifest. Runtime source/archive and both local locks are unchanged.

## Full gate and the missed diagnostic migrations

The first gate used `NEW=106 RETIRED=28 FLIPS=0 TOTAL=13086`.
Rows 7412207-7425292 had the right total, additions and retirements,
but four failures: `cov.110` and `cov.147` in both modes.
The actual xtraces showed two missed fixture migrations, not new runtime
failures:

- `$a[$i-]` is now a valid string word and therefore an SB4030 array-key
  type error, not the old SB4041 shape refusal.
- `a[]=1` is recognized as an indexed assignment and receives the shared
  SB4041 empty-key diagnostic, rather than falling through to SB5011.

The replacements are `wordkey.21` and `.22`, with distinguishing message
assertions. Their source programs and the coverage-file plan are unchanged.
The four complete before/after mode runs confirmed exactly those two
before-failures and no positive failures. The old `cov.146` malformed-name
refusal stays unchanged.

The corrected gate used:

```sh
NEW=4 RETIRED=4 FLIPS=0 TOTAL=13086 timeout 1800 mk -p 1 gate
```

It passes at rows 7425293-7438378: 6659 native and 6427 C checks,
13073 passing and 13 existing skips. There are no failures, flips,
ghosts or plan mismatches. Liveness is 246. Ordinary build/unit and
static gates pass; the obsolete assignment token-cap invariant unit was
retired with its helper.

Against the original 13008-check block, rows 7399199-7412206, all
12976 retained statuses are unchanged. Exactly 110 mode IDs are added
and 32 retired, a net increase of 78. The comparison uses those row
blocks, not a shared uncommitted HEAD hash.

Session evidence is retained under `files/word-keys-20261124/`, including
frozen compilers, positive/restoration manifests, before/after logs and
xtraces, `reversion-results.json`, both gate predictions/logs and
`final-gate-comparison.json`. No fresh coverage measurement was made.
