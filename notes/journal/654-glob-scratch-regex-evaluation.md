title: Evaluating the glob, scratch and regex trial
date: 2026-11-17

# Evaluating the glob, scratch and regex trial

DG required the DD131/DD132 package to be implemented and evaluated on a
separate branch before deciding whether to adopt it. The AI has completed
that implementation and the bounded practical, migration and resource
evaluation below. The evaluated trial is `76376c4e` on `glob-scratch-trial`;
the comparison is original main `6c7b899f`, not the preceding trial unit.
Main is unchanged. DD131/DD132 remain provisional and their
[integrated owner](../tasks/q-glob-semantics.md) remains open for DG's review.

The package makes useful programs simpler, especially fresh per-attempt
results and directory selection from validated pattern lists. It also has
two material adoption issues: unchanged converter output can silently read
old entries after an accumulating fill, and repeated append now repeatedly
scans/copies growing destination metadata. The latter is not a vague
"immutability overhead"; ordinary mutable arrays pay it too.

## Practical utility

Eight practical scenarios ran through the real CLI and both native/C
backends, with exact output assertions and normal taint checking:

| Scenario | Observed result |
|---|---|
| Default directory manifest | `.hidden.log`, `build 2.log`, `ready.log` |
| Ordered regex-list manifest with a duplicate pattern | `ready.log`, `build 2.log`, `ready.log` |
| Interactive candidate validation and selection | `selected=1`, `invalid-pattern`, `selected=0`, `selected=1` |
| NUL records accumulated across files | `alpha,beta,gamma` |
| Fresh match groups per attempt | `build:7`, `skipped`, `app:12` |
| Normalization with text and matched/no-match truth | `changed:build-7`, `unchanged:app-12`, `changed:-alpha-` |
| Two helper fills through a collection slot | `build 2.log`, `ready.log` |
| Mutable builder and independent immutable snapshot | `a,b:changed,different,c,d` |

These exercise useful compositions rather than substitute for the earlier
failure/proof gates. In particular, zero directory matches are successful
selection, a bad regex candidate is quiet false, and neither result is a
directory-operation failure. Duplicate patterns retain duplicate results;
fresh match groups avoid the old-index trap described below.

A mutable regex builder can remain mutable while each use gets a fresh,
validated candidate:

```sash
str pattern='^a'
for suffix in p ple {
    pattern+=$suffix
    str _pattern=$pattern
    attest _pattern r?
    if match apple $_pattern { echo $_pattern }
}
```

Both trial backends print `^ap` and `^apple`. This is a real use for
fresh-on-revisit construction, rather than a reason to weaken the immutable
pattern requirement or to make a mutable builder's old proof survive edits.

The repository's actual tour checker already needed two adaptations:
construct `_fence` from the mutable fence builder and attest it with `r?`;
explicitly discard `gsub`'s Boolean when indenting captured output. Journals
[650](650-immutable-regex-proofs.md) and
[652](652-regex-results-and-redirect-indices.md) record those changes and
byte-identical rendering of all 56 examples. That accepted evidence was
retained, not represented as a new tour run during this evaluation.

## Migration is not automatically supplied by the converter

The frozen main compiler/runtime and the complete trial pair ran the
compatibility controls in both backends. `sash-convert` source has not changed
between those commits. Existing converter annotations were reviewed before
comparing accepted programs; an unrelated refusal is not migration evidence.

| Existing source/use | Main | Trial |
|---|---|---|
| Reassign a mutable `_label` | Runs | SB5103 |
| Define functions named `glob` or `rglob` | Runs | SB5092 |
| Use a mutable/unproved runtime regex | Runs | SB5106 |
| Two converted `mapfile` fills into the same ordinary array | Prints `first`, `second` | Prints `first`, `first` |
| Converted `mapfile` loop with `array records` declared inside the loop | Prints `first`, `second` | Prints `first`, `second` |
| Reuse an ordinary capture-group output array | `build:7`, `skipped`, `app:12` | `build:7`, `skipped`, `build:7` |

The sequential mapfile case, after reviewing its attests, contains:

```sash
array records
/usr/bin/tr "\n" "\0" < first.txt | read records
attest record in records p?
printf "%s\n" "${records[0]}"
/usr/bin/tr "\n" "\0" < second.txt | read records
attest record in records p?
printf "%s\n" "${records[0]}"
```

Each source file contains its corresponding one-line word. Accumulation is
the intended DD131 behavior, but the old translation no longer preserves
the replacement intent of the Bash input. Both programs compile and exit
successfully: this is a silent converter compatibility problem, not an
annotation that already forces the author to notice the changed index.
The loop counterexample matters: its declaration resets the array each
iteration, so it does not demonstrate the same defect.

Eighteen additional mode/version observations verified author-reviewed
adaptations: rename mutable underscore bindings; rename the two new reserved
command names; construct and validate each regex candidate; use distinct
`_first`/`_second` results for the sequential mapfile example; and finish the
literal-glob converter sample's existing splat-disposition review. The latter
selects the same three files on both versions. The trial's exact sorted order
was asserted, but no old sorting guarantee is inferred from this directory.

