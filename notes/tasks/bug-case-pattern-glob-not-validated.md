---
title: BUG — case patterns ? and [...] silently never match (validator + runtime gap)
status: pending
priority: P0
kind: bug
depends: []
gates:
  - case patterns with ? or [...] either (a) get SB5006 at compile time, or (b) match correctly at runtime
  - control_flow.sh cf.c7 and cf.c8 pass under all 3 modes
  - SB5006 documentation updated to clarify which contexts it covers
---

# BUG — case patterns ? and [...] silently produce no-match

> **Superseded by DD#66 (2026-06-19).** `case`/`esac` and glob string-
> matching are being **removed** — `case` becomes the `[[ … ]]` dispatch
> form (`==` exact / `=~` regex). This bug dissolves: there is no `case`
> glob to validate or mis-match. The earlier fix-(a) (extend SB5006 to
> case patterns) is moot. If DD#66 is deferred, the validator one-liner
> remains the interim fix; otherwise close this on DD#66 landing. cf.c7/c8
> get rewritten/removed by `dd66-cond-form-dispatch`.

## Discovery

Found while writing `spec/control_flow.sh` (cf.c7, cf.c8). Adjacent
to the while-loop bug; same investigation surfaced both.

## Repro

```bash
$ bash -c 'x=ab; case $x in ?b) echo two-char-b ;; *) echo other ;; esac'
two-char-b

$ bash -c 'x=5; case $x in [0-9]) echo digit ;; *) echo other ;; esac'
digit

$ target/release/sash -c 'x=ab; case $x in ?b) echo two-char-b ;; *) echo other ;; esac' -o /tmp/t && /tmp/t
other

$ target/release/sash -c 'x=5; case $x in [0-9]) echo digit ;; *) echo other ;; esac' -o /tmp/t && /tmp/t
other
```

Sash silently falls through to the default arm — no validator error
and no runtime match. The compiled program produces semantically
wrong output.

## Root cause (two-part)

1. **Validator gap.** `validate.rs::check_unquoted_glob` (line 708)
   correctly flags `?` and `[...]` with SB5006, but it is only
   called from `validate_for` (line 176). Case-arm patterns never
   get checked.

2. **Runtime gap.** `sash-rt/src/lib.rs::fnmatch_impl` (line 2303)
   only implements `*`. The header comment claims "`?`, `[...]`,
   `\\` are rejected at compile time (SB5006)" — but they're not
   rejected for case patterns, so they reach runtime and silently
   never match.

## Decision — fix (a) [DG confirmed 2026-06-18]

> **Resolved direction.** DG: "globs support was intentionally simplified
> to `*` only." So `?` and `[...]` are **not** implemented — they are
> rejected at compile time (SB5006), matching the `fnmatch_impl` header
> comment and the `for`-loop path. The fix is the **validator gap only**;
> the runtime `fnmatch_impl` (`*`-only) is correct by design. **Timing
> open:** fix now vs. fold into the DD#63 `case`-glob desugar (which is
> still glob/`*`-only, just routed through a builtin — the SB5006 check
> applies either way).

Original options — (a) chosen, (b) rejected:

- **(a) Extend SB5006 to case patterns.** Reject `?` and `[...]`
  in case arms at compile time. Matches the existing claim in
  the fnmatch comment. Loses bash compatibility for these
  patterns but is consistent with sash's "subset of bash"
  philosophy.

- **(b) Implement `?` and `[...]` in fnmatch_impl.** Match bash
  exactly. Reverses the design intent stated in the fnmatch
  header comment. Larger code change in sash-rt.

The existing design intent (per the fnmatch comment) is (a). The
implementation is half-done: validator missed case patterns. The
minimum fix is to call `check_unquoted_glob` on each case pattern
in `validate.rs`. Confirm with DG before fixing.

## Expected behavior (after fix-a)

```
$ sash -c 'x=ab; case $x in ?b) echo ok ;; esac'
-c:1:18: error [SB5006] ? glob is not supported in sash — only * is allowed: ?b
```

## Test coverage to add once fixed

`spec/control_flow.sh` cf.c7 / cf.c8 already document the
behavior. Should flip from failing → passing with either fix.
Add a third test: bare `?` and bare `[abc]` in case patterns
should error with SB5006.

## References

- AGENTS.md "Bug policy" — zero tolerance
- DD#? — SB5006 design (subset-of-bash glob)
- `sash/src/validate.rs:708` — check_unquoted_glob (incomplete)
- `sash-rt/src/lib.rs:2303` — fnmatch_impl (only `*`)
- spec.db `native:cf.c7`, `native:cf.c8`
