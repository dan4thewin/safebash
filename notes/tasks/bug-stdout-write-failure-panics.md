---
title: "BUG: sash and sash-expand panic (rc 101) when a write to stdout fails, even after the binary is built"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "no failed stdout write makes `sash` or `sash-expand` panic"
  - "DG has ruled whether EPIPE is quiet or reported like any other error, and the ruling is recorded where the handling lives"
  - "specs pin EPIPE and ENOSPC for `--emit-c` and for the success line, each with a negative control on the pre-fix binary"
---

## What

Found while researching B55 (`bug-native-object-temp-file`); not on the
batch-531 branch.

Every stdout write in sash's CLI is a `print!` or `println!`, which panics
when the write fails:

- `sash/src/lib.rs:550`: usage (`check_flags`);
- `:757`: `--emit-ir` (`maybe_emit_alt_output`);
- `:786`: `--emit-c` (`emit_c_to_stdout`);
- `:815`: `--extract` (`dump_sash_source`);
- `:972`: `--manifest` (`emit_manifest`);
- `:1056`: the success line `sash: m.sh -> x (static binary)` (`compile_native`).

`sash-expand/src/main.rs:101` (`writeln!(out, …).unwrap()`) has the same
failure.

Measured 2026-10-26 on HEAD b62c8a7 (base and branch share these sites):

- `(sleep 0.5; exec sash --emit-c m.sh) | true`: the panic
  `failed printing to stdout: Broken pipe`, rc 101.
  - Rust ignores SIGPIPE, so a write after the reader has gone fails with
    EPIPE instead of killing sash.
  - Without the delay a small program's C fits in the pipe buffer and
    the write succeeds first, so `… | head -c1` usually exits 0.
- The same with `sash m.sh -o v`: `v` is built, then rc 101.
- `--emit-c`, and the success line, to `/dev/full`: the same panic (ENOSPC),
  rc 101.
- `--emit-c >&-`: rc 0, no output and no diagnostic. Rust's std treats a
  closed stdio descriptor (EBADF) as a sink.

## Precedent

`sash-convert/src/main.rs:70-75` reports `sash-convert: stdout: <error>` and
exits 1, for every error including EPIPE. Following it settles everything
here except the question below.

## Question for DG

1. Should EPIPE be quiet, as pipelines cut short by `head` usually expect?
   That would mean exiting without a diagnostic, either by restoring
   SIGPIPE's default action or by checking explicitly.
2. Or should EPIPE be reported like every other error, as sash-convert does?
3. Should a closed stdout be an error rather than a silent sink?