These are working adaptations, **not an implemented converter repair**.
Neither blanket renaming, clear-before-fill, private fill plus replacement,
nor a new annotation policy has been selected. A general translation must
account for aliases, later uses, failure state and taint; inserting a reset
is not automatically equivalent to completing a replacement successfully.

## Resource measurements

The final resource set has **58 unique workload/version/backend
configurations across 17 workloads**. Comparable old/new cases use the same
source and input. Each successful timing point checks output, then takes
five samples; counts are 1, 100, 1,000 and 4,000. Accumulating glob uses
1, 10, 100 and 1,000 because each call retains another 32 entries.
Execution and compilation are bounded; workloads run serially.

Times are medians including process launch and `timeout`, on a shared host.
Small scalar differences and precise native-versus-C speed comparisons are
not meaningful. A 64 MiB configured heap is used for timing. Separate
exponential searches and bisections find adjacent failing/passing configured
heap sizes at 4 KiB resolution for 100 and 1,000 iterations. These are
**heap-sufficiency brackets, not measured live or peak allocations**.
Glob figures depend on this fixture's 32 names and its absolute path length.

At 1,000 iterations every native/C pair has the same bracket:

| Workload | Main: fails / passes, KiB | Trial: fails / passes, KiB |
|---|---:|---:|
| Scalar copy | 20 / 24 | 20 / 24 |
| Scalar `toupper` construction | 20 / 24 | 20 / 24 |
| Buffered stdout capture | 288 / 292 | 288 / 292 |
| Three-byte file capture | 8,036 / 8,040 | 8,036 / 8,040 |
| Boolean literal regex match | 33,664 / 33,668 | 33,664 / 33,668 |
| Validate immutable pattern once, then match | Not available | 33,664 / 33,668 |
| Reconstruct and validate before each match | Not available | 33,664 / 33,668 |
| Fresh match-group array | 36,084 / 36,088 | 36,084 / 36,088 |
| Fresh keys snapshot | 780 / 784 | 780 / 784 |
| Fresh two-record NUL file read | 8,576 / 8,580 | 8,576 / 8,580 |
| Literal glob array sugar, 32 files | 7,124 / 7,128 | 9,888 / 9,892 |
| Explicit fresh `glob`, 32 files | Not available | 7,236 / 7,240 |
| Fresh `rglob`, 32 files | Not available | 7,280 / 7,284 |
| Mutable builder plus independent snapshot | 516 / 520 | 516 / 520 |
| One-cell append | 80 / 84 | 80 / 84 |
| Two-cell append | 160 / 164 | 320 / 324 |
| Accumulating `glob`, retaining 32,000 entries | Not available | 10,044 / 10,048 |

The buffered capture emits `ab` with a newline and then `c`, retaining four
bytes; the file capture reads `abc`. The NUL reader consumes an actual owned
file containing `a\0b\0`, not a shell here-string. All benchmark input files
are outside the `*.dat` match set, which stays exactly 32 files.

### The new append cost

| Workload | Iterations | Main native / C, ms | Trial native / C, ms |
|---|---:|---:|---:|
| One-cell append | 4,000 | 1.98 / 1.86 | 17.04 / 12.09 |
| Two-cell append | 1,000 | 1.68 / 1.75 | 22.92 / 22.76 |
| Two-cell append | 4,000 | 2.52 / 2.72 | 328.70 / 328.20 |
| Fresh keys snapshot | 4,000 | 3.08 / 3.30 | 3.08 / 3.16 |
| Fresh two-record read | 4,000 | 27.88 / 28.70 | 28.46 / 29.01 |
| Literal glob array sugar | 4,000 | 88.28 / 71.67 | 78.57 / 77.66 |
| Explicit fresh `glob` | 4,000 | Not available | 73.41 / 74.10 |
| Fresh `rglob` | 4,000 | Not available | 108.49 / 104.44 |
| Accumulating `glob` | 100 | Not available | 8.74 / 7.26 |
| Accumulating `glob` | 1,000 | Not available | 534.88 / 388.88 |

The append comparisons retain identical results on main and trial. For
example, the two-cell workload is:

```sash
text=$(arg 1 "1")
attest text (( count = text ))
int i=0 total=0
array builder
for (( i=0; i<count; i++ )) {
    builder+=(left right)
    (( total += 2 ))
}
echo "$total:$(len builder)"
```

With argument 4000, both print `8000:8000`. The trial takes about 0.33
seconds instead of 0.0025-0.0027 seconds. At 1,000 iterations its passing
heap endpoint is 324 KiB rather than 164 KiB.

The current runtime explains the scaling. `sash_array_append_batch` calls
`sash_array_next_index` for every nonempty batch. That scans the existing
entries for one past the largest live nonnegative index. A one-cell batch
then uses allocation-before-insertion directly. A multi-cell batch first
clones the destination's mutable table/index metadata, appends privately,
and publishes by swapping storage into the original header. It frees the
displaced metadata, not backing values that other views may retain.

