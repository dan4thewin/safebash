title: "bug: [[ -N file ]] disagrees with bash when mtime equals atime"
status: done
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

## DONE (2026-09-12)

Fixed as filed — `sash-rt/src/lib.rs` op 17 is now `st.st_mtime >=
st.st_atime`, nanoseconds dropped. New spec `spec/filetest.sh`, 28 tests.

### Bash's rule, measured rather than assumed

A four-case differential run settled what bash actually does. The decisive
case is the fourth:

| case | mtime vs atime | bash |
|---|---|---|
| fresh write, never read | equal to the nanosecond | yes |
| mtime > atime | greater | yes |
| mtime < atime | less | no |
| same second, **atime nsec greater** | mtime nsec *lower* | **yes** |

Case 4 answers "yes" even though atime's nanoseconds exceed mtime's. Had bash
compared with nanosecond precision under `>=` it would have said no. So bash
truncates to whole seconds and never consults nanoseconds — which is what the
fix now does. Pre-fix, sash failed cases 1 and 4; the filing had predicted
only case 1.

### The sweep found a second, larger bug

The filing's claim that `-N` is "the **only** divergence in the whole set" was
true only of the native backend. Re-running the sweep under `--emit-c`, **all
17 "yes" cases failed and every "no" case passed** — the C backend was
answering *false to every file test*, including `-e` and `-f`.

Cause: `emit_c.rs::lower_test_unary` had a catch-all arm that wrote a comment
and returned the constant `0`:

```rust
_ => {
    writeln!(e.out, "    /* TODO test unary op: {} */", op).unwrap();
    "0".to_string()
}
```

The `sash_file_test` extern was already declared in the emitted C and the
runtime function already existed; only the call was missing. Not a deliberate
deferral — `add-c-oracle.md` lists file tests as in scope for the C backend.

This was worse than the `-N` bug it was found alongside. `-N` was wrong in two
corners; this silently mis-compiled *every* `[[ -f ]]` / `[[ -e ]]` / `[[ -d ]]`
in the C backend, and a stub that returns a plausible value instead of failing
is invisible until something differentially tests it.

Fixed in the same pass:

- `codegen::file_test_op_code` is now `pub(crate)` and re-exported from
  `codegen/mod.rs`. `emit_c` calls **that** table rather than carrying a
  second copy — a duplicated opcode table is how two backends drift.
- Added `-t` (the extern `sash_isatty` was missing from emit_c's preamble;
  `sash_atoi` was already there).
- The catch-all now **panics** with the same ICE message native uses, instead
  of emitting a constant. Unreachable in practice — the parser's
  `is_unary_test_op` gates the set and validate rejects `-v`/`-o`/`-R`
  (SB5028) — but a backend that cannot lower an operator must fail loudly.

### Verification

Fail-first, native, reverting only `sash-rt`: exactly `ft.1`, `ft.2`, `ft.5`
fail — the fresh-file, exact-equality and nanosecond cases. `ft.3`/`ft.4`/
`ft.6` pass both before and after, so they stand as regression guards.

Fail-first, C backend: 17 of 28 failed before the `emit_c` fix.

`spec/filetest.sh` is guarded by section. Janet lowers only `-d`/`-e`/`-f`
(measured — the other 14 operators are `unknown symbol` compile errors), so
section B runs in all three modes and sections A and C are `native,c`. That
janet gap is pre-existing and separate; it is **not** the same class of bug,
because janet fails to compile rather than silently answering false.

### Method note

Both bugs came from differential sweeping, not code reading, and the second
one only surfaced because the sweep was re-run under every backend. A sweep
that covers one backend proves one backend.
