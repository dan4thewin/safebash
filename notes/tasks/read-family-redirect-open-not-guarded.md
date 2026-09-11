---
title: "bug: the read family does not abort on a failed redirect open"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`readline v < nosuchfile` aborts (DD#55), as `/bin/cat < nosuchfile` does"
  - "`read arr < nosuchfile` likewise, rather than yielding an empty array"
---

## What

DD#55 makes a failed redirect `open(2)` an abort — verified for an external:

```sh
/bin/cat < nosuch      # -> "nosuch: No such file or directory", aborts
```

The read family does not get that guard. Both of these succeed silently:

```sh
readline v < nosuch    # v = "", exit 0, no diagnostic
read arr < nosuch      # len 0,  exit 0, no diagnostic
```

So a mistyped path is indistinguishable from an empty file — the program
carries on with an empty value it believes it read.

## Why it is not the pipeline-stage bug

Separate from `bug-pipeline-stage-input-redirect-dropped` (fixed 2026-09-09,
commit b57e1ec): there the fd was wrong, so the redirect went to the stage's
stdout. Here the fd is right and the read works when the file exists — only
the **open failure** is unchecked.

Likely the same root as the note in `is_bulk_read_cmd`: the bulk reads lower
to SSA instructions that carry no redirect list, so the redirect is applied
around them by the caller, and that path does not run
`emit_redirects_guarded`.

## Notes

Pre-existing — `readline` behaves this way and was not touched by DD#99 stage
6. Found while restoring `read`'s redirect handling during the stage 6 rename.

## Resolution

Fixed 2026-09-11. The filing's guess at the root was right: `SsaInst::ReadArray`
carried no redirect list, so `read a < f` was bracketed by a
`RedirectPush`/`RedirectPop` pair built from the *unguarded* `emit_redirects`.
`emit_readline` had its own copy of the same omission.

Two options existed — guard inside `RedirectPush`, or give `ReadArray` its own
redirects the way `ReadLine` already has. The second won on the merits: SB4021
rejects redirects on compound commands at parse and SB4046 rejects them on
declarations/assignments, so `RedirectPush` is reachable *only* via bulk reads;
and push/pop bracket an arbitrary span of blocks with no body to skip, so the
"skip the body, flow the status to the condition" half of DD#55 cannot be
expressed there at all. (Internal-representation fork, decided and reported.)

Changes: `ReadArray` gained `redirects`, `attach_redirects_to_last_inst` gained
an arm, `is_bulk_read_cmd` dropped `"read"`, and both `emit_read_array` and
`emit_readline` now wrap their bodies in `emit_redirect_guarded_body` (native)
/ the new closure-taking `emit_guarded_body` (c). `codegen/entry.rs`'s
`emit_block_insts` only looked at `Command`/`Pipeline` when deciding
`in_condition`, so the reads never saw it — extended to the three read insts.

Behaviour, native and c agreeing:

| form | result |
|---|---|
| `read a < nosuch` | abort, `nosuch: No such file or directory`, rc 1 |
| `readline v < nosuch` | same |
| `- read a < nosuch` | still aborts (`-` binds the read, not the open) |
| `if read a < nosuch { } else { }` | caught → else, rc 0, len 0 |
| `read a < nosuch \|\| echo caught` | caught, rc 0, len 0 |

On the fail path the body never runs, so the target is never `def_var`'d in
that branch and the merge yields Cranelift's zero default — empty string, len
0. Verified, and the sensible value.

One divergence surfaced and was filed rather than fixed: a failed open in a
`while` **condition** aborts in native but is caught in c. It reproduces at
c1e9807 with a plain `echo`, so it predates this work, and whether a `while`
condition should catch is a DD#55 question for DG —
`dd55-while-condition-redirect-divergence`.

Spec: 55.60–55.66 in `spec/errors.sh` (55.60–55.64 gated `native,c`, since
janet has no DD#55 abort channel — `c-janet-dd55-abort-channel`).
