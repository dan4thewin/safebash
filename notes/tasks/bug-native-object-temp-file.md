title: "BUG: the native driver's temporary object `<output>.o` panics on a failed write or ld spawn, and overwrites, then deletes, whatever already has that name"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "no input makes `sash` panic while writing the object or starting `ld`: each failure is one `sash:` diagnostic and exit 1, as `read_source` does"
  - "a failure leaves no object behind, and `sash` never deletes or writes through a file it did not create"
  - "a pre-existing `<output>.o`, regular file or symlink, is untouched by a compile"
  - "every new spec fails on the pre-fix binary (its negative control)"

## What

B55 in `batch-531-findings.md`. It went to the backlog on 2026-10-26 under
DG's rule for the port: the branch does not fix it, and nothing in the port
depends on it.

`compile_native` (`sash/src/lib.rs:1028-1057`) names the object
`format!("{}.o", output)` (`:1038`) and writes it with
`std::fs::write(..).expect("write .o")` (`:1052`). `link_object`
(`:1000-1026`) then runs `Command::new("ld")…status().expect("ld failed")`
(`:1018`), removes the object (`:1020`), and reports a nonzero ld status as
`sash: linking failed`, exit 1 (`:1022-1025`, pinned by `cli.45`). With no
`-o`, `native_output_path` (`:979-991`) puts the output beside the source
(`foo.sh` -> `foo`), so the object is `foo.o` there.

Measured 2026-10-26 with `echo yes` as `m.sh`. The first seven rows were run
on HEAD b62c8a7, base 1309385 and branch e98fca8, with identical results; the
rest on HEAD only, where `git diff` shows these sites unchanged since base.

| case | result |
|---|---|
| `-o ro/x`, `ro` mode 555 | panic `write .o: Permission denied`, rc 101 |
| `-o missing/x` | panic (ENOENT), rc 101 |
| `-o d`, with `d.o` a directory | panic (EISDIR), rc 101 |
| `ro/m.sh` with no `-o` | panic, rc 101 |
| `PATH` without `ld` | panic at `ld failed`, rc 101, `x.o` left behind |
| `PATH` with a non-executable `ld` | the same, `x.o` left behind |
| `-o blocked`, a directory (control) | `sash: linking failed`, rc 1, object removed |
| `prlimit --fsize=512`, SIGXFSZ ignored | panic (EFBIG), rc 101, a 512-byte `x.o` left behind |
| `x.o` an existing regular file | overwritten, then deleted; rc 0 |
| `x.o` a symlink to a user file | the object is written through the link into the target, then the link is deleted; the target keeps the object bytes; rc 0 |

The last two rows have one cause: the object's name is fixed and it is opened
with a plain create-and-truncate. A directory holding `foo.c`, built to
`foo.o`, and `foo.sh` loses `foo.o` to `sash foo.sh`. Two compiles to the same
output also race on one object.

## Precedents

- `read_source` (`lib.rs:935-944`): `sash: <path>: <error>`, exit 1. Every
  CLI failure in `lib.rs` exits 1.
- `read_squashfs_listing` (`lib.rs:158-179`): a spawn failure is
  `unsquashfs is not installed` for ENOENT and
  `could not run unsquashfs: <error>` otherwise. `cli.41` and `cli.43` pin
  both; a present but non-executable tool is not a missing installation.
  The ld spawn can share one helper with it.
- `sash-convert/src/main.rs:64-68`: a failed output write is
  `sash-convert: <path>: <error>`, exit 1.

## Design notes (2026-10-26 research; not yet discussed with DG)

1. ld spawn: take the `Result`, remove the object first, then report
   `sash: linking failed: ld is not installed` or
   `sash: linking failed: could not run ld: <error>`. Keep
   `sash: linking failed` for a nonzero status.
2. The object: recommended is a fresh, uniquely named file created with
   `O_CREAT|O_EXCL` in the output's directory. `O_EXCL` also refuses a
   symlink in the final component. Nothing visible changes except that
   nothing is clobbered. An open failure reports the error and touches
   nothing; a write failure removes the partial object and reports.
   Alternatives, each with a visible cost:
   - keep `<output>.o` with `O_EXCL`, which refuses to compile when it exists;
   - `$TMPDIR`;
   - no file at all: a memfd handed to ld as `/dev/fd/N`. This is Linux
     only; see `q-runtime-syscall-layer` and `cross-compile-and-setjmp-on-mac`.
   If DG prefers an alternative, the specs change with it (the `d.o`
   directory row stops failing under a unique name).

## Specs

- In `spec/cli.sh`, which runs in both modes, so each check counts twice in
  the sweep. Take the id high-water mark from `spec.db` (it was `cli.45` on
  2026-10-26).
- Prefer shapes that hold under root: ENOENT (a missing directory) and
  EISDIR. EACCES from `chmod` does not hold under root.
- A deterministic write failure:
  `( trap '' XFSZ; exec prlimit --fsize=512 "$SASH" m.sh -o d/x )` gives
  EFBIG on the object. `prlimit` is already a required suite tool.
- Assert:
  - rc 1;
  - the message;
  - no `SB[0-9]{4}` (`grep -cE`, never a bare `SB`);
  - what is left in the directory;
  - that a pre-existing `x.o`, or a symlink's target, is untouched.

## Related

- `bug-stdout-write-failure-panics`: the success line (`:1056`) panics when
  stdout fails, after the binary is built.
