# 166 — c parity (batch 10): redirect open(2)-failure abort

The last piece of descriptive-abort. A failed redirect `open(2)` (the implicit
`exec N>file` that runs before the command) was ignored in c — `emit_open_redirect`
called `sash_redirect` and dropped the result, so `echo hi > /bad/f` misrouted
to the original stdout and printed. Native skips the body (no misroute) and
aborts, governed by context.

## Implementation

- `emit_open_redirect` now captures `(sash_redirect(...) < 0)` into a fresh
  guard var and returns `(failed, path_ptr, path_len)`; `emit_one_redirect`
  and a new `emit_redirects_guarded` propagate it (OR-folding multiple opens,
  keeping the first path for the message). `emit_redirects` stays a thin
  wrapper.
- `emit_command` splits: the body dispatch moved to `emit_command_body`, and
  on an open failure `emit_redirect_guarded_body` emits
  `if (failed) { status=1; [diag]; restore; [goto abort] } else { body; restore }`.
- **in_condition**: the `-` suppression and the if/|| condition both make
  `checked` false, but a redirect open must still abort under `-` while being
  caught by if/||. Added `Emit.in_condition`, set for the final command of a
  block that terminates in a Branch/Loop (mirrors native
  `in_condition = is_branch && idx == last_cmd_idx`). The guard aborts unless
  in_condition.

Verified: 55.24 bare aborts + `<argv0>: <path>: <reason>` diagnostic; 55.25
`if` → else (caught); 55.26 `- echo > /bad` still aborts; 55.30 `||` caught;
a valid redirect still writes the file.

## Result

Full c sweep: **0 regressions, +4** (55.24/25/26/30). descriptive-abort is
fully done (all 55.20-30 pass in c). **Zero failing c tests.**

## Files

- `sash/src/emit_c.rs` — `emit_open_redirect`/`emit_one_redirect` return the
  open-failure; `emit_redirects_guarded`; `emit_command` →
  `emit_command_body` + `emit_redirect_guarded_body`; `Emit.in_condition`
  (set in `emit_block`).
- `spec/errors.sh` — un-guard 55.24/25/26/30 to native,c.
