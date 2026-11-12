title: argv/arg/argc trio — array a=$(argv) snapshot
status: done
priority: P1
kind: feature
depends: [argv-trio-codegen]
gates:
  - "`array a=$(argv)` lowers to a snapshot of pointers + lengths"
  - "`array a=$(argv N)` lowers to a slice snapshot starting at N"
  - resulting named array is tainted; DD#61 array-bless rules apply
    (`array a=$(argv)` is a DD#61 bulk-read construction source)
  - av.n1..n3 pass under native and c backends
  - independent snapshots: modifying one does not affect another

# argv/arg/argc trio — `array a=$(argv)` snapshot

Final positive-context use of the new array-returning builtins:
capture a copy into a named array for repeated iteration without
re-calling `$(argv)`.

## Why P1 and not P0

Bug 2 closure (T7) doesn't need this. The for-in path (T3) and
DD46 enforcement (T2) close the ICE. Snapshot is a quality-of-life
addition that may inform real user code patterns (one-time argv
parse, then reuse).

Can be deferred if T2..T5 timeline is tight; doesn't block the
bug closure.

## Implementation sketch

**IR**: extend the array-literal lowering at
`sash/src/transform/expressions.rs:107 lower_array_literal` to accept
an `$(argv)` / `$(argv N)` RHS as a special initializer form.

**SSA**: a snapshot instruction that allocates an array of
`(ptr, len)` pairs sized by the slice length and copies from
`sash_argv` + length lookups. Lives alongside today's array-literal
SSA shape.

**Codegen**: emit a fixed-size stack/heap allocation (size = (argc -
N) entries) and a loop copying from `sash_argv[i]` + `strlen` (or
the existing length helper) for i in N..argc. Or call out to a
runtime helper `sash_argv_snapshot(start_idx, out_array)` that does
the copy — likely cleaner.

**Runtime**: add `sash_argv_snapshot(start, dst)` in `sash-rt/src/lib.rs`
returning the entry count (argc - start). Mirrors `sash_argv_get`
shape.

**Taint**: the resulting array is tainted (every element came from
argv). `array a=$(argv)` is a DD#61 bulk-read construction source
(alongside `read -a` / `mapfile`), so it reuses the DD#61 array-bless
infrastructure. **Bless via bless-all**, not scalar bless — `bless a
[[…]]` on an array is now a DD#61 type error:

```bash
array a=$(argv)
bless x in a [[ $x =~ … ]]   # clears the whole snapshot
```

No new taint plumbing.

## Test gates

  - av.n1 `array a=$(argv); bless x in a [[ -n $x ]]; for x in @a; do echo $x; done`
    invoked as `./bin a b c` → program-path\na\nb\nc
    (snapshot → bless-all → iterate `@a`; iterating the tainted splat
    directly with a body-bless is rejected — see argv-trio-design
    "Consumption + taint")
  - av.n2 `array a=$(argv 1); bless x in a [[ -n $x ]]; echo ${#a[@]}`
    invoked as `./bin a b c` → 3
  - av.n3 `array a=$(argv 1); array b=$(argv); echo ${#a[@]} ${#b[@]}`
    invoked as `./bin a b` → 2 3 (counts need no bless; reading
    elements would)
  - av.n4 `array a=$(argv 1); bless x in a [[ -n $x ]]; echo $(join @a)`
    → DD#41 `join` on the blessed snapshot

## Cross-backend

  - `--janet` backend (T7 of this task chain doesn't change janet
    semantics; janet has its own native argv access). T4 should
    add a janet emit path or document non-support.
  - `--emit-c` backend mirrors the cranelift path; the C runtime
    has `argc`/`argv` natively.

## References

  - [argv-trio-design](argv-trio-design.md)
  - [argv-trio-codegen](argv-trio-codegen.md)
  - `sash/src/transform/expressions.rs:107 lower_array_literal`
  - `sash-rt/src/lib.rs:362 sash_argv_get` — sibling helper
