# 162 — c parity (batch 5): function / multi-statement stdout capture

`$(f)`, `$(a; b)`, and `$(a | b)` captured into a var or argument were
unimplemented in c: `CaptureStart`/`CaptureEnd` were `/* TODO inst */`, so the
body ran straight to stdout and the capture was empty (mcs.3 → `a\nb\n[]`).

## Implementation

The runtime already had the primitives:
- `sash_capture_start() -> (saved_stdout, pipe_read)` — dup stdout aside,
  create a pipe, dup2 pipe-write onto stdout.
- `sash_capture_end(saved, pipe_read) -> (ptr, len)` — restore stdout, read
  the pipe (64KB), strip trailing newlines.

emit_c now:
- declares `__cap_saved` / `__cap_pipe` in `main` (the state between the two
  insts; single pair, no nesting — matching native's single capture vars);
- adds a `sash_ii { i64 a, b; }` typedef for `sash_capture_start`'s two-i64
  return, plus the two externs;
- handles `CaptureStart`/`CaptureEnd` in a new `emit_capture_inst`
  sub-dispatcher (alongside `emit_read_inst`/`emit_bless_inst`, keeping
  `emit_inst` under the complexity gate). `CaptureEnd` stores into the
  target: int → `sash_atoi`, else `_ptr`/`_len`. The target is declared via a
  `collect_inst_var_types` `CaptureEnd` arm.

Mirrors native `emit_capture_end` (dispatch.rs:1249). The body between the two
insts runs in-process writing to the pipe, so captures over ~64KB would block —
same limitation as native.

## Result

Full c sweep: **0 regressions, +4** (`c:17.3`, `c:17.4`, `c:mcs.3`,
`c:mcs.4`). Un-guarded functions.sh 17.3/17.4 and taint.sh mcs.3/mcs.4 to
`native,c`.

Only `c:38.5` / `c:55.19` (`$!` bg pid) remain failing; the remaining skipped
c tests are compound-pipeline (14/15/32.x), ivc.8 (local int coll), and the
native-only-by-design `$rerr` checkers.

## Files

- `sash/src/emit_c.rs` — `sash_ii` typedef + capture externs; `__cap_saved`/
  `__cap_pipe`; `emit_capture_inst`; `CaptureEnd` arm in
  `collect_inst_var_types`.
- `spec/functions.sh`, `spec/taint.sh` — guard widenings.
