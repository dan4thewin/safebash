title: bug: contradictory usage strings, no --help, unknown flags silently ignored
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "one usage string, emitted from a single source of truth, used by every call site"
  - "`sash --help` / `-h` prints full usage on stdout and exits 0"
  - "usage covers every user-facing flag and agrees with docs/sash.1"
  - "`--no-taint-cst` is accepted but absent from the usage text"
  - "an unknown `--` flag is rejected with a diagnostic and a nonzero exit"
  - "`-o` / `-c` with a missing operand is an error, not a silent ignore"
  - "docs/sash.1 no longer lists the driver under 'silently ignores'"

# CLI: inconsistent usage message, no `--help`, unknown flags silently ignored

`sash`'s argument handling has three defects. The man page (`docs/sash.1`)
is complete and correct; the **binary** is what disagrees with it.

## 1. Two different usage strings

`sash/src/lib.rs` prints a different usage depending on how you fail:

| trigger | line | text |
|---|---|---|
| no args at all | 73 | `sash <script.sh> [-o <output>] [--janet] [--manifest] [--extract <binary>]` |
| flags but no input | 382 | `sash [-c 'script'] [-o output] [--janet] <script.sh>` |
| `--extract` misuse | 321 | `sash --extract <binary>` |

Measured:

    $ sash            -> rc=1  Usage: sash <script.sh> [-o <output>] [--janet] [--manifest] [--extract <binary>]
    $ sash --help     -> rc=1  Usage: sash [-c 'script'] [-o output] [--janet] <script.sh>

Line 73 advertises `--manifest`/`--extract` but not `-c`; line 382
advertises `-c` but not `--manifest`/`--extract`. Neither mentions
`--emit-c`, `--janet-map`, `--no-taint`, `--trace`, or
`--sysroot=`.

## 2. No `--help`

`--help` is not handled. It is collected as an unknown flag, leaves no
positional input, and falls into the line-382 error branch — **stderr, exit
1**. There is a full man page but no way to get help out of the binary.

## 3. Unknown `--` flags are silently ignored

`parse_cli_args` (`sash/src/lib.rs:~373`) collects *any* `--` token:

```rust
s if s.starts_with("--") => flags.push(s),
```

`apply_and_check_flags` then only rejects the three converter-only flags
(DD#57). Everything else is carried along and never looked at, so a typo
silently produces **a different artifact than the one requested**:

    $ sash --emit-C h.sash   # capital C
    sash: h.sash -> h.sash.out (static binary)     rc=0

    $ sash --jannet h.sash
    sash: h.sash -> h.sash.out (static binary)     rc=0

The user asked for C source and for a Janet build; both got a native ELF,
exit 0, no warning.

Related: a missing operand is also swallowed —

    $ sash h.sash -o
    sash: h.sash -> h.sash.out (static binary)     rc=0

(`-c` with no operand does exit 1, so only `-o` is affected.)

## Not a deliberate behaviour

`docs/sash.1` (~line 405) lists the driver in a table of *inconsistencies*
in unknown-flag handling, alongside `read` (rejects — SB5015) and
`sash-convert` (rejects):

> `sash` (the driver) silently ignores: `sash --bogus script.sh` compiles
> and exits 0.

It is recorded as an inconsistency, not a design. No DD blesses it. The
established convention in this codebase is to reject: `read` rejects,
`sash-convert` rejects, and `apply_and_check_flags` already rejects the
DD#57 converter flags with `exit 1` — so rejection has a working precedent
to extend rather than a new mechanism to invent.

That man-page entry also cross-references
`notes/tasks/switch-free-read-family.md` as "an open decision"; that task is
`status: done`. The stale reference goes with the fix.

## Real flag inventory (11)

Verified against `sash/src/lib.rs`. This is what the usage text must cover:

| flag | note |
|---|---|
| `-c 'script'` | inline source |
| `-o output` | output path |
| `--janet` | Janet backend (writes to **stdout**) |
| `--janet-map` | |
| `--emit-c` | C backend (writes to **stdout**) |
| `--manifest` | |
| `--extract binary` | dump `.sash_source` |
| `--no-taint` | bypass — documented |
| `--no-taint-cst` | testing aid only, **not advertised** in usage |
| `--trace` | |
| `--sysroot=path` | prefix-matched, not an exact token |

`--emit-source`, `--annotate`, `--nolint` are **sash-convert** flags and are
deliberately rejected by `sash` (DD#57). They stay rejected; the diagnostic
is already good. Note `AGENTS.md` currently describes `--emit-source` and
`--annotate` as `sash` modes — that line is stale and should be corrected
with this task.

## Complexity gate will fire on this commit

`parse_cli_args` (`sash/src/lib.rs`) is currently **cc=11, cog=10**, delta=−1.
The gate rejects at cog > 6.58 (3× mean) for any function a commit touches,
and delta=−1 is nowhere near the −15 dispatcher exemption. This task
modifies `parse_cli_args` by definition, so it **must** bring it under the
bar in the same commit. Do not reach for `SKIP_COMPLEXITY_GATE=1`.

Two other lib.rs functions are also in the cohort and may be touched:
`sysroot_file_exists` (8/8) and `dump_sash_source` (7/7).

Adding a known-flag table and an unknown-flag rejection should *lower*
`parse_cli_args` if the flag set becomes data (a `const` slice of known
flags) rather than more match arms.

## Scope

1. Hoist the usage text to one constant; use it at all three call sites.
2. Handle `--help` / `-h`: full text on **stdout**, exit **0**.
3. Validate flags against the known set; reject unknown with a diagnostic
   and nonzero exit, extending `apply_and_check_flags`. Keep `--sysroot=`'s
   prefix match working.
4. Error on a missing `-o` operand.
5. Update `docs/sash.1`: drop the driver row from the "silently ignores"
   table and the stale `switch-free-read-family` cross-reference.
6. Spec coverage for: unknown flag rejected, `--help` exits 0, missing `-o`
   operand rejected.

## Decided (DG, 2026-09-12): the two bypass flags are not peers

`--no-taint` **is documented** in the usage text — it is a supported user
flag.

`--no-taint-cst` is a **testing aid only, not for general use**. It stays
out of the binary's usage output. It is still accepted, and still rejects
unknown flags around it; it is simply not advertised. `docs/sash.1` may keep
describing it — the man page is the reference for people working on the
compiler — but it must say plainly that it is a testing aid and not for
general use.

This means the usage text lists **10** flags, not 11.