Repeated growth therefore repeats a full index scan, and multi-cell growth
also repeatedly copies the growing metadata. Both old and new storage must
coexist during publication. Journal 647's allocation repair removed
redundant private initialization and added the one-cell path; it did not
remove these scans or general clone costs.

Accumulating glob illustrates that cost with larger batches, but its timing
is not a like-for-like comparison with fresh glob: the former retains
32,000 paths at 1,000 calls, the latter only the current 32-entry result.
Literal glob sugar also requires more configured heap than explicit fresh
glob in this fixture. Neither observation justifies discarding the
unchanged-on-error or alias-identity contract. No optimization algorithm was
selected or implemented during this evaluation.

### Costs that already exist on main

All repeated scalar regex-match and match-group workloads exhaust 64 MiB
before completing 4,000 iterations, on main **and** trial. A thousand
Boolean matches need about 33 MiB; a thousand group captures about 35 MiB.
This is not a new immutable-scratch regression.

The ordinary match path still allocates terminated pattern/subject copies
and recompiles with `regcomp9`; those allocations and the program are not
released when `sash_regex_match` returns. Capture-bearing paths additionally
retain result storage, so a safe ownership repair must distinguish
operation-only temporaries from data that survives in results.

The quiet `r?` path frees its candidate copy and compiled program. Validating
once or reconstructing and validating each iteration gives the same measured
heap bracket as literal matching, but does not cache the program for the
consumer. `rglob` separately owns and releases its compiled pattern batches;
that does not repair other regex entrypoints.

The existing [literal-regex reuse task](../tasks/dd39-precompile-literal-regex.md)
now carries this evidence. A literal cache and complete temporary/result
ownership are different problems; no cache design, garbage collector,
general reclamation scheme or engine replacement is selected here.

Fresh file captures, readers, keys and builder snapshots likewise retain
allocation costs already present on main. In these fixtures the trial has
identical configured-heap brackets. Fresh language instances do not imply
automatic destruction of all old backing storage.

## Contract accounting and remaining decision

The DD inventory was compared mechanically with both original recording
intervals and the implementation interval:

| Interval | Changed/new DD paths |
|---|---:|
| `bad3ff56` to selector recording `2a70e7dd` | 14 |
| `2a70e7dd` to combined recording `2e902f97` | 27 |
| Original main `6c7b899f` to trial `76376c4e` | 10 |
| Union, normalized to current filenames by DD number | 29 |

All 29 current paths are recorded by the integrated owner: no missing or
extra file. Its per-contract evidence table connects those paths to delivery
commits, practical results and costs. This is path/accounting evidence, not
DG's adoption disposition. All provisional groups still await that decision.

Ordinary const validation, strict non-aliasing, resultless assignment,
DD130's failure criterion, the coarse-glob choices and DG's guarded
redirect-index ruling have independent authority. Revising or withdrawing
the provisional package must reconcile individual sections, not restore
whole historical DD files over those decisions.

The AI's recommendation is to keep the trial off main while reconciling the
converter's silent sequential-fill change, and to address or explicitly
accept the measured append cost before adoption. Fresh-result use is a
useful demonstrated idiom, not permission to choose an automatic migration
policy on DG's behalf. DG has not adopted, revised or withdrawn the package.

## Evidence and retained gate

Private evidence is under
`files/glob-trial-20261117/evaluation/` in the session workspace:

- `functional-01` through `functional-04` retain practical, original/reviewed
  converter and adapted-program outcomes, including failed fixture attempts.
- `resource-results.json` consolidates 58 unique completed configurations
  from resource runs 01, 02, 03, 05, 06 and 07. Run 04 failed on an invalid
  fixture; `resource-smoke` is a preflight, not extra measurement rows.
- Sources, generated C, raw probes, compiler/runtime/link evidence,
  `dd-inventory.json` and retrospective evaluation provenance are retained.
  Successful run-end identity records agree. Partial early runs did not
  write those final manifests; later verification is labeled retrospective,
  not fabricated as a contemporaneous record.

Native comparisons use a session-only linker shim to select the matching
runtime archive, because the compiler embeds its live runtime path at build
time. Actual archive-open traces establish the old/new linkage. C programs
link the matching archive explicitly. The frozen main compiler/runtime and
the accepted trial source/compiler/runtime/lock manifests remain intact.

The early resource attempts are not hidden: the first driver did not yet
recognize the engine's OOM diagnostic; the keys-only baseline hit the
already-repaired nested-use traversal defect from journal 645; and two
reader fixtures used unsupported NUL/here-string spellings. The comparable
keys program now observes the source map's length, and the final reader uses
the real NUL file. Only completed, output-checked measurements enter the
consolidation.

No product or spec source changed for this evaluation. Journal 653's accepted
gate remains authoritative: rows 7246113-7258677, 12565 checks, 12552 passing,
13 existing C skips, no failures/flips/retirements, and 116 first-ever IDs.
All 12449 prior accepted statuses are unchanged. Its sixteen reversions are
restored. No new sweep, spec-history write or coverage run was needed merely
to document these measurements.
