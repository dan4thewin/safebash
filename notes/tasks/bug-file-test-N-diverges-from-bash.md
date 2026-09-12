title: bug: [[ -N file ]] disagrees with bash when mtime equals atime
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`[[ -N f ]]` on a file whose mtime equals its atime returns true, as bash does"
  - "the whole file-test operator set is swept against bash with zero divergences"
  - "spec coverage for `-N` in all three modes"

# bug: `[[ -N file ]]` disagrees with bash

Found 2026-09-12 while reviewing `unary-test-ops-k-g-u`, by sweeping every
file-test operator against bash.

`-N` ("file modified since last read") is the **only** divergence in the
whole set — the other 17 operators match bash exactly on a plain file, an
empty file, and a directory.

    $ printf 'x\n' > /tmp/plain.txt
    $ cat t.sash
    if [[ -N /tmp/plain.txt ]]; then echo yes; else echo no; fi

    $ ./t.bin      # sash
    no
    $ bash t.sash
    yes

Accepted, compiled, silently wrong — no diagnostic. This is the
"accepted-but-divergent" class that `fuzz-validator` calls out as the one the
adversarial audit's threat model misses.

## Root cause

`sash-rt/src/lib.rs:2446`, op code 17:

```rust
if st.st_mtime > st.st_atime
    || (st.st_mtime == st.st_atime && st.st_mtime_nsec > st.st_atime_nsec)
```

A freshly written, never-read file has mtime and atime **exactly equal** —
same second *and* same nanosecond:

    $ stat --printf='mtime_ns=%.9Y\natime_ns=%.9X\n' /tmp/plain.txt
    mtime_ns=1789181638.450736895
    atime_ns=1789181638.450736895

so both disjuncts are false and sash returns 0. Bash returns true here.

Two differences from bash at once:

1. **Strict `>` where bash uses `>=`.** Equal timestamps are the common case
   for a file that has been written and not read — precisely the case `-N` is
   meant to report true.
2. **Nanosecond precision where bash compares whole seconds.**

## Fix

Match bash: compare whole seconds with `>=`, i.e. `st.st_mtime >=
st.st_atime`, dropping the nanosecond tiebreak. "Byte-identical to Bash" is
the project's stated contract, so bash's coarser comparison is the
specification, not a thing to improve on.

Rebuild `sash-rt` (`cd sash-rt && cargo build --release`) — the c/native
backends link `libsash_rt.a`, so an `--emit-c` link without a rebuild reports
undefined references.

## Note on method

Found by differential sweep, not by reading code: 18 operators × 3 targets,
comparing sash output to bash output for each. That sweep is cheap and worth
keeping as a spec — see the second gate.
